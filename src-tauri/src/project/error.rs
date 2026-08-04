//! 项目模块的统一错误类型。
//!
//! 双层设计：
//! - [`ProjectError`]：内部错误类型，使用 `?` 传播，实现 `thiserror::Error`。
//! - [`ProjectErrorResponse`]：可序列化错误响应，返回给前端，
//!   通过 `kind` 标签支持精确判断错误类型。

use serde::Serialize;

use super::frontmatter::FrontMatterError;
use super::super::platform::PlatformError;

// ---------------------------------------------------------------------------
// 内部错误类型（用于 ? 传播，不直接返回前端）
// ---------------------------------------------------------------------------

/// 项目操作中可能出现的错误。
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    /// 文件 I/O 错误。
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// YAML 序列化 / 反序列化错误。
    #[error("YAML 解析错误: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// JSON 序列化 / 反序列化错误。
    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),

    /// 请求参数校验失败。
    #[error("校验失败: {0}")]
    Validation(String),

    /// 项目路径已存在（重复创建）。
    #[error("路径已存在: {0}")]
    AlreadyExists(String),

    /// 项目路径不存在或不是目录。
    #[error("项目路径不存在或不是目录: {0}")]
    NotADirectory(String),

    /// config.yaml 缺失或格式错误。
    #[error("配置文件错误: {0}")]
    ConfigError(String),

    /// sections.json 缺失或格式错误。
    #[error("章节索引错误: {0}")]
    SectionsIndexError(String),

    /// 章节文件缺失（sections.json 引用了但文件不存在）。
    #[error("章节文件缺失: {0}")]
    SectionFileMissing(String),

    /// 章节文件 front matter 解析失败。
    #[error("章节 front matter 解析失败 ({section_id}): {reason}")]
    SectionParseError { section_id: String, reason: String },

    /// front matter 解析/序列化错误。
    #[error("front matter 错误: {0}")]
    FrontMatter(#[from] FrontMatterError),

    /// 平台路径解析失败。
    #[error(transparent)]
    Platform(#[from] PlatformError),
}

// ---------------------------------------------------------------------------
// 可序列化错误响应（返回前端，支持精确判断）
// ---------------------------------------------------------------------------

/// 返回给前端的结构化错误。
///
/// 前端通过 `kind` 字段精确判断错误类型，做差异化 UI 反馈。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
pub enum ProjectErrorResponse {
    /// 路径不存在或不是目录。
    NotADirectory { path: String },
    /// config.yaml 缺失或格式错误。
    ConfigError { reason: String },
    /// sections.json 缺失或格式错误。
    SectionsIndexError { reason: String },
    /// sections.json 引用了不存在的章节文件。
    SectionFileMissing { section_id: String },
    /// 章节文件 front matter 解析失败。
    SectionParseError { section_id: String, reason: String },
    /// 校验失败（参数不合法等）。
    Validation { reason: String },
    /// 路径已存在。
    AlreadyExists { path: String },
    /// 其他 IO 错误。
    Io { reason: String },
}

impl From<ProjectError> for ProjectErrorResponse {
    fn from(e: ProjectError) -> Self {
        match e {
            ProjectError::NotADirectory(p) => Self::NotADirectory { path: p },
            ProjectError::ConfigError(r) => Self::ConfigError { reason: r },
            ProjectError::SectionsIndexError(r) => Self::SectionsIndexError { reason: r },
            ProjectError::SectionFileMissing(id) => Self::SectionFileMissing { section_id: id },
            ProjectError::SectionParseError { section_id, reason } => {
                Self::SectionParseError { section_id, reason }
            }
            ProjectError::Validation(r) => Self::Validation { reason: r },
            ProjectError::AlreadyExists(p) => Self::AlreadyExists { path: p },
            ProjectError::Io(e) => Self::Io { reason: e.to_string() },
            ProjectError::Yaml(e) => Self::ConfigError { reason: e.to_string() },
            ProjectError::Json(e) => Self::SectionsIndexError { reason: e.to_string() },
            ProjectError::Platform(e) => Self::Io { reason: e.to_string() },
            ProjectError::FrontMatter(e) => Self::SectionParseError {
                section_id: String::new(),
                reason: e.to_string(),
            },
        }
    }
}
