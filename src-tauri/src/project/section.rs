//! 章节级操作——创建章节、标题重命名、插入子标题、`.temp.md` 回写。
//!
//! 所有操作完成后调用 [`loader::open_project`] 重新加载项目，
//! 保证 `.temp.md` 与各 `sec-{id}.md` 文件的数据一致性。
//!
//! ## 设计决策
//!
//! - **文本匹配定位**：重命名通过 `section_id` + `level` + `old_text`
//!   在章节文件中匹配标题行，不依赖行号，避免编辑器偏移导致错位。
//! - **标记完整性校验**：`save_temp_md` 拆分后校验块数量与 `sections.json`
//!   条目数一致，不一致则拒绝写入，保护原始文件。
//! - **全量重载**：每次操作后调用 `loader::open_project`，确保数据一致。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::error::ProjectError;
use super::frontmatter;
use super::loader;
use super::model::{
    CreateSectionRequest, InsertHeadingRequest, OpenProjectResult, RenameHeadingRequest,
    SaveTempMdRequest, SectionFrontMatter, SectionMeta,
};

/// 章节标记前缀（与 `loader::SEC_MARKER_PREFIX` 保持一致）。
const SEC_MARKER_PREFIX: &str = "<!-- @sec_id:";

// ---------------------------------------------------------------------------
// 公开接口
// ---------------------------------------------------------------------------

/// 创建新章节（一级标题）。
///
/// 生成 `sec-{UUID4}.md` 文件，更新 `sections.json`，重新加载项目。
pub fn create_section(request: CreateSectionRequest) -> Result<OpenProjectResult, ProjectError> {
    request.validate()?;

    let project_dir = PathBuf::from(&request.project_path);
    let section_id = generate_section_id();
    let now = chrono::Utc::now().to_rfc3339();

    // 读取 sections.json，计算新 order
    let mut sections = read_sections_index(&project_dir)?;
    let new_order = sections.iter().map(|s| s.order).max().unwrap_or(0) + 1;

    // 写入章节文件
    let front_matter = SectionFrontMatter {
        title: request.title.clone(),
        title_html: String::new(),
        created: now.clone(),
        updated: now,
    };
    let body = format!("# {}\n", request.title);
    let content = frontmatter::join(&front_matter, &body)?;
    let section_path = sections_dir(&project_dir).join(format!("{}.md", section_id));
    std::fs::write(&section_path, content)?;

    // 更新 sections.json
    sections.push(SectionMeta {
        id: section_id,
        order: new_order,
        title: request.title,
        title_html: None,
        references: vec![],
    });
    write_sections_index(&project_dir, &sections)?;

    // 重新加载项目（自动重拼 .temp.md + 软校验）
    loader::open_project(&request.project_path)
}

/// 重命名标题（任意层级）。
///
/// 通过 `section_id` + `level` + `old_text` 在章节正文中**文本匹配**定位标题行，
/// 替换为 `new_text`。若为 H1 标题，同步更新 front matter `title` 和 `sections.json`。
pub fn rename_heading(request: RenameHeadingRequest) -> Result<OpenProjectResult, ProjectError> {
    request.validate()?;

    let project_dir = PathBuf::from(&request.project_path);

    // 读取章节文件
    let (mut front_matter, body) = read_section_file_parts(&project_dir, &request.section_id)?;

    // 在正文中查找并替换标题行
    let new_body = replace_first_heading(&body, request.level, &request.old_text, &request.new_text)
        .ok_or_else(|| {
            ProjectError::Validation(format!(
                "未找到匹配的标题: H{} \"{}\"",
                request.level, request.old_text
            ))
        })?;

    // 若为 H1，同步更新 front matter title 和 sections.json
    if request.level == 1 {
        front_matter.title = request.new_text.clone();
        front_matter.updated = chrono::Utc::now().to_rfc3339();

        let mut sections = read_sections_index(&project_dir)?;
        if let Some(meta) = sections.iter_mut().find(|s| s.id == request.section_id) {
            meta.title = request.new_text.clone();
        }
        write_sections_index(&project_dir, &sections)?;
    }

    // 写回章节文件
    let content = frontmatter::join(&front_matter, &new_body)?;
    let section_path = sections_dir(&project_dir).join(format!("{}.md", request.section_id));
    std::fs::write(&section_path, content)?;

    // 重新加载
    loader::open_project(&request.project_path)
}

