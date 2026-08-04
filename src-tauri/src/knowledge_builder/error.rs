//! 知识库构建模块的统一错误类型。

use std::path::PathBuf;

/// 知识库构建操作中可能出现的错误。
#[derive(Debug, thiserror::Error)]
pub enum KnowledgeBuilderError {
    /// 文件 I/O 错误。
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// JSON 序列化 / 反序列化错误。
    #[error("JSON 解析错误: {0}")]
    Parse(#[from] serde_json::Error),

    /// 知识库底层错误。
    #[error("知识库错误: {0}")]
    Knowledge(#[from] fluen_knowledge::KnowledgeError),

    /// LLM 调用错误。
    #[error("LLM 调用失败: {0}")]
    Llm(String),

    /// 任务被取消。
    #[error("任务被取消")]
    Cancelled,

    /// 配置错误（如未配置模型、provider 引用失效）。
    #[error("配置错误: {0}")]
    Config(String),

    /// 任务状态机错误（如缺少 plan 却试图执行 Execution 阶段）。
    #[error("状态机错误: {0}")]
    StateMachine(String),

    /// 文献 MD 文件不存在。
    #[error("文献 MD 文件不存在: {0}")]
    MdNotFound(PathBuf),

    /// AI 输出无法解析为有效的 ExtractionPlan 或 tool_call 结果。
    #[error("AI 输出解析失败: {0}")]
    AiOutput(String),

    /// 任务队列错误。
    #[error("任务队列错误: {0}")]
    TaskQueue(String),
}

impl From<crate::task_queue::error::TaskQueueError> for KnowledgeBuilderError {
    fn from(e: crate::task_queue::error::TaskQueueError) -> Self {
        Self::TaskQueue(e.to_string())
    }
}

impl From<crate::llm_config::error::LlmConfigError> for KnowledgeBuilderError {
    fn from(e: crate::llm_config::error::LlmConfigError) -> Self {
        Self::Config(e.to_string())
    }
}

/// 序列化为错误字符串（供 Tauri 命令返回前端）。
impl serde::Serialize for KnowledgeBuilderError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
