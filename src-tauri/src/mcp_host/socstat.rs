//! socstat-mcp 统计服务器宿主（tauri State）。
//!
//! [socstat-mcp](https://crates.io/crates/socstat)（`crates/socstat` 的可选
//! 子 crate，rmcp 实现）以 stdio 子进程方式托管：应用启动时连接待机、
//! 退出时停机（生命周期见 [`super::process`]）。已发现工具（数据集管理、
//! 数据变换、描述统计、各类检验与回归等约 35 个）注册进 `data_analyst`
//! 工具集，使数据分析助手获得与前端数据面板同源的统计能力。
//!
//! ## 状态边界
//!
//! 数据集驻留在 MCP 服务器进程内存（`load_dataset` 后按名复用），
//! 与前端数据面板（主进程 socstat 直连）互相独立、互不可见。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use referee_agent::tool::mcp::TransportConfig;
use referee_agent::tool::McpServerConfig;
use referee_ai::tool::Tool;

use super::process::McpProcess;

/// 显式指定 socstat-mcp 可执行文件路径的环境变量（覆盖自动定位）。
const BIN_ENV: &str = "FLUEN_SOCSTAT_MCP_BIN";
/// 结果行上限：相关矩阵 / PCA 等结果的 JSON 可超 referee 默认 128 KiB。
const MAX_LINE_LEN: usize = 4 * 1024 * 1024;
/// 单次统计请求超时：大数据集的多元分析可超 referee 默认 30s。
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// socstat-mcp 服务器宿主（tauri State，随应用启动 / 退出管理）。
///
/// `Clone` 仅克隆句柄——底层连接与子进程全应用共享一份。
#[derive(Clone)]
pub struct SocstatMcpHost {
    process: Arc<McpProcess>,
}

impl SocstatMcpHost {
    /// 创建宿主并解析服务器配置（不连接；连接见 [`Self::warm`]）。
    pub fn new() -> Self {
        // 二进制缺失时回退 PATH 查找，再失败由 McpProcess 降级（空工具集）
        let command = resolve_server_binary().unwrap_or_else(|| "socstat-mcp".to_string());
        let mut config = McpServerConfig::new(command, "fluen", env!("CARGO_PKG_VERSION"));
        config.transport = TransportConfig {
            max_line_len: MAX_LINE_LEN,
            request_timeout: REQUEST_TIMEOUT,
            ..TransportConfig::default()
        };
        Self {
            process: Arc::new(McpProcess::new("socstat-mcp", config)),
        }
    }

    /// 启动期预热：提前建立连接与工具发现（失败仅记日志，不阻塞启动）。
    pub async fn warm(&self) {
        self.process.warm().await;
    }

    /// 已发现工具快照（连接失败降级为空列表）。
    pub async fn tools(&self) -> Vec<Arc<dyn Tool>> {
        self.process.tools().await
    }

    /// 应用退出时优雅停机子进程。
    pub async fn shutdown(&self) {
        self.process.shutdown().await;
    }
}

impl Default for SocstatMcpHost {
    fn default() -> Self {
        Self::new()
    }
}

/// 定位 socstat-mcp 可执行文件，按优先级尝试：
///
/// 1. 环境变量 [`BIN_ENV`]（显式覆盖）
/// 2. 可执行文件同目录的 sidecar（Tauri `externalBin` 布局，
///    文件名带 target-triple 后缀）及裸名
/// 3. 开发布局：cargo workspace 共享 target 目录下的各 profile
///    （本应用与 socstat-mcp 同属根 workspace，产物同根）
///
/// 全部未命中返回 `None`（调用方回退 PATH 查找）。
fn resolve_server_binary() -> Option<String> {
    if let Ok(path) = std::env::var(BIN_ENV) {
        if !path.trim().is_empty() {
            return Some(path);
        }
    }
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    binary_candidates(&exe_dir)
        .into_iter()
        .chain(dev_binary_candidates(&exe_dir))
        .find(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned())
}

/// 已安装布局候选：sidecar（带 target-triple 后缀）与裸名。
fn binary_candidates(exe_dir: &Path) -> Vec<PathBuf> {
    let suffix = std::env::consts::EXE_SUFFIX;
    vec![
        exe_dir.join(format!("socstat-mcp-{}{suffix}", env!("FLUEN_TARGET_TRIPLE"))),
        exe_dir.join(format!("socstat-mcp{suffix}")),
    ]
}

/// 开发布局候选：`<workspace>/target/{release,debug}/socstat-mcp`。
///
/// dev 下本应用可执行文件位于 `<workspace>/target/<profile>/`，与
/// socstat-mcp 的构建产物同属共享 target 根，向上一级即得。
fn dev_binary_candidates(exe_dir: &Path) -> Vec<PathBuf> {
    let suffix = std::env::consts::EXE_SUFFIX;
    match exe_dir.ancestors().nth(1) {
        Some(target_root) => ["release", "debug"]
            .iter()
            .map(|profile| {
                target_root
                    .join(profile)
                    .join(format!("socstat-mcp{suffix}"))
            })
            .collect(),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluen_mcp_host_{tag}_{}_{:?}_{}",
            std::process::id(),
            std::thread::current().id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn installed_candidates_use_sidecar_naming_first() {
        let dir = temp_dir("installed");
        let candidates = binary_candidates(&dir);
        assert_eq!(candidates.len(), 2);
        assert!(candidates[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(env!("FLUEN_TARGET_TRIPLE")));
    }

    #[test]
    fn dev_candidates_resolve_existing_profile() {
        // 模拟 workspace 布局：<root>/target/debug/（应用）+ <root>/target/release/（socstat-mcp）
        let root = temp_dir("dev");
        let exe_dir = root.join("target/debug");
        let release = root.join("target/release");
        fs::create_dir_all(&exe_dir).unwrap();
        fs::create_dir_all(&release).unwrap();

        let suffix = std::env::consts::EXE_SUFFIX;
        let binary = release.join(format!("socstat-mcp{suffix}"));
        fs::write(&binary, b"mock").unwrap();

        let hit = dev_binary_candidates(&exe_dir)
            .into_iter()
            .find(|p| p.exists())
            .unwrap();
        assert_eq!(hit, binary);
    }

    #[test]
    fn resolve_falls_back_to_none_when_absent() {
        let dir = temp_dir("absent");
        assert!(binary_candidates(&dir).into_iter().all(|p| !p.exists()));
    }

    /// 端到端：与真实 socstat-mcp 二进制连接（referee 客户端 ↔ rmcp 服务器
    /// 协议兼容性验证）。二进制不存在时跳过（CI / 未构建环境不失败）。
    #[tokio::test]
    async fn connects_to_real_socstat_mcp_when_binary_present() {
        let binary = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../target/release")
            .join(format!("socstat-mcp{}", std::env::consts::EXE_SUFFIX));
        if !binary.exists() {
            eprintln!("skip: socstat-mcp 未构建（{}）", binary.display());
            return;
        }
        std::env::set_var(BIN_ENV, binary.to_string_lossy().to_string());

        let host = SocstatMcpHost::new();
        host.warm().await;
        let tools = host.tools().await;
        assert!(!tools.is_empty(), "socstat-mcp 应发现统计工具");
        assert!(
            tools.iter().any(|t| t.name() == "load_dataset"),
            "工具面应包含 load_dataset"
        );
        host.shutdown().await;
    }
}
