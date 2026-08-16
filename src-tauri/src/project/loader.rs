//! 项目加载逻辑：读取配置 → 加载主文档 `main.md` → 读取章节索引。
//!
//! ## 执行流程
//!
//! 1. **硬校验**：路径存在且是目录
//! 2. 读取 `config.yaml` → [`ProjectConfig`]
//! 3. 读取 `sections.json` → `Vec<`[`SectionMeta`]`>`（按 order 排序）
//! 4. 加载主文档 `manuscript/main.md`：
//!    - 存在 → 直接读取
//!    - 不存在 → **旧版迁移**：按 order 从 `sec-{id}.md` 拼装 `main.md`，
//!      写入后删除遗留的 `.temp.md`
//! 5. **软校验**：目录结构 + 章节一致性 → 收集 [`ProjectWarning`]
//! 6. 返回 [`OpenProjectResult`]

use std::path::{Path, PathBuf};

use super::error::ProjectError;
use super::frontmatter;
use super::model::{
    OpenProjectResult, ProjectConfig, SectionContent, SectionFrontMatter, SectionMeta,
};
use super::validator;

/// 主文档文件名（`manuscript/main.md`）。
pub const MAIN_MD_NAME: &str = "main.md";

/// 主文档中章节分隔标记的前缀。
///
/// 完整格式：`<!-- @sec_id:{section_id} -->`，位于每个章节 H1 标题正上方。
const SEC_MARKER_PREFIX: &str = "<!-- @sec_id:";

// ---------------------------------------------------------------------------
// 公开接口
// ---------------------------------------------------------------------------

/// 打开并加载文章项目。
///
/// 硬校验失败时立即返回 `Err`；软校验问题收集到 `warnings` 中不阻断加载。
pub fn open_project(project_path: &str) -> Result<OpenProjectResult, ProjectError> {
    let project_dir = PathBuf::from(project_path);

    // 1. 硬校验：路径存在且是目录
    if !project_dir.is_dir() {
        return Err(ProjectError::NotADirectory(project_path.to_string()));
    }

    // 2. 读取 config.yaml
    let config = read_config(&project_dir)?;

    // 3. 读取 sections.json
    let mut sections = read_sections_index(&project_dir)?;
    sections.sort_by_key(|s| s.order);

    // 4. 加载主文档 main.md（旧版项目自动迁移）
    let main_md = ensure_main_md(&project_dir, &sections)?;

    // 5. 软校验：收集警告（不阻断）
    let mut warnings = validator::check_structure(&project_dir);
    warnings.extend(validator::check_section_consistency(
        &project_dir,
        &sections.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
    ));

    // 6. 返回结果
    Ok(OpenProjectResult {
        config,
        project_path: project_path.to_string(),
        sections,
        main_md,
        warnings,
    })
}

/// 返回 `manuscript/main.md` 的完整路径。
pub fn main_md_path(project_dir: &Path) -> PathBuf {
    project_dir.join("manuscript").join(MAIN_MD_NAME)
}

/// 读取主文档内容；文件缺失时返回 `None`。
pub fn read_main_md(project_dir: &Path) -> Result<Option<String>, ProjectError> {
    let path = main_md_path(project_dir);
    if !path.is_file() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    Ok(Some(content))
}

// ---------------------------------------------------------------------------
// 内部辅助函数
// ---------------------------------------------------------------------------

/// 确保主文档 `main.md` 存在。
///
/// 已存在则直接读取；否则触发**旧版迁移**：
/// 从 `sections.json` + 各 `sec-{id}.md` 按 order 拼装 `main.md`，
/// 写入后删除遗留的 `manuscript/.temp.md`。
///
/// 公开给 [`section`](super::section) 在章节操作前复用，保证
/// `main.md` 就绪（旧项目或 main.md 被手动删除时自动重建）。
pub fn ensure_main_md(project_dir: &Path, sections: &[SectionMeta]) -> Result<String, ProjectError> {
    if let Some(content) = read_main_md(project_dir)? {
        return Ok(content);
    }

    // 旧版迁移：从章节文件拼装主文档
    let contents = sections
        .iter()
        .map(|meta| read_section_file(project_dir, &meta.id))
        .collect::<Result<Vec<SectionContent>, _>>()?;
    let main_md = assemble_main_md(sections, &contents);
    write_main_md(project_dir, &main_md)?;

    // 删除遗留的 .temp.md（若有）
    let legacy = project_dir.join("manuscript").join(".temp.md");
    if legacy.is_file() {
        std::fs::remove_file(legacy)?;
    }

    Ok(main_md)
}

