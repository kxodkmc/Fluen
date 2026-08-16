//! 项目模块的纯数据模型。
//!
//! 所有结构体仅承载数据，不涉及文件 I/O。
//! `ProjectConfig` 序列化为 YAML（`config.yaml`），
//! `references-index.json` 与 `sections.json` 初始为空 JSON 数组。

use serde::{Deserialize, Serialize};

use super::error::ProjectError;

/// Fluen 项目配置文件版本。
pub const PROJECT_CONFIG_VERSION: &str = "1.0.0";

// ---------------------------------------------------------------------------
// 创建项目请求（前端 → 后端）
// ---------------------------------------------------------------------------

/// 前端传来的创建项目请求。
///
/// `project_name` 应由前端根据标题清洗生成，后端会再次校验。
/// `storage_path` 为**父目录**（不含项目名），最终项目路径为
/// `storage_path / project_name`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    /// 文章标题（用户原始输入）。
    pub title: String,
    /// 作者姓名。
    pub author: String,
    /// 项目文件夹名（由标题清洗生成，后端双重校验）。
    pub project_name: String,
    /// 存储父目录路径（如 `~/Documents/Fluen`）。
    pub storage_path: String,
    /// 文章描述（可选，用于简要描述文章内容或写作计划）。
    #[serde(default)]
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// config.yaml 数据模型
// ---------------------------------------------------------------------------

/// 对应项目根目录下 `config.yaml` 的数据结构。
///
/// 记录文章元信息与 Fluen 文件版本号。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Fluen 文件版本号。
    pub version: String,
    /// 文章标题。
    pub title: String,
    /// 作者姓名。
    pub author: String,
    /// 文章描述（可选，用于简要描述文章内容或写作计划）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 创建时间（ISO 8601 UTC）。
    pub created_at: String,
    /// 最后更新时间（ISO 8601 UTC）。
    pub updated_at: String,
}

impl ProjectConfig {
    /// 根据标题、作者与描述创建初始配置，时间戳取当前 UTC。
    pub fn new(title: &str, author: &str, description: Option<&str>) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            version: PROJECT_CONFIG_VERSION.to_string(),
            title: title.to_string(),
            author: author.to_string(),
            description: description.map(|d| d.to_string()),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

// ---------------------------------------------------------------------------
// 文件夹名清洗
// ---------------------------------------------------------------------------

/// 文件系统非法字符与常见标点符号集合（跨平台并集）。
///
/// 包含文件系统非法字符（`\ / : * ? " < > |`）
/// 以及常见标点（`! . , ; ' ~ # $ % ^ & ( ) = + [ ] { }`），
/// 这些字符虽部分在文件系统中合法，但在文件夹名中易引起歧义或跨平台兼容问题。
const INVALID_FILENAME_CHARS: &[char] = &[
    '\\', '/', ':', '*', '?', '"', '<', '>', '|',
    '!', '.', ',', ';', '\'', '~', '#', '$', '%', '^', '&',
    '(', ')', '=', '+', '[', ']', '{', '}',
];

/// 将文章标题清洗为合法的文件夹名。
///
/// 规则：
/// 1. 去除文件系统非法字符与常见标点符号
/// 2. 空格替换为 `-`
/// 3. 转为小写
/// 4. 折叠连续的 `-`
/// 5. 去除首尾 `-`
/// 6. 若结果为空，返回默认名 `untitled`
///
/// # 示例
///
/// ```
/// // "My: Cool Article!" -> "my-cool-article"
/// // "研究 / 测试" -> "研究-测试"
/// // "..." -> "untitled"
/// ```
pub fn sanitize_project_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if INVALID_FILENAME_CHARS.contains(&c) {
                String::new()
            } else if c == ' ' {
                "-".to_string()
            } else {
                c.to_string()
            }
        })
        .collect();

    // 折叠连续的 `-`
    let mut result = String::new();
    let mut prev_dash = false;
    for ch in cleaned.chars() {
        if ch == '-' {
            if !prev_dash {
                result.push('-');
            }
            prev_dash = true;
        } else {
            result.push(ch);
            prev_dash = false;
        }
    }

    // 去除首尾 `-` 并转小写
    let result = result.trim_matches('-').to_lowercase();

    if result.is_empty() {
        "untitled".to_string()
    } else {
        result
    }
}

