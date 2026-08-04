//! 宠物助手模块的统一错误类型。

/// 宠物助手操作中可能出现的错误。
#[derive(Debug, thiserror::Error)]
pub enum MascotError {
    /// 文件 I/O 错误。
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// JSON 序列化 / 反序列化错误。
    #[error("JSON 解析错误: {0}")]
    Parse(#[from] serde_json::Error),

    /// 配置校验失败。
    #[error("配置校验失败: {0}")]
    Validation(String),

    /// 平台路径解析失败。
    #[error(transparent)]
    Platform(#[from] crate::platform::PlatformError),
}
