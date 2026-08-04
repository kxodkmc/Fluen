use thiserror::Error;

/// 知识库模块统一错误类型。
///
/// 设计遵循可恢复性分类：
/// - `NotFound`：条目/文献不存在，调用方应检查输入
/// - `Invalid`：参数非法（如错误的 wikiID 格式、不支持的 type）
/// - `Io`：文件系统错误
/// - `Db`：SQLite 操作错误
/// - `Embedding`：向量生成失败（不阻塞主流程，仅降级）
/// - `Internal`：其他未分类错误
#[derive(Debug, Error)]
pub enum KnowledgeError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid: {0}")]
    Invalid(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("db error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("embedding error: {0}")]
    Embedding(String),

    #[error("internal: {0}")]
    Internal(#[from] anyhow::Error),

    /// 异步运行时错误（spawn_blocking join 失败等）。
    #[error("async runtime error: {0}")]
    AsyncRuntime(String),

    /// MCP 协议错误（JSON-RPC 解析、未知方法等）。
    #[error("mcp protocol error: {0}")]
    McpProtocol(String),

    /// 工具调用参数错误。
    #[error("tool param error: {0}")]
    ToolParam(String),
}

pub type Result<T> = std::result::Result<T, KnowledgeError>;

/// 编译期断言：KnowledgeError 满足 Send + Sync。
#[allow(dead_code)]
fn _assert_knowledge_error_send_sync() {
    fn _assert<T: Send + Sync>() {}
    _assert::<KnowledgeError>();
}
