//! 学术助手模块的统一错误类型。
//!
//! 实现 [`serde::Serialize`] 以便作为 Tauri command 的返回错误类型，
//! 序列化为错误消息字符串。

/// 学术助手操作中可能出现的错误。
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum AiAssistantError {
    /// 配置错误（缺少必需字段、提供商不存在等）。
    #[error("配置错误: {0}")]
    Config(String),

    /// LLM 配置缺失或无效。
    #[error("LLM 配置错误: {0}")]
    LlmConfig(String),

    /// 运行时错误（会话启动失败等）。
    #[error("运行时错误: {0}")]
    Runtime(String),

    /// IO 错误。
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// 平台路径解析失败。
    #[error(transparent)]
    Platform(#[from] crate::platform::PlatformError),

    /// 无可用的 LLM 提供商或模型。
    #[error("无可用的 LLM 提供商或模型")]
    NoProvider,

    /// 会话已被取消。
    #[error("会话已取消")]
    Cancelled,

    /// Tauri 事件发送失败。
    #[error("事件发送失败: {0}")]
    Emit(String),
}

/// 为 Tauri command 实现 [`serde::Serialize`]，将错误序列化为消息字符串。
impl serde::Serialize for AiAssistantError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
