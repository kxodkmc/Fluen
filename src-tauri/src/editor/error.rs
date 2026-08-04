//! 编辑器错误类型。

use thiserror::Error;

/// 编辑器错误。
#[derive(Debug, Error)]
pub enum EditorError {
    #[error("文本区间越界: {0:?}")]
    RangeOutOfBounds(std::ops::Range<usize>),

    #[error("未绑定项目路径")]
    NoProjectBound,

    #[error("渲染失败: {0}")]
    RenderError(String),

    #[error("项目操作失败: {0}")]
    ProjectError(String),

    #[error("非法的资源文件名: {0}")]
    InvalidAssetFilename(String),

    #[error("资源文件 I/O 失败: {0}")]
    AssetIoFailed(String),
}

pub type Result<T> = std::result::Result<T, EditorError>;
