//! 参考文献模块的统一错误类型。

use serde::Serialize;

use crate::ai_services::error::AiServiceError;

/// 参考文献操作中可能出现的错误。
#[derive(Debug, thiserror::Error)]
pub enum ReferenceError {
    /// 文件 I/O 错误。
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// JSON 序列化 / 反序列化错误。
    #[error("JSON 解析错误: {0}")]
    Parse(#[from] serde_json::Error),

    /// 文件格式不支持。
    #[error("不支持的文件格式: {extension}（仅支持 PDF 和图片）")]
    UnsupportedFormat { extension: String },

    /// 文件不存在。
    #[error("文件不存在: {0}")]
    FileNotFound(String),

    /// 文献 ID 不存在（查找 / 删除 / 重试时）。
    #[error("文献不存在: {0}")]
    NotFound(String),

    /// 重复导入（相同文件哈希已存在）。
    #[error("文件已导入: {existing_title}")]
    Duplicate {
        /// 已存在文献的标题。
        existing_title: String,
        /// 已存在文献的 ID。
        existing_id: String,
    },

    /// OCR 服务未配置或不可用。
    #[error("OCR 服务不可用: {0}")]
    OcrUnavailable(String),

    /// OCR 调用错误（透传 [`AiServiceError`]）。
    #[error(transparent)]
    Ocr(#[from] AiServiceError),

    /// 导入任务已被取消。
    #[error("导入任务已取消")]
    Cancelled,

    /// 通用错误。
    #[error("{0}")]
    Other(String),
}

/// 为 Tauri command 实现序列化——错误消息字符串。
impl Serialize for ReferenceError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<tauri::Error> for ReferenceError {
    fn from(e: tauri::Error) -> Self {
        ReferenceError::Other(e.to_string())
    }
}