/// 读取并解析 `config.yaml`。
fn read_config(project_dir: &Path) -> Result<ProjectConfig, ProjectError> {
    let path = project_dir.join("config.yaml");
    let content =
        std::fs::read_to_string(&path).map_err(|_| ProjectError::ConfigError("config.yaml 读取失败".into()))?;
    serde_yaml::from_str(&content)
        .map_err(|e| ProjectError::ConfigError(format!("config.yaml 解析失败: {}", e)))
}

/// 读取并解析 `sections.json`，返回章节列表（未排序）。
fn read_sections_index(project_dir: &Path) -> Result<Vec<SectionMeta>, ProjectError> {
    let path = project_dir
        .join("manuscript")
        .join("sections")
        .join("sections.json");
    let content = std::fs::read_to_string(&path)
        .map_err(|_| ProjectError::SectionsIndexError("sections.json 读取失败".into()))?;
    serde_json::from_str(&content)
        .map_err(|e| ProjectError::SectionsIndexError(format!("sections.json 解析失败: {}", e)))
}

/// 读取单个 `sec-{UUID4}.md` 文件，分离 front matter 与正文。
fn read_section_file(
    project_dir: &Path,
    section_id: &str,
) -> Result<SectionContent, ProjectError> {
    let path = project_dir
        .join("manuscript")
        .join("sections")
        .join(format!("{}.md", section_id));

    if !path.is_file() {
        return Err(ProjectError::SectionFileMissing(section_id.to_string()));
    }

    let content = std::fs::read_to_string(&path).map_err(|e| ProjectError::SectionParseError {
        section_id: section_id.to_string(),
        reason: e.to_string(),
    })?;

    let (yaml, body) = frontmatter::split(&content).map_err(|e| ProjectError::SectionParseError {
        section_id: section_id.to_string(),
        reason: e.to_string(),
    })?;

    let front_matter: SectionFrontMatter =
        frontmatter::parse_yaml(&yaml).map_err(|e| ProjectError::SectionParseError {
            section_id: section_id.to_string(),
            reason: e.to_string(),
        })?;

    Ok(SectionContent {
        front_matter,
        body: body.trim().to_string(),
    })
}

/// 按 order 拼接所有章节为 `main.md` 内容。
///
/// 格式：每个章节块前加 HTML 注释标记，章节间空行分隔。
///
/// ```text
/// <!-- @sec_id:sec-aaa -->
/// # 引言
/// 正文…
///
/// <!-- @sec_id:sec-bbb -->
/// # 相关工作
/// 正文…
/// ```
fn assemble_main_md(sections: &[SectionMeta], contents: &[SectionContent]) -> String {
    let mut parts = Vec::with_capacity(sections.len());
    for (meta, content) in sections.iter().zip(contents.iter()) {
        parts.push(format!(
            "{}{} -->\n{}",
            SEC_MARKER_PREFIX, meta.id, content.body
        ));
    }
    parts.join("\n\n")
}