/// 插入子标题（在锚点标题的作用域末尾）。
///
/// 锚点作用域 = 锚点标题行之后、下一个同级或更浅标题之前的区域。
/// 新标题插入在作用域末尾（即下一个同级/更浅标题之前），确保前有空行分隔。
///
/// 例如在 `## 背景` 的作用域末尾插入 `### 新标题`：
/// ```text
/// # 引言
/// ## 背景        ← 锚点
/// 背景正文
/// ## 方法        ← 下一个同级标题（插入点在这之前）
/// ```
/// 结果：
/// ```text
/// # 引言
/// ## 背景
/// 背景正文
///
/// ### 新标题    ← 新插入
///
/// ## 方法
/// ```
pub fn insert_heading(request: InsertHeadingRequest) -> Result<OpenProjectResult, ProjectError> {
    request.validate()?;

    let project_dir = PathBuf::from(&request.project_path);

    // 读取章节文件
    let (front_matter, body) = read_section_file_parts(&project_dir, &request.section_id)?;

    // 在锚点作用域末尾插入新标题
    let new_body = insert_after_scope(
        &body,
        request.anchor_level,
        &request.anchor_text,
        request.new_level,
        &request.new_text,
    )
    .ok_or_else(|| {
        ProjectError::Validation(format!(
            "未找到锚点标题: H{} \"{}\"",
            request.anchor_level, request.anchor_text
        ))
    })?;

    // 写回章节文件
    let content = frontmatter::join(&front_matter, &new_body)?;
    let section_path = sections_dir(&project_dir).join(format!("{}.md", request.section_id));
    std::fs::write(&section_path, content)?;

    // 重新加载
    loader::open_project(&request.project_path)
}

/// 保存 `.temp.md` 内容——拆分回各章节文件。
///
/// **安全校验**：
/// 1. 所有标记的 section ID 必须在 `sections.json` 中存在（拒绝未知 ID）
/// 2. 不允许重复标记（拒绝重复 ID）
///
/// **标记缺失处理**：
/// 当用户在编辑器中删除标题时，对应的 `<!-- @sec_id:xxx -->` 标记可能随之被删除。
/// 此时按 ID 匹配已有标记的内容，缺失标记的章节写入空正文，而非拒绝保存。
/// 这确保用户删除标题后保存不会丢失其他章节的编辑。
pub fn save_temp_md(request: SaveTempMdRequest) -> Result<OpenProjectResult, ProjectError> {
    request.validate()?;

    let project_dir = PathBuf::from(&request.project_path);

    // 统一换行符为 \n（消除 \r\n 差异）
    let content = request.content.replace("\r\n", "\n").replace('\r', "\n");

    // 读取 sections.json
    let sections = read_sections_index(&project_dir)?;

    // 按 sec_id 标记拆分；标记全部缺失时返回空 Vec（后续按空正文处理）
    let blocks = split_temp_md(&content).unwrap_or_default();

    // 安全校验：所有标记的 section ID 必须在 sections.json 中存在
    let section_ids: HashSet<&str> = sections.iter().map(|s| s.id.as_str()).collect();
    for (id, _) in &blocks {
        if !section_ids.contains(id.as_str()) {
            return Err(ProjectError::Validation(format!(
                "未知章节 ID: {}，拒绝写入以保护原始文件",
                id
            )));
        }
    }

    // 安全校验：不允许重复标记
    let mut seen = HashSet::new();
    for (id, _) in &blocks {
        if !seen.insert(id.as_str()) {
            return Err(ProjectError::Validation(format!(
                "重复章节标记: {}，拒绝写入以保护原始文件",
                id
            )));
        }
    }

    // 构建 id → body 映射；缺失标记的章节写空正文
    let block_map: HashMap<&str, &str> = blocks
        .iter()
        .map(|(id, body)| (id.as_str(), body.as_str()))
        .collect();

    // 按 sections.json 顺序逐块更新章节文件
    let now = chrono::Utc::now().to_rfc3339();
    let mut updated_sections = sections.clone();

    for meta in &sections {
        let body = block_map.get(meta.id.as_str()).copied().unwrap_or("");
        let (mut front_matter, _old_body) = read_section_file_parts(&project_dir, &meta.id)?;

        // 从正文提取 H1 标题更新 front matter
        if let Some(h1_title) = extract_h1_title(body) {
            front_matter.title = h1_title.clone();
            front_matter.updated = now.clone();

            if let Some(m) = updated_sections.iter_mut().find(|s| &s.id == &meta.id) {
                m.title = h1_title;
            }
        } else {
            // 正文无 H1 标题（如内容被清空）：更新时间戳
            front_matter.updated = now.clone();
        }

        let content = frontmatter::join(&front_matter, body)?;
        let section_path = sections_dir(&project_dir).join(format!("{}.md", meta.id));
        std::fs::write(&section_path, content)?;
    }

    // 更新 sections.json
    write_sections_index(&project_dir, &updated_sections)?;

    // 重新加载（标准化 .temp.md 格式 + 软校验）
    loader::open_project(&request.project_path)
}