impl CreateProjectRequest {
    /// 校验请求完整性。
    ///
    /// 确保标题、作者非空，`project_name` 合法，`storage_path` 非空。
    /// 后端双重保险：即使前端已清洗，后端仍校验 `project_name` 不含非法字符。
    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.title.trim().is_empty() {
            return Err(ProjectError::Validation("标题不能为空".into()));
        }
        if self.author.trim().is_empty() {
            return Err(ProjectError::Validation("作者不能为空".into()));
        }
        if self.project_name.trim().is_empty() {
            return Err(ProjectError::Validation("项目名不能为空".into()));
        }
        if self.storage_path.trim().is_empty() {
            return Err(ProjectError::Validation("存储路径不能为空".into()));
        }

        // 后端双重校验：检查 project_name 是否与清洗结果一致
        let sanitized = sanitize_project_name(&self.title);
        if self.project_name != sanitized && self.project_name != sanitize_project_name(&self.project_name) {
            return Err(ProjectError::Validation(format!(
                "项目名不合法（应为: {}）",
                sanitized
            )));
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// sections.json 章节索引条目
// ---------------------------------------------------------------------------

/// `sections.json` 中的单个章节条目。
///
/// 章节的实际排列顺序由 `order` 字段决定（升序）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionMeta {
    /// 章节唯一标识，格式 `sec-{UUID4}`。
    pub id: String,
    /// 章节顺序（从 0 开始，按此字段升序排列）。
    pub order: u32,
    /// 章节标题（纯文本）。
    pub title: String,
    /// 章节标题的 HTML 版本（可选，保留富文本格式如颜色、字号）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_html: Option<String>,
    /// 该章节引用的文献 ID 列表（扩展性预留）。
    #[serde(default)]
    pub references: Vec<String>,
}

// ---------------------------------------------------------------------------
// sec-{UUID4}.md front matter
// ---------------------------------------------------------------------------

/// 章节文件的 YAML front matter。
///
/// 对应 `sec-{UUID4}.md` 文件中 `---` 分隔的 YAML 区域。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionFrontMatter {
    /// 章节标题（双引号包裹，内部双引号和反斜杠转义）。
    pub title: String,
    /// 标题的 HTML 版本（无富文本时为空字符串）。
    #[serde(default)]
    pub title_html: String,
    /// 章节创建时间（RFC3339）。
    pub created: String,
    /// 章节最后更新时间（RFC3339）。
    pub updated: String,
}

// ---------------------------------------------------------------------------
// 章节完整内容
// ---------------------------------------------------------------------------

/// 一个章节的完整内容（front matter + 正文）。
#[derive(Debug, Clone)]
pub struct SectionContent {
    /// YAML front matter（预留：未来保存/校验时使用）。
    #[allow(dead_code)]
    pub front_matter: SectionFrontMatter,
    /// Markdown 正文（front matter 之后的内容，含 `#` 标题行）。
    pub body: String,
}

// ---------------------------------------------------------------------------
// 软校验警告
// ---------------------------------------------------------------------------

/// 警告类型分类。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningKind {
    /// 缺失目录。
    MissingDir,
    /// 缺失文件。
    MissingFile,
    /// sections.json 有条目但文件不存在。
    OrphanSectionEntry,
    /// 存在 sec-*.md 文件但 sections.json 中无记录。
    OrphanSectionFile,
}

