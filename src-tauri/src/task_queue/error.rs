//! 任务队列模块的统一错误类型。

use crate::platform::PlatformError;

/// 任务队列操作中可能出现的错误。
#[derive(Debug, thiserror::Error)]
pub enum TaskQueueError {
    /// 文件 I/O 错误。
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// JSON 序列化 / 反序列化错误。
    #[error("JSON 解析错误: {0}")]
    Parse(#[from] serde_json::Error),

    /// 平台路径解析失败。
    #[error(transparent)]
    Platform(#[from] PlatformError),

    /// 任务不存在。
    #[error("任务不存在: {0}")]
    NotFound(String),

    /// 任务状态非法（如在 Running 状态下尝试删除）。
    #[error("任务状态非法: {0}")]
    InvalidState(String),

    /// 任务执行失败。
    #[error("任务执行失败: {0}")]
    Execution(String),

    /// 任务被取消。
    #[error("任务被取消")]
    Cancelled,
}

/// 序列化为错误字符串（供 Tauri 命令返回前端）。
impl serde::Serialize for TaskQueueError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