/// 将主文档内容写入 `manuscript/main.md`。
pub fn write_main_md(project_dir: &Path, content: &str) -> Result<(), ProjectError> {
    let path = main_md_path(project_dir);
    std::fs::write(path, content)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::creator;
    use crate::project::model::{CreateProjectRequest, sanitize_project_name};
    use std::fs;

    /// 创建临时项目目录。
    fn temp_project_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluen_loader_test_{}_{:?}_{}",
            std::process::id(),
            std::thread::current().id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 创建测试用项目。
    fn create_test_project(storage: &Path) -> PathBuf {
        let request = CreateProjectRequest {
            title: "测试文章".into(),
            author: "张三".into(),
            project_name: sanitize_project_name("测试文章"),
            storage_path: storage.to_string_lossy().to_string(),
            description: Some("测试描述".into()),
        };
        let path = creator::create_project(request).unwrap();
        PathBuf::from(path)
    }

    /// 向项目中添加一个章节（含 sections.json 更新）。
    fn add_section(project_dir: &Path, id: &str, order: u32, title: &str, body: &str) {
        let sections_dir = project_dir.join("manuscript").join("sections");

        // 写入章节文件
        let content = format!(
            "---\ntitle: \"{}\"\ntitle_html: \"\"\ncreated: 2026-01-01T00:00:00Z\nupdated: 2026-01-01T00:00:00Z\n---\n# {}\n\n{}",
            title, title, body
        );
        fs::write(sections_dir.join(format!("{}.md", id)), content).unwrap();

        // 更新 sections.json
        let json_path = sections_dir.join("sections.json");
        let mut sections: Vec<serde_json::Value> =
            serde_json::from_reader(fs::File::open(&json_path).unwrap()).unwrap();
        sections.push(serde_json::json!({
            "id": id,
            "order": order,
            "title": title,
            "references": []
        }));
        fs::write(&json_path, serde_json::to_string_pretty(&sections).unwrap()).unwrap();

        // 模拟旧版项目：无 main.md（creator 会创建空 main.md，此处移除以触发迁移拼装）
        let _ = fs::remove_file(project_dir.join("manuscript").join("main.md"));
    }

    #[test]
    fn open_project_valid() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);

        add_section(&project_dir, "sec-aaa11111", 0, "引言", "引言正文");
        add_section(&project_dir, "sec-bbb22222", 1, "方法", "方法正文");

        let result = open_project(project_dir.to_str().unwrap()).unwrap();

        assert_eq!(result.config.title, "测试文章");
        assert_eq!(result.sections.len(), 2);
        assert_eq!(result.sections[0].id, "sec-aaa11111");
        assert_eq!(result.sections[1].id, "sec-bbb22222");
        assert!(result.main_md.contains("<!-- @sec_id:sec-aaa11111 -->"));
        assert!(result.main_md.contains("# 引言"));
        assert!(result.main_md.contains("<!-- @sec_id:sec-bbb22222 -->"));
        assert!(result.main_md.contains("# 方法"));
        assert!(result.warnings.is_empty());
        assert!(project_dir.join("manuscript").join("main.md").exists());

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn open_project_legacy_temp_md_migrated() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);

        add_section(&project_dir, "sec-aaa11111", 0, "引言", "引言正文");
        add_section(&project_dir, "sec-bbb22222", 1, "方法", "方法正文");

        // 模拟旧版遗留：写入 .temp.md 且不存在 main.md
        let legacy = project_dir.join("manuscript").join(".temp.md");
        fs::write(&legacy, "legacy content").unwrap();

        let result = open_project(project_dir.to_str().unwrap()).unwrap();

        // main.md 由章节拼装生成
        assert!(result.main_md.contains("<!-- @sec_id:sec-aaa11111 -->"));
        assert!(result.main_md.contains("# 引言"));
        assert!(result.main_md.contains("<!-- @sec_id:sec-bbb22222 -->"));
        assert!(result.main_md.contains("# 方法"));
        // main.md 已写入磁盘
        assert!(project_dir.join("manuscript").join("main.md").exists());
        // 遗留 .temp.md 已删除
        assert!(!legacy.exists());

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn open_project_not_directory() {
        let storage = temp_project_dir();
        let file_path = storage.join("not_a_dir.txt");
        fs::write(&file_path, "hello").unwrap();

        let result = open_project(file_path.to_str().unwrap());
        assert!(matches!(result, Err(ProjectError::NotADirectory(_))));

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn open_project_missing_config() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);

        fs::remove_file(project_dir.join("config.yaml")).unwrap();
        let result = open_project(project_dir.to_str().unwrap());
        assert!(matches!(result, Err(ProjectError::ConfigError(_))));

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn open_project_empty_sections() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);

        let result = open_project(project_dir.to_str().unwrap()).unwrap();

        assert_eq!(result.sections.len(), 0);
        assert_eq!(result.main_md, "");
        assert!(result.warnings.is_empty());

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn open_project_orphan_entry() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);

        // sections.json 有记录但不写入文件
        let json_path = project_dir.join("manuscript").join("sections").join("sections.json");
        let sections = vec![serde_json::json!({
            "id": "sec-ghost",
            "order": 0,
            "title": "幽灵",
            "references": []
        })];
        fs::write(&json_path, serde_json::to_string_pretty(&sections).unwrap()).unwrap();

        // 模拟旧版项目：移除 main.md，触发从 sections 拼装的迁移路径（读取 sec-ghost 失败）
        let _ = fs::remove_file(project_dir.join("manuscript").join("main.md"));

        let result = open_project(project_dir.to_str().unwrap());
        assert!(matches!(result, Err(ProjectError::SectionFileMissing(id)) if id == "sec-ghost"));

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn open_project_orphan_file_warning() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);

        // 创建未登记的章节文件
        let orphan_path = project_dir.join("manuscript").join("sections").join("sec-orphan.md");
        fs::write(
            &orphan_path,
            "---\ntitle: \"孤儿\"\ntitle_html: \"\"\ncreated: 2026-01-01T00:00:00Z\nupdated: 2026-01-01T00:00:00Z\n---\n# 孤儿\n正文",
        )
        .unwrap();

        let result = open_project(project_dir.to_str().unwrap()).unwrap();

        let orphan_warnings: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| matches!(w.kind, super::super::model::WarningKind::OrphanSectionFile))
            .collect();
        assert_eq!(orphan_warnings.len(), 1);
        assert_eq!(orphan_warnings[0].target, "sec-orphan");

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn open_project_missing_subdir_warning() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);

        // 删除一个子目录
        fs::remove_dir_all(project_dir.join("references").join("raw")).unwrap();

        let result = open_project(project_dir.to_str().unwrap()).unwrap();

        let dir_warnings: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| matches!(w.kind, super::super::model::WarningKind::MissingDir))
            .collect();
        assert_eq!(dir_warnings.len(), 1);
        assert_eq!(dir_warnings[0].target, "references/raw");

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn open_project_crlf_line_endings() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);

        // 写入 CRLF 格式的章节文件
        let sections_dir = project_dir.join("manuscript").join("sections");
        let crlf_content =
            "---\r\ntitle: \"CRLF测试\"\r\ntitle_html: \"\"\r\ncreated: 2026-01-01T00:00:00Z\r\nupdated: 2026-01-01T00:00:00Z\r\n---\r\n# CRLF测试\r\n\r\n正文内容\r\n";
        fs::write(sections_dir.join("sec-crlf0001.md"), crlf_content).unwrap();

        let sections_json = vec![serde_json::json!({
            "id": "sec-crlf0001",
            "order": 0,
            "title": "CRLF测试",
            "references": []
        })];
        fs::write(
            sections_dir.join("sections.json"),
            serde_json::to_string_pretty(&sections_json).unwrap(),
        )
        .unwrap();

        // 模拟旧版项目：移除 main.md，触发迁移拼装（验证 CRLF 兼容）
        let _ = fs::remove_file(project_dir.join("manuscript").join("main.md"));

        let result = open_project(project_dir.to_str().unwrap()).unwrap();

        assert_eq!(result.sections.len(), 1);
        assert_eq!(result.sections[0].title, "CRLF测试");
        assert_eq!(result.sections[0].id, "sec-crlf0001");
        assert!(result.main_md.contains("# CRLF测试"));
        assert!(result.main_md.contains("正文内容"));

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn assemble_main_md_format() {
        let sections = vec![
            SectionMeta {
                id: "sec-aaa".into(),
                order: 0,
                title: "引言".into(),
                title_html: None,
                references: vec![],
            },
            SectionMeta {
                id: "sec-bbb".into(),
                order: 1,
                title: "方法".into(),
                title_html: None,
                references: vec![],
            },
        ];
        let contents = vec![
            SectionContent {
                front_matter: SectionFrontMatter {
                    title: "引言".into(),
                    title_html: "".into(),
                    created: "2026-01-01T00:00:00Z".into(),
                    updated: "2026-01-01T00:00:00Z".into(),
                },
                body: "# 引言\n\n引言正文".into(),
            },
            SectionContent {
                front_matter: SectionFrontMatter {
                    title: "方法".into(),
                    title_html: "".into(),
                    created: "2026-01-01T00:00:00Z".into(),
                    updated: "2026-01-01T00:00:00Z".into(),
                },
                body: "# 方法\n\n方法正文".into(),
            },
        ];

        let main_md = assemble_main_md(&sections, &contents);

        // 验证标记格式
        assert!(main_md.contains("<!-- @sec_id:sec-aaa -->\n# 引言"));
        assert!(main_md.contains("<!-- @sec_id:sec-bbb -->\n# 方法"));
        // 验证章节间有空行分隔
        assert!(main_md.contains("引言正文\n\n<!-- @sec_id:sec-bbb"));
    }
}
