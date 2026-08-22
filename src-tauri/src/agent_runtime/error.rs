//! 运行时错误类型。

/// 运行时构建与操作错误。
#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// LLM 连接层配置错误。
    #[error("LLM 配置错误: {0}")]
    LlmConfig(String),
    /// 引擎启动错误。
    #[error("引擎启动错误: {0}")]
    EngineStart(String),
    /// 会话不存在或状态异常。
    #[error("会话错误: {0}")]
    Session(String),
}

impl From<crate::llm_chat::LlmChatError> for RuntimeError {
    fn from(e: crate::llm_chat::LlmChatError) -> Self {
        match e {
            crate::llm_chat::LlmChatError::Config(msg) => Self::LlmConfig(msg),
            crate::llm_chat::LlmChatError::NoProvider => {
                Self::LlmConfig("未配置 LLM 提供商".into())
            }
        }
    }
}