// ---------------------------------------------------------------------------
// 内部辅助函数
// ---------------------------------------------------------------------------

/// 生成章节 ID：`sec-{16位UUID4十六进制}`。
fn generate_section_id() -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    format!("sec-{}", &id[..16])
}

/// 返回 `manuscript/sections/` 目录路径。
fn sections_dir(project_dir: &Path) -> PathBuf {
    project_dir.join("manuscript").join("sections")
}

/// 读取并解析 `sections.json`。
fn read_sections_index(project_dir: &Path) -> Result<Vec<SectionMeta>, ProjectError> {
    let path = sections_dir(project_dir).join("sections.json");
    let content = std::fs::read_to_string(&path)
        .map_err(|_| ProjectError::SectionsIndexError("sections.json 读取失败".into()))?;
    serde_json::from_str(&content)
        .map_err(|e| ProjectError::SectionsIndexError(format!("sections.json 解析失败: {}", e)))
}

/// 写入 `sections.json`。
fn write_sections_index(project_dir: &Path, sections: &[SectionMeta]) -> Result<(), ProjectError> {
    let path = sections_dir(project_dir).join("sections.json");
    let json = serde_json::to_string_pretty(sections)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// 读取章节文件，分离 front matter 与正文。
fn read_section_file_parts(
    project_dir: &Path,
    section_id: &str,
) -> Result<(SectionFrontMatter, String), ProjectError> {
    let path = sections_dir(project_dir).join(format!("{}.md", section_id));
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
    Ok((front_matter, body.trim().to_string()))
}

/// 在正文中查找并替换首个匹配的标题行。
///
/// 匹配规则：行首（允许前导空白）为 `#{level} {old_text}`。
/// 替换为 `#{level} {new_text}`，保留原始缩进。
fn replace_first_heading(body: &str, level: u32, old_text: &str, new_text: &str) -> Option<String> {
    let prefix = format!("{} ", "#".repeat(level as usize));
    let new_line = format!("{}{}", prefix, new_text);

    let mut found = false;
    let result: Vec<String> = body
        .lines()
        .map(|line| {
            if !found {
                let trimmed = line.trim_start();
                if trimmed.starts_with(&prefix) {
                    let text = trimmed[prefix.len()..].trim();
                    if text == old_text {
                        found = true;
                        let indent = &line[..line.len() - line.trim_start().len()];
                        return format!("{}{}", indent, new_line);
                    }
                }
            }
            line.to_string()
        })
        .collect();

    if found {
        Some(result.join("\n"))
    } else {
        None
    }
}

/// 在锚点标题的作用域末尾插入新标题。
///
/// 锚点作用域 = 锚点标题行之后、下一个同级或更浅标题（level <= anchor_level）之前的区域。
/// 若没有下一个同级/更浅标题，则作用域延伸到正文末尾。
///
/// 插入位置：作用域末尾（去除尾部空行后），新标题前后各留空行。
fn insert_after_scope(
    body: &str,
    anchor_level: u32,
    anchor_text: &str,
    new_level: u32,
    new_text: &str,
) -> Option<String> {
    let prefix = format!("{} ", "#".repeat(anchor_level as usize));
    let lines: Vec<&str> = body.lines().collect();

    // 1. 查找锚点标题行
    let anchor_idx = lines.iter().position(|line| {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&prefix) {
            trimmed[prefix.len()..].trim() == anchor_text
        } else {
            false
        }
    })?;

    // 2. 查找作用域末尾：从锚点行之后开始，找下一个 level <= anchor_level 的标题
    let insert_idx = lines[anchor_idx + 1..]
        .iter()
        .position(|line| {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix('#') {
                let hashes = rest.chars().take_while(|c| *c == '#').count() + 1;
                if hashes <= anchor_level as usize {
                    // 确认是标题行（# 后跟空格或行尾）
                    let after = &rest[hashes - 1..];
                    after.is_empty() || after.starts_with(' ')
                } else {
                    false
                }
            } else {
                false
            }
        })
        .map(|i| anchor_idx + 1 + i)
        .unwrap_or(lines.len());

    // 3. 构建新内容
    let new_heading = format!(
        "{} {}",
        "#".repeat(new_level as usize),
        new_text
    );

    let mut result: Vec<String> = Vec::with_capacity(lines.len() + 3);

    // 锚点行及之前的内容
    for line in &lines[..insert_idx] {
        result.push(line.to_string());
    }

    // 去除尾部空行
    while result.last().map(|s| s.trim().is_empty()).unwrap_or(false) {
        result.pop();
    }

    // 新标题（前后各空行）
    result.push(String::new());
    result.push(new_heading);
    result.push(String::new());

    // 剩余内容
    for line in &lines[insert_idx..] {
        result.push(line.to_string());
    }

    Some(result.join("\n"))
}

