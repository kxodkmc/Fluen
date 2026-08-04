//! AI 服务模块的统一错误类型。
//!
//! 实现 [`serde::Serialize`] 以便作为 Tauri command 的返回错误类型，
//! 序列化为错误消息字符串。

use crate::platform::PlatformError;

/// AI 服务操作中可能出现的错误。
#[derive(Debug, thiserror::Error)]
pub enum AiServiceError {
    /// 文件 I/O 错误。
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// JSON 序列化 / 反序列化错误。
    #[error("JSON 解析错误: {0}")]
    Parse(#[from] serde_json::Error),

    /// 配置校验失败（如必填字段缺失、ID 重复等）。
    #[error("配置校验失败: {0}")]
    Validation(String),

    /// 平台路径解析失败。
    #[error(transparent)]
    Platform(#[from] PlatformError),

    /// HTTP 请求错误。
    #[error("HTTP 请求错误: {0}")]
    Http(#[from] reqwest::Error),

    /// API 返回错误状态码。
    #[error("API 错误 ({status}): {body}")]
    ApiStatus {
        /// HTTP 状态码。
        status: u16,
        /// 响应体文本。
        body: String,
    },

    /// API 返回的业务错误（如 jobId 不存在、Token 无效等）。
    #[error("API 业务错误: {0}")]
    ApiBusiness(String),

    /// 任务已被取消。
    #[error("任务已取消")]
    Cancelled,

    /// Tauri 事件发送失败。
    #[error("事件发送失败: {0}")]
    Emit(String),

    /// 通用错误（如文件不存在、配置缺失等）。
    #[error("{0}")]
    Other(String),
}

/// 为 Tauri command 实现 [`serde::Serialize`]，将错误序列化为消息字符串。
impl serde::Serialize for AiServiceError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<tauri::Error> for AiServiceError {
    fn from(e: tauri::Error) -> Self {
        AiServiceError::Emit(e.to_string())
    }
}