/// 单条校验警告（软校验，不阻断加载）。
#[derive(Debug, Clone, Serialize)]
pub struct ProjectWarning {
    /// 警告类型。
    pub kind: WarningKind,
    /// 相对项目根的路径或章节 ID。
    pub target: String,
    /// 人类可读描述。
    pub message: String,
}

// ---------------------------------------------------------------------------
// 打开项目返回值
// ---------------------------------------------------------------------------

/// 打开项目后返回给前端的完整数据。
#[derive(Debug, Clone, Serialize)]
pub struct OpenProjectResult {
    /// 项目配置（config.yaml）。
    pub config: ProjectConfig,
    /// 项目根路径。
    pub project_path: String,
    /// 章节列表（已按 order 排序）。
    pub sections: Vec<SectionMeta>,
    /// 主文档 `manuscript/main.md` 的全文内容（含 `<!-- @sec_id: -->` 标记）。
    pub main_md: String,
    /// 软校验产生的警告列表（不阻断加载）。
    pub warnings: Vec<ProjectWarning>,
}

// ---------------------------------------------------------------------------
// 章节操作请求（前端 → 后端）
// ---------------------------------------------------------------------------

/// 前端传来的创建章节请求。
///
/// 创建一级标题章节，后端生成 `sec-{UUID4}.md` 文件并更新 `sections.json`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSectionRequest {
    /// 项目根路径。
    pub project_path: String,
    /// 章节标题。
    pub title: String,
}

/// 前端传来的标题重命名请求。
///
/// 通过 `section_id` + `level` + `old_text` 在章节文件中**文本匹配**定位标题行，
/// 替换为 `new_text`。不依赖行号，避免编辑器偏移导致错位。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameHeadingRequest {
    /// 项目根路径。
    pub project_path: String,
    /// 章节唯一标识（`sec-{UUID4}`）。
    pub section_id: String,
    /// 标题层级（1-6）。
    pub level: u32,
    /// 原标题文本（用于在文件中匹配定位）。
    pub old_text: String,
    /// 新标题文本。
    pub new_text: String,
}

/// 前端传来的插入子标题请求。
///
/// 在锚点标题（`anchor_level` + `anchor_text`）的作用域末尾插入新标题。
/// 锚点作用域 = 锚点标题行之后、下一个同级或更浅标题之前的区域。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertHeadingRequest {
    /// 项目根路径。
    pub project_path: String,
    /// 章节唯一标识（`sec-{UUID4}`）。
    pub section_id: String,
    /// 锚点标题层级（1-6，用于在文件中定位插入位置）。
    pub anchor_level: u32,
    /// 锚点标题文本（用于在文件中匹配定位）。
    pub anchor_text: String,
    /// 新标题层级（2-6）。
    pub new_level: u32,
    /// 新标题文本。
    pub new_text: String,
}

/// 前端传来的主文档保存请求。
///
/// 将编辑后的 `main.md` 全文持久化：校验文档结构无异常后写入 `main.md`，
/// 并拆分回各 `sec-{id}.md` 备份文件（同时更新 `sections.json`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveDocumentRequest {
    /// 项目根路径。
    pub project_path: String,
    /// 编辑后的 `main.md` 全文内容。
    pub content: String,
}

impl CreateSectionRequest {
    /// 校验请求完整性。
    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.project_path.trim().is_empty() {
            return Err(ProjectError::Validation("项目路径不能为空".into()));
        }
        if self.title.trim().is_empty() {
            return Err(ProjectError::Validation("章节标题不能为空".into()));
        }
        Ok(())
    }
}

impl RenameHeadingRequest {
    /// 校验请求完整性。
    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.project_path.trim().is_empty() {
            return Err(ProjectError::Validation("项目路径不能为空".into()));
        }
        if self.section_id.trim().is_empty() {
            return Err(ProjectError::Validation("章节 ID 不能为空".into()));
        }
        if self.new_text.trim().is_empty() {
            return Err(ProjectError::Validation("新标题不能为空".into()));
        }
        if !(1..=6).contains(&self.level) {
            return Err(ProjectError::Validation("标题层级必须在 1-6 之间".into()));
        }
        Ok(())
    }
}