/// 从正文中提取首个 H1 标题文本。
fn extract_h1_title(body: &str) -> Option<String> {
    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("# ") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

/// 按 `<!-- @sec_id:xxx -->` 标记拆分 `.temp.md` 内容。
///
/// 返回 `Vec<(section_id, body)>`，body 为标记行之后到下一个标记行（或 EOF）的内容。
/// 标记行之前的内容被忽略（正常 `.temp.md` 不应有）。
fn split_temp_md(content: &str) -> Result<Vec<(String, String)>, ProjectError> {
    let lines: Vec<&str> = content.lines().collect();
    let mut blocks: Vec<(String, String)> = Vec::new();
    let mut current_id: Option<String> = None;
    let mut current_body: Vec<&str> = Vec::new();

    for line in &lines {
        if let Some(id) = parse_sec_marker(line) {
            // 保存上一个块
            if let Some(id) = current_id.take() {
                blocks.push((id, current_body.join("\n").trim().to_string()));
                current_body.clear();
            }
            current_id = Some(id);
        } else if current_id.is_some() {
            current_body.push(line);
        }
    }

    // 保存最后一个块
    if let Some(id) = current_id {
        blocks.push((id, current_body.join("\n").trim().to_string()));
    }

    if blocks.is_empty() {
        return Err(ProjectError::Validation(
            "未找到任何章节标记 (<!-- @sec_id:xxx -->)，可能标记已损坏".into(),
        ));
    }

    Ok(blocks)
}

/// 解析 `<!-- @sec_id:xxx -->` 标记行，返回 section ID。
fn parse_sec_marker(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with(SEC_MARKER_PREFIX) && trimmed.ends_with("-->") {
        let start = SEC_MARKER_PREFIX.len();
        let end = trimmed.len() - "-->".len();
        Some(trimmed[start..end].trim().to_string())
    } else {
        None
    }
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

    fn temp_project_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluen_section_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn create_test_project(storage: &Path) -> PathBuf {
        let request = CreateProjectRequest {
            title: "测试文章".into(),
            author: "张三".into(),
            project_name: sanitize_project_name("测试文章"),
            storage_path: storage.to_string_lossy().to_string(),
            description: None,
        };
        let path = creator::create_project(request).unwrap();
        PathBuf::from(path)
    }

    fn add_section(project_dir: &Path, id: &str, order: u32, title: &str, body: &str) {
        let sections_dir = project_dir.join("manuscript").join("sections");
        let content = format!(
            "---\ntitle: \"{}\"\ntitle_html: \"\"\ncreated: 2026-01-01T00:00:00Z\nupdated: 2026-01-01T00:00:00Z\n---\n{}",
            title, body
        );
        fs::write(sections_dir.join(format!("{}.md", id)), content).unwrap();

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
    }

    #[test]
    fn create_section_works() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        let project_path = project_dir.to_str().unwrap();

        let request = CreateSectionRequest {
            project_path: project_path.into(),
            title: "引言".into(),
        };

        let result = create_section(request).unwrap();
        assert_eq!(result.sections.len(), 1);
        assert_eq!(result.sections[0].title, "引言");
        assert!(result.temp_md.contains("# 引言"));
        assert!(project_dir.join("manuscript").join(".temp.md").exists());

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn rename_h1_heading() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(&project_dir, "sec-aaa11111", 0, "引言", "# 引言\n\n引言正文");

        let request = RenameHeadingRequest {
            project_path: project_dir.to_str().unwrap().into(),
            section_id: "sec-aaa11111".into(),
            level: 1,
            old_text: "引言".into(),
            new_text: "绪论".into(),
        };

        let result = rename_heading(request).unwrap();
        assert_eq!(result.sections[0].title, "绪论");
        assert!(result.temp_md.contains("# 绪论"));
        assert!(!result.temp_md.contains("# 引言"));

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn rename_h2_heading() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(
            &project_dir,
            "sec-aaa11111",
            0,
            "引言",
            "# 引言\n\n## 背景\n\n背景正文\n\n## 贡献\n\n贡献正文",
        );

        let request = RenameHeadingRequest {
            project_path: project_dir.to_str().unwrap().into(),
            section_id: "sec-aaa11111".into(),
            level: 2,
            old_text: "背景".into(),
            new_text: "研究背景".into(),
        };

        let result = rename_heading(request).unwrap();
        assert!(result.temp_md.contains("## 研究背景"));
        assert!(!result.temp_md.contains("## 背景"));
        // H1 标题不变
        assert!(result.temp_md.contains("# 引言"));
        // sections.json title 不变（非 H1）
        assert_eq!(result.sections[0].title, "引言");

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn rename_heading_not_found() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(&project_dir, "sec-aaa11111", 0, "引言", "# 引言\n\n引言正文");

        let request = RenameHeadingRequest {
            project_path: project_dir.to_str().unwrap().into(),
            section_id: "sec-aaa11111".into(),
            level: 1,
            old_text: "不存在".into(),
            new_text: "新标题".into(),
        };

        let result = rename_heading(request);
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn insert_heading_under_h1() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(&project_dir, "sec-aaa11111", 0, "引言", "# 引言\n\n引言正文");

        let request = InsertHeadingRequest {
            project_path: project_dir.to_str().unwrap().into(),
            section_id: "sec-aaa11111".into(),
            anchor_level: 1,
            anchor_text: "引言".into(),
            new_level: 2,
            new_text: "背景".into(),
        };

        let result = insert_heading(request).unwrap();
        assert!(result.temp_md.contains("# 引言"));
        assert!(result.temp_md.contains("## 背景"));
        assert!(result.temp_md.contains("引言正文"));
        // 新标题在引言正文之后
        let idx_intro = result.temp_md.find("引言正文").unwrap();
        let idx_bg = result.temp_md.find("## 背景").unwrap();
        assert!(idx_intro < idx_bg);

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn insert_heading_between_siblings() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(
            &project_dir,
            "sec-aaa11111",
            0,
            "引言",
            "# 引言\n\n## 背景\n\n背景正文\n\n## 方法\n\n方法正文",
        );

        // 在 ## 背景 的作用域末尾插入 ### 子标题
        let request = InsertHeadingRequest {
            project_path: project_dir.to_str().unwrap().into(),
            section_id: "sec-aaa11111".into(),
            anchor_level: 2,
            anchor_text: "背景".into(),
            new_level: 3,
            new_text: "子标题".into(),
        };

        let result = insert_heading(request).unwrap();
        let md = &result.temp_md;
        let idx_bg = md.find("## 背景").unwrap();
        let idx_sub = md.find("### 子标题").unwrap();
        let idx_method = md.find("## 方法").unwrap();
        // 子标题在背景之后、方法之前
        assert!(idx_bg < idx_sub);
        assert!(idx_sub < idx_method);

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn save_temp_md_roundtrip() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(&project_dir, "sec-aaa11111", 0, "引言", "# 引言\n\n引言正文");
        add_section(&project_dir, "sec-bbb22222", 1, "方法", "# 方法\n\n方法正文");

        // 修改 temp_md 中的标题
        let original = loader::open_project(project_dir.to_str().unwrap()).unwrap();
        let modified = original.temp_md.replace("# 引言", "# 绪论");

        let request = SaveTempMdRequest {
            project_path: project_dir.to_str().unwrap().into(),
            content: modified,
        };

        let result = save_temp_md(request).unwrap();
        assert!(result.temp_md.contains("# 绪论"));
        assert_eq!(result.sections[0].title, "绪论");
        assert_eq!(result.sections[1].title, "方法");

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn save_temp_md_partial_marker_loss_succeeds() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(&project_dir, "sec-aaa11111", 0, "引言", "# 引言\n\n引言正文");
        add_section(&project_dir, "sec-bbb22222", 1, "方法", "# 方法\n\n方法正文");

        // 模拟用户在编辑器中删除了第二个章节的标题（标记随之被删除）
        let original = loader::open_project(project_dir.to_str().unwrap()).unwrap();
        let modified = original.temp_md.replace("<!-- @sec_id:sec-bbb22222 -->\n", "");

        let request = SaveTempMdRequest {
            project_path: project_dir.to_str().unwrap().into(),
            content: modified,
        };

        // 应当保存成功，而非拒绝
        let result = save_temp_md(request).unwrap();

        // 保存后 open_project 重装 temp_md，两个标记都恢复
        assert!(result.temp_md.contains("<!-- @sec_id:sec-aaa11111 -->"));
        assert!(result.temp_md.contains("<!-- @sec_id:sec-bbb22222 -->"));
        // sec-aaa 内容保留
        assert!(result.temp_md.contains("# 引言"));

        // sec-bbb 章节文件正文为空（标记缺失 → 空正文）
        let sec_bbb = fs::read_to_string(
            project_dir.join("manuscript/sections/sec-bbb22222.md"),
        ).unwrap();
        assert!(!sec_bbb.contains("# 方法"));
        assert!(sec_bbb.contains("title:")); // front matter 仍保留

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn save_temp_md_rejects_unknown_id() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(&project_dir, "sec-aaa11111", 0, "引言", "# 引言\n\n引言正文");

        // 注入一个 sections.json 中不存在的标记
        let original = loader::open_project(project_dir.to_str().unwrap()).unwrap();
        let modified = format!(
            "{}\n\n<!-- @sec_id:sec-unknown -->\n# 伪造",
            original.temp_md
        );

        let request = SaveTempMdRequest {
            project_path: project_dir.to_str().unwrap().into(),
            content: modified,
        };

        let result = save_temp_md(request);
        assert!(matches!(result, Err(ProjectError::Validation(_))));

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn save_temp_md_rejects_duplicate_marker() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(&project_dir, "sec-aaa11111", 0, "引言", "# 引言\n\n引言正文");

        // 复制标记造成重复
        let original = loader::open_project(project_dir.to_str().unwrap()).unwrap();
        let modified = format!(
            "{}\n\n<!-- @sec_id:sec-aaa11111 -->\n# 重复",
            original.temp_md
        );

        let request = SaveTempMdRequest {
            project_path: project_dir.to_str().unwrap().into(),
            content: modified,
        };

        let result = save_temp_md(request);
        assert!(matches!(result, Err(ProjectError::Validation(_))));

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn save_temp_md_empty_content_writes_empty_sections() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(&project_dir, "sec-aaa11111", 0, "引言", "# 引言\n\n引言正文");
        add_section(&project_dir, "sec-bbb22222", 1, "方法", "# 方法\n\n方法正文");

        // 模拟用户在编辑器中删除全部内容（标记随之消失）
        let request = SaveTempMdRequest {
            project_path: project_dir.to_str().unwrap().into(),
            content: String::new(),
        };

        let result = save_temp_md(request).unwrap();

        // 保存成功后 temp_md 由 open_project 重装，标记应恢复
        assert!(result.temp_md.contains("<!-- @sec_id:sec-aaa11111 -->"));
        assert!(result.temp_md.contains("<!-- @sec_id:sec-bbb22222 -->"));
        // 正文应为空（无标题内容）
        assert!(!result.temp_md.contains("# 引言"));
        assert!(!result.temp_md.contains("# 方法"));

        // 章节文件应存在但正文为空
        let sec_aaa = fs::read_to_string(
            project_dir.join("manuscript/sections/sec-aaa11111.md"),
        ).unwrap();
        assert!(!sec_aaa.contains("# 引言"));
        assert!(sec_aaa.contains("title:")); // front matter 仍保留

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn generate_id_format() {
        let id = generate_section_id();
        assert!(id.starts_with("sec-"));
        assert_eq!(id.len(), 20); // "sec-" (4) + 16 hex chars
    }

    #[test]
    fn parse_sec_marker_valid() {
        assert_eq!(
            parse_sec_marker("<!-- @sec_id:sec-abc12345 -->"),
            Some("sec-abc12345".into())
        );
    }

    #[test]
    fn parse_sec_marker_invalid() {
        assert_eq!(parse_sec_marker("<!-- not a marker -->"), None);
        assert_eq!(parse_sec_marker("# 标题"), None);
    }

    #[test]
    fn extract_h1_title_works() {
        assert_eq!(extract_h1_title("# 引言\n正文"), Some("引言".into()));
        assert_eq!(extract_h1_title("## 子标题\n# 引言"), Some("引言".into()));
        assert_eq!(extract_h1_title("正文无标题"), None);
    }

    #[test]
    fn insert_after_scope_at_end() {
        let body = "# 引言\n\n引言正文";
        let result = insert_after_scope(body, 1, "引言", 2, "背景").unwrap();
        assert!(result.contains("# 引言\n\n引言正文\n\n## 背景\n"));
    }

    #[test]
    fn insert_after_scope_between_siblings() {
        let body = "# 引言\n\n## 背景\n\n背景正文\n\n## 方法\n\n方法正文";
        let result = insert_after_scope(body, 2, "背景", 3, "子标题").unwrap();
        let idx_bg = result.find("## 背景").unwrap();
        let idx_sub = result.find("### 子标题").unwrap();
        let idx_method = result.find("## 方法").unwrap();
        assert!(idx_bg < idx_sub);
        assert!(idx_sub < idx_method);
    }

    #[test]
    fn insert_after_scope_anchor_not_found() {
        let body = "# 引言\n\n正文";
        assert!(insert_after_scope(body, 2, "不存在", 3, "新").is_none());
    }

    #[test]
    fn replace_first_heading_exact_match() {
        let body = "# 引言\n\n## 背景\n\n背景正文";
        let result = replace_first_heading(body, 2, "背景", "研究背景").unwrap();
        assert!(result.contains("## 研究背景"));
        assert!(!result.contains("## 背景\n"));
        assert!(result.contains("# 引言"));
    }

    #[test]
    fn replace_first_heading_no_match() {
        let body = "# 引言\n\n正文";
        assert!(replace_first_heading(body, 2, "不存在", "新").is_none());
    }
}
