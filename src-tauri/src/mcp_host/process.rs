//! 单个 MCP 服务器子进程的生命周期包装。
//!
//! 包装 referee [`McpServer`]（stdio 桥）：`connect`（spawn 子进程 +
//! discover + tools/list）→ 待机（工具快照被多次克隆共享）→
//! [`McpProcess::shutdown`]（关 stdin → 等待退出 → 超时 kill）。
//!
//! ## 连接语义
//!
//! 进程生命周期内**只连接一次**（与「应用启动时开启、待机」一致）：
//! - 连接幂等：并发调用共享同一次 `connect`，成功后不再协议往返；
//! - 失败降级：连接失败不抛给调用方——缓存失败原因并返回空工具集
//!   （如 `data_analyst` 退化为无统计工具，不阻塞聊天主链路），
//!   直到应用重启才会重试。

use std::sync::Arc;

use referee_agent::tool::{McpServer, McpServerConfig};
use referee_ai::tool::Tool;
use tokio::sync::OnceCell;

/// 一个 MCP 服务器子进程的生命周期管理器。
pub struct McpProcess {
    /// 日志与错误信息中的服务器标识。
    name: &'static str,
    config: McpServerConfig,
    /// 唯一一次连接的结果（`Err` 缓存失败原因，避免反复 spawn）。
    conn: OnceCell<Result<Arc<McpServer>, String>>,
}

impl McpProcess {
    /// 创建生命周期管理器（不连接；连接在首次 [`Self::tools`] /
    /// [`Self::warm`] 时发生）。
    pub fn new(name: &'static str, config: McpServerConfig) -> Self {
        Self {
            name,
            config,
            conn: OnceCell::new(),
        }
    }

    /// 已发现工具快照；未连接则先连接（失败降级为空列表）。
    pub async fn tools(&self) -> Vec<Arc<dyn Tool>> {
        let config = self.config.clone();
        match self
            .conn
            .get_or_init(move || async move {
                tracing::info!(server = self.name, "mcp 服务器连接中");
                McpServer::connect(config)
                    .await
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            })
            .await
        {
            Ok(server) => server.tools().to_vec(),
            Err(err) => {
                tracing::warn!(server = self.name, %err, "mcp 服务器连接失败，本次运行降级为无工具");
                Vec::new()
            }
        }
    }

    /// 启动期预热：提前建立连接与工具发现（失败仅记日志，不阻塞启动）。
    pub async fn warm(&self) {
        let _ = self.tools().await;
    }

    /// 优雅停机：关 stdin 让服务器自行退出，5s 未退出则 kill（referee 语义）。
    /// 尚未连接（或连接失败）时为空操作——子进程本就不存在。
    pub async fn shutdown(&self) {
        if let Some(Ok(server)) = self.conn.get() {
            server.shutdown().await;
            tracing::info!(server = self.name, "mcp 服务器已停机");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dead_process(name: &'static str) -> McpProcess {
        McpProcess::new(
            name,
            McpServerConfig::new("fluen-mcp-nonexistent-binary", "fluen-test", "0.0.1"),
        )
    }

    #[tokio::test]
    async fn connect_failure_degrades_to_empty_tools() {
        let process = dead_process("dead");
        assert!(process.tools().await.is_empty());
    }

    #[tokio::test]
    async fn failure_is_cached_without_retry() {
        let process = dead_process("cached");
        assert!(process.tools().await.is_empty());
        // 失败结果被缓存：二次访问直接降级，不重复 spawn
        assert!(process.tools().await.is_empty());
    }

    #[tokio::test]
    async fn shutdown_without_connection_is_noop() {
        let process = dead_process("noop");
        process.shutdown().await;
    }
}