impl InsertHeadingRequest {
    /// 校验请求完整性。
    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.project_path.trim().is_empty() {
            return Err(ProjectError::Validation("项目路径不能为空".into()));
        }
        if self.section_id.trim().is_empty() {
            return Err(ProjectError::Validation("章节 ID 不能为空".into()));
        }
        if self.anchor_text.trim().is_empty() {
            return Err(ProjectError::Validation("锚点标题不能为空".into()));
        }
        if !(1..=6).contains(&self.anchor_level) {
            return Err(ProjectError::Validation("锚点层级必须在 1-6 之间".into()));
        }
        if self.new_text.trim().is_empty() {
            return Err(ProjectError::Validation("新标题不能为空".into()));
        }
        if !(2..=6).contains(&self.new_level) {
            return Err(ProjectError::Validation("子标题层级必须在 2-6 之间".into()));
        }
        Ok(())
    }
}

impl SaveDocumentRequest {
    /// 校验请求完整性。
    ///
    /// 注意：允许 `content` 为空（用户在编辑器中删除全部内容后保存是合法操作，
    /// 后端 `save_document` 会将空内容写入所有章节文件）。
    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.project_path.trim().is_empty() {
            return Err(ProjectError::Validation("项目路径不能为空".into()));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_basic() {
        assert_eq!(sanitize_project_name("My: Cool Article!"), "my-cool-article");
    }

    #[test]
    fn sanitize_chinese() {
        assert_eq!(sanitize_project_name("研究 / 测试"), "研究-测试");
    }

    #[test]
    fn sanitize_multiple_spaces() {
        assert_eq!(sanitize_project_name("  multiple   spaces  "), "multiple-spaces");
    }

    #[test]
    fn sanitize_empty_title() {
        assert_eq!(sanitize_project_name(""), "untitled");
    }

    #[test]
    fn sanitize_only_invalid_chars() {
        assert_eq!(sanitize_project_name("/:*?\"<>|"), "untitled");
    }

    #[test]
    fn sanitize_all_dots() {
        assert_eq!(sanitize_project_name("..."), "untitled");
    }

    #[test]
    fn config_new_has_timestamps() {
        let config = ProjectConfig::new("测试标题", "测试作者", Some("测试描述"));
        assert_eq!(config.title, "测试标题");
        assert_eq!(config.author, "测试作者");
        assert_eq!(config.description.as_deref(), Some("测试描述"));
        assert_eq!(config.version, PROJECT_CONFIG_VERSION);
        assert!(!config.created_at.is_empty());
        assert_eq!(config.created_at, config.updated_at);
    }

    #[test]
    fn config_new_without_description() {
        let config = ProjectConfig::new("测试标题", "测试作者", None);
        assert!(config.description.is_none());
    }

    #[test]
    fn validate_ok() {
        let req = CreateProjectRequest {
            title: "Test Article".into(),
            author: "Author".into(),
            project_name: "test-article".into(),
            storage_path: "/tmp".into(),
            description: Some("A test article".into()),
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn validate_ok_without_description() {
        let req = CreateProjectRequest {
            title: "Test Article".into(),
            author: "Author".into(),
            project_name: "test-article".into(),
            storage_path: "/tmp".into(),
            description: None,
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn validate_empty_title() {
        let req = CreateProjectRequest {
            title: "  ".into(),
            author: "Author".into(),
            project_name: "test".into(),
            storage_path: "/tmp".into(),
            description: None,
        };
        assert!(req.validate().is_err());
    }

    #[test]
    fn validate_invalid_project_name() {
        let req = CreateProjectRequest {
            title: "Test".into(),
            author: "Author".into(),
            project_name: "test:invalid".into(),
            storage_path: "/tmp".into(),
            description: None,
        };
        assert!(req.validate().is_err());
    }
}
