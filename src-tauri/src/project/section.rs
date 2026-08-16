//! 章节级操作——创建章节、标题重命名、插入子标题、主文档 `main.md` 持久化。
//!
//! 所有操作完成后调用 [`loader::open_project`] 重新加载项目，
//! 保证 `main.md` 与各 `sec-{id}.md` 备份文件的数据一致性。
//!
//! ## 设计决策
//!
//! - **主文档为唯一真相源**：编辑器直接编辑 `manuscript/main.md` 全文；
//!   `sec-{id}.md` 是每个 H1 章节的备份（恢复数据源），仅在保存时从
//!   `main.md` 拆分同步。章节块以 `<!-- @sec_id:{id} -->` 标记定位。
//! - **保存前结构校验**：`save_document` 在写入前用 fluen-markup 解析并
//!   lint 整个 `main.md`，存在 `Severity::Error` 硬错误时拒绝保存。
//! - **ID 稳定匹配**：拆分后的章节块优先按标记 ID 匹配旧索引；标记缺失时
//!   按 H1 标题匹配复用旧 ID；都无法匹配时生成新 ID。被移除的章节
//!   （旧索引中存在但新 `main.md` 中消失）从索引删除并清理备份文件。
//! - **文本匹配定位**：重命名通过 `section_id` + `level` + `old_text`
//!   在章节块内匹配标题行，不依赖行号，避免编辑器偏移导致错位。
//! - **全量重载**：每次操作后调用 `loader::open_project`，确保数据一致。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::error::ProjectError;
use super::frontmatter;
use super::loader;
use super::model::{
    CreateSectionRequest, InsertHeadingRequest, OpenProjectResult, RenameHeadingRequest,
    SaveDocumentRequest, SectionFrontMatter, SectionMeta,
};

/// 章节标记前缀（与 `loader::SEC_MARKER_PREFIX` 保持一致）。
const SEC_MARKER_PREFIX: &str = "<!-- @sec_id:";

// ---------------------------------------------------------------------------
// 公开接口
// ---------------------------------------------------------------------------

/// 创建新章节（一级标题）。
///
/// 在 `main.md` 末尾追加 `<!-- @sec_id:{new-id} -->` + `# {title}` 块，
/// 生成 `sec-{UUID4}.md` 备份文件，更新 `sections.json`，重新加载项目。
pub fn create_section(request: CreateSectionRequest) -> Result<OpenProjectResult, ProjectError> {
    request.validate()?;

    let project_dir = PathBuf::from(&request.project_path);
    let section_id = generate_section_id();
    let now = chrono::Utc::now().to_rfc3339();

    // 0. 确保 main.md 就绪（旧项目自动迁移拼装）
    let mut sections = read_sections_index(&project_dir)?;
    let main_md = loader::ensure_main_md(&project_dir, &sections)?;

    // 1. 在 main.md 末尾追加新章节块（含标记 + H1）
    let block = format!("{}{} -->\n# {}\n", SEC_MARKER_PREFIX, section_id, request.title);
    let new_main = if main_md.trim().is_empty() {
        block
    } else {
        format!("{}\n\n{}", main_md.trim_end(), block)
    };
    loader::write_main_md(&project_dir, &new_main)?;

    // 2. 写备份文件
    let front_matter = SectionFrontMatter {
        title: request.title.clone(),
        title_html: String::new(),
        created: now.clone(),
        updated: now,
    };
    write_sec_file(&project_dir, &section_id, &front_matter, &format!("# {}\n", request.title))?;

    // 3. 更新 sections.json
    let new_order = sections.iter().map(|s| s.order).max().unwrap_or(0) + 1;
    sections.push(SectionMeta {
        id: section_id,
        order: new_order,
        title: request.title,
        title_html: None,
        references: vec![],
    });
    write_sections_index(&project_dir, &sections)?;

    // 4. 重新加载
    loader::open_project(&request.project_path)
}

/// 重命名标题（任意层级）。
///
/// 通过 `section_id` 定位 `main.md` 中的章节块，再以 `level` + `old_text`
/// **文本匹配**替换标题行。若为 H1 标题，同步更新 `sections.json` 与
/// 备份文件的 front matter `title`。
pub fn rename_heading(request: RenameHeadingRequest) -> Result<OpenProjectResult, ProjectError> {
    request.validate()?;

    let project_dir = PathBuf::from(&request.project_path);

    // 0. 确保 main.md 就绪（旧项目自动迁移拼装）
    let sections = read_sections_index(&project_dir)?;
    let main_md = loader::ensure_main_md(&project_dir, &sections)?;

    // 1. 定位章节块范围
    let (span_start, span_end) = find_section_span(&main_md, &request.section_id).ok_or_else(|| {
        ProjectError::Validation(format!("未找到章节: {}", request.section_id))
    })?;
    let section_text = main_md
        .lines()
        .skip(span_start)
        .take(span_end - span_start)
        .collect::<Vec<_>>()
        .join("\n");

    // 2. 在章节块内文本匹配替换标题行
    let new_section_text =
        replace_first_heading(&section_text, request.level, &request.old_text, &request.new_text)
            .ok_or_else(|| {
                ProjectError::Validation(format!(
                    "未找到匹配的标题: H{} \"{}\"",
                    request.level, request.old_text
                ))
            })?;

    // 3. 重建并写回 main.md
    let new_main = replace_span(&main_md, span_start, span_end, &new_section_text);
    loader::write_main_md(&project_dir, &new_main)?;

    // 4. 若为 H1，同步 sections.json 标题
    if request.level == 1 {
        let mut sections = read_sections_index(&project_dir)?;
        if let Some(meta) = sections.iter_mut().find(|s| s.id == request.section_id) {
            meta.title = request.new_text.clone();
        }
        write_sections_index(&project_dir, &sections)?;
    }

    // 5. 同步备份文件
    sync_section_backup(&project_dir, &new_section_text, &request.section_id)?;

    // 6. 重新加载
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

    // 0. 确保 main.md 就绪（旧项目自动迁移拼装）
    let sections = read_sections_index(&project_dir)?;
    let main_md = loader::ensure_main_md(&project_dir, &sections)?;

    // 1. 定位章节块范围
    let (span_start, span_end) = find_section_span(&main_md, &request.section_id).ok_or_else(|| {
        ProjectError::Validation(format!("未找到章节: {}", request.section_id))
    })?;
    let section_text = main_md
        .lines()
        .skip(span_start)
        .take(span_end - span_start)
        .collect::<Vec<_>>()
        .join("\n");

    // 2. 在锚点作用域末尾插入新标题（作用于含标记行的章节块文本）
    let new_section_text = insert_after_scope(
        &section_text,
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

    // 3. 重建并写回 main.md
    let new_main = replace_span(&main_md, span_start, span_end, &new_section_text);
    loader::write_main_md(&project_dir, &new_main)?;

    // 4. 同步备份文件
    sync_section_backup(&project_dir, &new_section_text, &request.section_id)?;

    // 5. 重新加载
    loader::open_project(&request.project_path)
}

/// 保存 `main.md` 全文——校验后持久化主文档并拆分回各章节备份。
///
/// **流程**：
/// 1. fluen-markup 解析 + lint 整个 `main.md`，存在 `Severity::Error`
///    硬错误（如重复 id、缺 caption）时**拒绝保存**。
/// 2. 按 `<!-- @sec_id:xxx -->` 标记与 H1 标题拆分章节块，保 ID 稳定：
///    - 标记 ID 在旧索引中存在 → 复用（标题变更不换 ID）
///    - 标记缺失 → 按 H1 标题匹配旧索引 → 复用旧 ID
///    - 均无法匹配 → 生成新 ID（新增章节）
///    - 旧索引中存在但新文档中消失 → 从索引移除并删除备份文件
/// 3. **归一化重建** `manuscript/main.md`（补齐标记、规整块间空行）后写入，
///    再更新各 `sec-{id}.md` 备份文件与 `sections.json`，最后重新加载项目。
///
/// **空内容**：用户在编辑器中删除全部内容后保存是合法操作——
/// 保留现有章节结构（ID 稳定），各章节正文清空，`main.md` 写空。
pub fn save_document(request: SaveDocumentRequest) -> Result<OpenProjectResult, ProjectError> {
    request.validate()?;

    let project_dir = PathBuf::from(&request.project_path);

    // 统一换行符为 \n（消除 \r\n 差异）
    let content = request.content.replace("\r\n", "\n").replace('\r', "\n");

    // 1. fluen-markup 校验：解析 + lint，存在硬错误时拒绝保存
    validate_markup(&content)?;

    // 2. 拆分章节块；无任何标记/H1 但内容非空时（新项目首存），整体作为首个章节
    let mut blocks = split_main_md(&content);
    if blocks.is_empty() && !content.trim().is_empty() {
        blocks.push(MainBlock {
            marker_id: None,
            body: content.trim().to_string(),
        });
    }

    // 3. 读取旧索引（用于 ID 稳定匹配与删除检测）
    let old_sections = read_sections_index(&project_dir)?;

    // 4. 空内容特判：保留章节结构，仅清空正文，main.md 写空
    if blocks.is_empty() {
        loader::write_main_md(&project_dir, "")?;
        let now = chrono::Utc::now().to_rfc3339();
        let mut new_sections = Vec::with_capacity(old_sections.len());
        for (order, meta) in old_sections.iter().enumerate() {
            let (mut fm, _) = read_section_file_parts(&project_dir, &meta.id)?;
            fm.updated = now.clone();
            write_sec_file(&project_dir, &meta.id, &fm, "")?;
            let mut kept = meta.clone();
            kept.order = order as u32;
            new_sections.push(kept);
        }
        write_sections_index(&project_dir, &new_sections)?;
        return loader::open_project(&request.project_path);
    }

    // 5. 按标记 + H1 标题匹配，保 ID 稳定
    let resolved = resolve_section_ids(&blocks, &old_sections);

    // 6. 归一化重建并写入 main.md（补齐标记、规整格式）
    let normalized = rebuild_main_md(&resolved);
    loader::write_main_md(&project_dir, &normalized)?;

    // 7. 写备份文件 + 重建 sections.json
    let now = chrono::Utc::now().to_rfc3339();
    let mut new_sections: Vec<SectionMeta> = Vec::with_capacity(resolved.len());
    for (order, rb) in resolved.iter().enumerate() {
        let body = rb.body.trim();
        let (created, title_html, references) = match &rb.old_meta {
            Some(meta) => {
                let (fm, _) = read_section_file_parts(&project_dir, &meta.id)?;
                (fm.created, fm.title_html, meta.references.clone())
            }
            None => (now.clone(), String::new(), vec![]),
        };
        let title = extract_h1_title(body)
            .unwrap_or_else(|| {
                rb.old_meta
                    .as_ref()
                    .map(|m| m.title.clone())
                    .unwrap_or_else(|| "未命名".to_string())
            });
        let front_matter = SectionFrontMatter {
            title: title.clone(),
            title_html,
            created,
            updated: now.clone(),
        };
        write_sec_file(&project_dir, &rb.id, &front_matter, body)?;
        new_sections.push(SectionMeta {
            id: rb.id.clone(),
            order: order as u32,
            title,
            title_html: None,
            references,
        });
    }

    // 8. 删除被移除章节的备份文件
    let kept_ids: HashSet<&str> = new_sections.iter().map(|s| s.id.as_str()).collect();
    for meta in &old_sections {
        if !kept_ids.contains(meta.id.as_str()) {
            let path = sections_dir(&project_dir).join(format!("{}.md", meta.id));
            if path.is_file() {
                std::fs::remove_file(path)?;
            }
        }
    }

    // 9. 写回 sections.json 并重载
    write_sections_index(&project_dir, &new_sections)?;
    loader::open_project(&request.project_path)
}

// ---------------------------------------------------------------------------
// 内部辅助函数
// ---------------------------------------------------------------------------

/// `main.md` 中的一个章节块（按标记 + H1 切分）。
struct MainBlock {
    /// 章节标记 ID（`<!-- @sec_id:xxx -->`），缺失时标题匹配兜底。
    marker_id: Option<String>,
    /// 块正文（从 H1 标题行开始，不含标记行）。
    body: String,
}

/// 已确定 ID 的章节块。
struct ResolvedBlock {
    /// 最终使用的章节 ID。
    id: String,
    /// 块正文。
    body: String,
    /// 复用的旧章节元信息（无则 `None`，表示新增章节）。
    old_meta: Option<SectionMeta>,
}

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

/// 写入单个章节备份文件（front matter + 正文）。
fn write_sec_file(
    project_dir: &Path,
    section_id: &str,
    front_matter: &SectionFrontMatter,
    body: &str,
) -> Result<(), ProjectError> {
    let content = frontmatter::join(front_matter, body)?;
    let path = sections_dir(project_dir).join(format!("{}.md", section_id));
    std::fs::write(path, content)?;
    Ok(())
}

/// fluen-markup 校验：解析 + lint，存在 `Severity::Error` 硬错误时拒绝保存。
fn validate_markup(content: &str) -> Result<(), ProjectError> {
    let doc = fluen_markup::parse::parse_document(content)
        .map_err(|e| ProjectError::Validation(format!("文档解析失败，拒绝保存: {}", e)))?;
    let problems = fluen_markup::validate::lint(&doc);
    for problem in &problems {
        if let fluen_markup::MarkupError::Lint {
            message,
            line,
            severity,
        } = problem
        {
            if *severity == fluen_markup::Severity::Error {
                return Err(ProjectError::Validation(format!(
                    "文档校验失败（第 {} 行）: {}，拒绝保存",
                    line, message
                )));
            }
        }
    }
    Ok(())
}

/// 按标记与 H1 标题将 `main.md` 切分为章节块。
///
/// 规则（与前端 `outlineParser.ts` 保持一致）：
/// - `<!-- @sec_id:xxx -->` 标记行**立即开启新章节块**（即使后续没有 H1 标题，
///   标记后的正文也不会丢失——章节保留，标题回退到旧值/「未命名」）
/// - `# 标题` 行（行首 `# `）开启新章节块；若当前块是"标记刚开启的空块"
///   （标记与标题相邻），则标题归入该块而非另开新块
/// - 其余行追加到当前章节块正文；无章节块的前导内容被忽略
fn split_main_md(content: &str) -> Vec<MainBlock> {
    let mut blocks: Vec<MainBlock> = Vec::new();
    let mut current: Option<MainBlock> = None;

    for line in content.lines() {
        // 标记行：开启新章节块（含标记）
        if let Some(id) = parse_sec_marker(line) {
            if let Some(b) = current.take() {
                blocks.push(b);
            }
            current = Some(MainBlock {
                marker_id: Some(id),
                body: String::new(),
            });
            continue;
        }
        // H1 标题行
        if is_h1_line(line) {
            // 标记刚开启的空块：H1 归入该块（标记与标题相邻）
            if let Some(b) = current.as_mut() {
                if b.body.is_empty() && b.marker_id.is_some() {
                    b.body = line.to_string();
                    continue;
                }
            }
            if let Some(b) = current.take() {
                blocks.push(b);
            }
            current = Some(MainBlock {
                marker_id: None,
                body: line.to_string(),
            });
            continue;
        }
        // 普通行：追加到当前块
        if let Some(b) = current.as_mut() {
            if !b.body.is_empty() {
                b.body.push('\n');
            }
            b.body.push_str(line);
        }
    }

    if let Some(b) = current.take() {
        blocks.push(b);
    }
    blocks
}

/// 判断行是否为 H1 标题（行首 `# `）。
fn is_h1_line(line: &str) -> bool {
    line.trim_start().starts_with("# ")
}

/// 为拆分出的章节块确定稳定 ID。
///
/// 匹配优先级：
/// 1. 标记 ID（未在本轮使用过）——显式声明，即使标题已变更也复用；
/// 2. H1 标题匹配旧索引（未在本轮使用过）——标记被删除时兜底；
/// 3. 均无法匹配 → 生成新 ID（新增章节）。
fn resolve_section_ids(blocks: &[MainBlock], old_sections: &[SectionMeta]) -> Vec<ResolvedBlock> {
    let mut by_id: HashMap<&str, &SectionMeta> = HashMap::new();
    let mut by_title: HashMap<&str, &SectionMeta> = HashMap::new();
    for meta in old_sections {
        by_id.insert(meta.id.as_str(), meta);
        by_title.insert(meta.title.as_str(), meta);
    }

    let mut used: HashSet<String> = HashSet::new();
    let mut result = Vec::with_capacity(blocks.len());

    for block in blocks {
        let h1 = extract_h1_title(&block.body);
        let mut id: Option<String> = None;

        // 1) 标记优先（未被使用过）
        if let Some(mid) = &block.marker_id {
            if !used.contains(mid) {
                id = Some(mid.clone());
            }
        }

        // 2) 标记缺失/被占用 → 按标题匹配旧索引
        if id.is_none() {
            if let Some(title) = &h1 {
                if let Some(meta) = by_title.get(title.as_str()) {
                    if !used.contains(&meta.id) {
                        id = Some(meta.id.clone());
                    }
                }
            }
        }

        // 3) 生成新 ID
        let id = id.unwrap_or_else(generate_section_id);
        used.insert(id.clone());
        let old_meta = by_id.get(id.as_str()).copied().cloned();

        result.push(ResolvedBlock {
            id,
            body: block.body.clone(),
            old_meta,
        });
    }

    result
}

/// 在 `main.md` 中定位章节块的行范围（含标记行，到下一个标记行前）。
///
/// 返回 `(start, end)` 行索引（`end` 不包含）；未找到时返回 `None`。
fn find_section_span(main_md: &str, section_id: &str) -> Option<(usize, usize)> {
    let lines: Vec<&str> = main_md.lines().collect();
    let target = format!("{}{} -->", SEC_MARKER_PREFIX, section_id);
    let start = lines.iter().position(|l| l.trim() == target)?;
    let end = lines[start + 1..]
        .iter()
        .position(|l| parse_sec_marker(l).is_some())
        .map(|i| start + 1 + i)
        .unwrap_or(lines.len());
    Some((start, end))
}

/// 用 `replacement` 替换 `main_md` 中 `[start, end)` 行范围。
fn replace_span(main_md: &str, start: usize, end: usize, replacement: &str) -> String {
    let lines: Vec<&str> = main_md.lines().collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len() + 2);
    result.extend(lines[..start].iter().map(|s| s.to_string()));
    result.extend(replacement.lines().map(|s| s.to_string()));
    result.extend(lines[end..].iter().map(|s| s.to_string()));
    result.join("\n")
}

/// 从章节块文本中去除首行的标记行，返回备份正文。
fn strip_first_marker_line(text: &str) -> String {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("");
    if parse_sec_marker(first).is_some() {
        lines.collect::<Vec<_>>().join("\n").trim().to_string()
    } else {
        text.trim().to_string()
    }
}

/// 按章节块顺序归一化重建 `main.md` 内容（补齐标记、规整块间空行）。
///
/// 与 `loader::assemble_main_md` 的拼装格式保持一致：
/// 每个章节块前加 `<!-- @sec_id:{id} -->` 标记，块间空行分隔。
fn rebuild_main_md(resolved: &[ResolvedBlock]) -> String {
    let mut parts = Vec::with_capacity(resolved.len());
    for rb in resolved {
        parts.push(format!(
            "{}{} -->\n{}",
            SEC_MARKER_PREFIX,
            rb.id,
            rb.body.trim()
        ));
    }
    parts.join("\n\n")
}

/// 同步单个章节的备份文件：正文取章节块文本（去标记行），
/// H1 标题同步到 front matter `title`。
fn sync_section_backup(
    project_dir: &Path,
    section_text: &str,
    section_id: &str,
) -> Result<(), ProjectError> {
    let body = strip_first_marker_line(section_text);
    let (mut fm, _) = read_section_file_parts(project_dir, section_id)?;
    if let Some(h1) = extract_h1_title(&body) {
        fm.title = h1;
    }
    fm.updated = chrono::Utc::now().to_rfc3339();
    write_sec_file(project_dir, section_id, &fm, &body)
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
            "fluen_section_test_{}_{:?}_{}",
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

        // 模拟旧版项目：无 main.md，由 loader 迁移拼装（creator 会创建空 main.md，
        // 此处移除以保证章节操作从 sections + sec-*.md 拼装主文档）
        let _ = fs::remove_file(project_dir.join("manuscript").join("main.md"));
    }

    /// 构造含标记的 main.md 内容（与 loader 拼装格式一致）。
    fn main_md_with(id: &str, title: &str, body: &str) -> String {
        format!("<!-- @sec_id:{} -->\n# {}\n\n{}", id, title, body)
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
        assert!(result.main_md.contains("<!-- @sec_id:sec-"));
        assert!(result.main_md.contains("# 引言"));
        assert!(project_dir.join("manuscript").join("main.md").exists());

        // 连续创建第二个章节，顺序追加
        let request2 = CreateSectionRequest {
            project_path: project_path.into(),
            title: "方法".into(),
        };
        let result2 = create_section(request2).unwrap();
        assert_eq!(result2.sections.len(), 2);
        assert_eq!(result2.sections[1].title, "方法");
        let idx_intro = result2.main_md.find("# 引言").unwrap();
        let idx_method = result2.main_md.find("# 方法").unwrap();
        assert!(idx_intro < idx_method);

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
        assert!(result.main_md.contains("# 绪论"));
        assert!(!result.main_md.contains("# 引言"));

        // 备份文件 front matter 与正文同步
        let sec_file = fs::read_to_string(
            project_dir.join("manuscript/sections/sec-aaa11111.md"),
        )
        .unwrap();
        assert!(sec_file.contains("title: 绪论"));
        assert!(sec_file.contains("# 绪论"));

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
        assert!(result.main_md.contains("## 研究背景"));
        assert!(!result.main_md.contains("## 背景"));
        // H1 标题不变
        assert!(result.main_md.contains("# 引言"));
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
        assert!(result.main_md.contains("# 引言"));
        assert!(result.main_md.contains("## 背景"));
        assert!(result.main_md.contains("引言正文"));
        // 新标题在引言正文之后
        let idx_intro = result.main_md.find("引言正文").unwrap();
        let idx_bg = result.main_md.find("## 背景").unwrap();
        assert!(idx_intro < idx_bg);

        // 备份文件同步
        let sec_file = fs::read_to_string(
            project_dir.join("manuscript/sections/sec-aaa11111.md"),
        )
        .unwrap();
        assert!(sec_file.contains("## 背景"));

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
        let md = &result.main_md;
        let idx_bg = md.find("## 背景").unwrap();
        let idx_sub = md.find("### 子标题").unwrap();
        let idx_method = md.find("## 方法").unwrap();
        // 子标题在背景之后、方法之前
        assert!(idx_bg < idx_sub);
        assert!(idx_sub < idx_method);

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn save_document_roundtrip() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(&project_dir, "sec-aaa11111", 0, "引言", "# 引言\n\n引言正文");
        add_section(&project_dir, "sec-bbb22222", 1, "方法", "# 方法\n\n方法正文");

        // 修改 main_md 中的标题（标记保留 → 复用旧 ID）
        let original = loader::open_project(project_dir.to_str().unwrap()).unwrap();
        let modified = original.main_md.replace("# 引言", "# 绪论");

        let request = SaveDocumentRequest {
            project_path: project_dir.to_str().unwrap().into(),
            content: modified,
        };

        let result = save_document(request).unwrap();
        assert!(result.main_md.contains("# 绪论"));
        assert_eq!(result.sections[0].title, "绪论");
        assert_eq!(result.sections[0].id, "sec-aaa11111"); // ID 稳定
        assert_eq!(result.sections[1].title, "方法");

        // main.md 落盘
        let main_file = fs::read_to_string(project_dir.join("manuscript/main.md")).unwrap();
        assert!(main_file.contains("# 绪论"));

        // 备份文件同步
        let sec_file = fs::read_to_string(
            project_dir.join("manuscript/sections/sec-aaa11111.md"),
        )
        .unwrap();
        assert!(sec_file.contains("# 绪论"));

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn save_document_removed_marker_keeps_id_by_title() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(&project_dir, "sec-aaa11111", 0, "引言", "# 引言\n\n引言正文");
        add_section(&project_dir, "sec-bbb22222", 1, "方法", "# 方法\n\n方法正文");

        // 模拟用户在编辑器中删除了第二个章节的标记（标题保留）
        let original = loader::open_project(project_dir.to_str().unwrap()).unwrap();
        let modified = original.main_md.replace("<!-- @sec_id:sec-bbb22222 -->\n", "");

        let request = SaveDocumentRequest {
            project_path: project_dir.to_str().unwrap().into(),
            content: modified,
        };

        // 按 H1 标题匹配：sec-bbb22222 应被保留且正文不丢失
        let result = save_document(request).unwrap();
        assert_eq!(result.sections.len(), 2);
        assert_eq!(result.sections[1].id, "sec-bbb22222");
        assert_eq!(result.sections[1].title, "方法");

        let sec_bbb = fs::read_to_string(
            project_dir.join("manuscript/sections/sec-bbb22222.md"),
        )
        .unwrap();
        assert!(sec_bbb.contains("# 方法"));

        // 保存归一化：main.md 标记被补齐（按标题匹配复用原 ID）
        assert!(result.main_md.contains("@sec_id:sec-bbb22222"));

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn save_document_unknown_marker_creates_new_section() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(&project_dir, "sec-aaa11111", 0, "引言", "# 引言\n\n引言正文");

        // 注入一个 sections.json 中不存在的标记 → 视为新增章节（保留显式 ID）
        let original = loader::open_project(project_dir.to_str().unwrap()).unwrap();
        let modified = format!(
            "{}\n\n<!-- @sec_id:sec-unknown -->\n# 伪造",
            original.main_md
        );

        let request = SaveDocumentRequest {
            project_path: project_dir.to_str().unwrap().into(),
            content: modified,
        };

        let result = save_document(request).unwrap();
        assert_eq!(result.sections.len(), 2);
        assert_eq!(result.sections[1].id, "sec-unknown");
        assert_eq!(result.sections[1].title, "伪造");

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn save_document_duplicate_marker_generates_new_id() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(&project_dir, "sec-aaa11111", 0, "引言", "# 引言\n\n引言正文");

        // 复制标记造成重复 → 第二个块生成新 ID，不互相覆盖
        let original = loader::open_project(project_dir.to_str().unwrap()).unwrap();
        let modified = format!(
            "{}\n\n<!-- @sec_id:sec-aaa11111 -->\n# 重复",
            original.main_md
        );

        let request = SaveDocumentRequest {
            project_path: project_dir.to_str().unwrap().into(),
            content: modified,
        };

        let result = save_document(request).unwrap();
        assert_eq!(result.sections.len(), 2);
        assert_eq!(result.sections[0].id, "sec-aaa11111");
        assert_ne!(result.sections[1].id, "sec-aaa11111");
        assert_eq!(result.sections[1].title, "重复");

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn save_document_removed_section_deletes_backup() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(&project_dir, "sec-aaa11111", 0, "引言", "# 引言\n\n引言正文");
        add_section(&project_dir, "sec-bbb22222", 1, "方法", "# 方法\n\n方法正文");

        // 用户删除整个第二个章节（标记 + 标题 + 正文）
        let original = loader::open_project(project_dir.to_str().unwrap()).unwrap();
        let modified = original
            .main_md
            .replace(
                "\n\n<!-- @sec_id:sec-bbb22222 -->\n# 方法\n\n方法正文",
                "",
            );

        let request = SaveDocumentRequest {
            project_path: project_dir.to_str().unwrap().into(),
            content: modified,
        };

        let result = save_document(request).unwrap();
        assert_eq!(result.sections.len(), 1);
        assert_eq!(result.sections[0].id, "sec-aaa11111");
        // 备份文件已删除
        assert!(!project_dir
            .join("manuscript/sections/sec-bbb22222.md")
            .exists());

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn save_document_empty_content_keeps_sections() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(&project_dir, "sec-aaa11111", 0, "引言", "# 引言\n\n引言正文");
        add_section(&project_dir, "sec-bbb22222", 1, "方法", "# 方法\n\n方法正文");

        // 用户删除全部内容
        let request = SaveDocumentRequest {
            project_path: project_dir.to_str().unwrap().into(),
            content: String::new(),
        };

        let result = save_document(request).unwrap();
        // main.md 为空
        assert_eq!(result.main_md, "");
        // 章节结构保留（ID 稳定）
        assert_eq!(result.sections.len(), 2);
        assert_eq!(result.sections[0].id, "sec-aaa11111");
        assert_eq!(result.sections[1].id, "sec-bbb22222");

        // 章节文件正文为空但 front matter 保留
        let sec_aaa = fs::read_to_string(
            project_dir.join("manuscript/sections/sec-aaa11111.md"),
        )
        .unwrap();
        assert!(!sec_aaa.contains("# 引言"));
        assert!(sec_aaa.contains("title:"));

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn save_document_auto_creates_first_section_when_project_has_none() {
        // 新项目无任何章节：用户直接输入内容保存，应自动创建首个章节而非静默丢弃
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);

        let request = SaveDocumentRequest {
            project_path: project_dir.to_str().unwrap().into(),
            content: "# 我的论文\n\n这是我的正文内容".into(),
        };

        let result = save_document(request).unwrap();

        // 自动创建了一个章节，标题取自首个 H1
        assert_eq!(result.sections.len(), 1);
        assert_eq!(result.sections[0].title, "我的论文");
        // main_md 含标记与正文，退出重进后内容可完整恢复
        assert!(result.main_md.contains("<!-- @sec_id:sec-"));
        assert!(result.main_md.contains("# 我的论文"));
        assert!(result.main_md.contains("这是我的正文内容"));

        // 章节文件已写入
        let section_files = fs::read_dir(project_dir.join("manuscript").join("sections")).unwrap();
        let md_count = section_files
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |x| x == "md"))
            .count();
        assert_eq!(md_count, 1);

        // 再次保存应走正常拆分路径（不再自动创建），内容保持
        let request2 = SaveDocumentRequest {
            project_path: project_dir.to_str().unwrap().into(),
            content: result.main_md.clone(),
        };
        let result2 = save_document(request2).unwrap();
        assert_eq!(result2.sections.len(), 1);
        assert!(result2.main_md.contains("# 我的论文"));
        assert!(result2.main_md.contains("这是我的正文内容"));

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn save_document_auto_created_section_uses_default_title_without_h1() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);

        let request = SaveDocumentRequest {
            project_path: project_dir.to_str().unwrap().into(),
            content: "没有标题的纯正文".into(),
        };

        let result = save_document(request).unwrap();
        assert_eq!(result.sections.len(), 1);
        assert_eq!(result.sections[0].title, "未命名");
        assert!(result.main_md.contains("没有标题的纯正文"));

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn save_document_rejects_lint_error() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(&project_dir, "sec-aaa11111", 0, "引言", "# 引言\n\n引言正文");

        // 重复 id 触发 lint Severity::Error
        let bad = format!(
            "{}\n\n<f-fig id=\"fig:a\" src=\"assets/a.png\">\n<f-caption>图一</f-caption>\n</f-fig>\n\n<f-fig id=\"fig:a\" src=\"assets/a.png\">\n<f-caption>图二</f-caption>\n</f-fig>",
            loader::open_project(project_dir.to_str().unwrap()).unwrap().main_md
        );

        let request = SaveDocumentRequest {
            project_path: project_dir.to_str().unwrap().into(),
            content: bad,
        };

        let result = save_document(request);
        assert!(matches!(result, Err(ProjectError::Validation(_))));

        // 拒绝保存：main.md 未被写入用户提交的含错误内容（保持迁移后的原样）
        let main_file = fs::read_to_string(project_dir.join("manuscript/main.md")).unwrap();
        assert!(main_file.contains("# 引言"));
        assert!(!main_file.contains("<f-fig"));

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn save_document_rejects_parse_error() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(&project_dir, "sec-aaa11111", 0, "引言", "# 引言\n\n引言正文");

        // <f-fig> 缺 <f-caption> 触发解析阶段硬错误
        let bad = format!(
            "{}\n\n<f-fig id=\"fig:b\" src=\"assets/b.png\">\n</f-fig>",
            loader::open_project(project_dir.to_str().unwrap()).unwrap().main_md
        );

        let request = SaveDocumentRequest {
            project_path: project_dir.to_str().unwrap().into(),
            content: bad,
        };

        let result = save_document(request);
        assert!(matches!(result, Err(ProjectError::Validation(_))));

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
    fn split_main_md_parses_markers_and_h1() {
        let content = "<!-- @sec_id:sec-aaa11111 -->\n# 引言\n\n引言正文\n\n<!-- @sec_id:sec-bbb22222 -->\n# 方法\n\n方法正文";
        let blocks = split_main_md(content);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].marker_id.as_deref(), Some("sec-aaa11111"));
        assert!(blocks[0].body.starts_with("# 引言"));
        assert!(blocks[0].body.contains("引言正文"));
        assert!(!blocks[0].body.contains("<!--"));
        assert_eq!(blocks[1].marker_id.as_deref(), Some("sec-bbb22222"));
        assert!(blocks[1].body.starts_with("# 方法"));
    }

    #[test]
    fn split_main_md_handles_markerless_h1() {
        // 用户手写 H1 无标记：marker_id 为 None
        let content = "# 手写章节\n\n内容";
        let blocks = split_main_md(content);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].marker_id.is_none());
        assert!(blocks[0].body.starts_with("# 手写章节"));
    }

    #[test]
    fn split_main_md_ignores_preamble() {
        // 标记之前的正文被忽略（正常 main.md 不应有）
        let content = "前导文本\n\n<!-- @sec_id:sec-aaa11111 -->\n# 引言\n\n正文";
        let blocks = split_main_md(content);
        assert_eq!(blocks.len(), 1);
        assert!(!blocks[0].body.contains("前导文本"));
    }

    #[test]
    fn split_main_md_keeps_body_when_h1_missing() {
        // 标记存在但 H1 被删（正文紧随标记）：章节块仍保留，正文不丢失
        let content =
            "<!-- @sec_id:sec-aaa11111 -->\n只剩正文没有标题了\n\n第二段正文";
        let blocks = split_main_md(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].marker_id.as_deref(), Some("sec-aaa11111"));
        assert!(!blocks[0].body.starts_with("# "));
        assert!(blocks[0].body.contains("只剩正文没有标题了"));
        assert!(blocks[0].body.contains("第二段正文"));
    }

    #[test]
    fn save_document_keeps_section_when_h1_deleted() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(&project_dir, "sec-aaa11111", 0, "引言", "# 引言\n\n引言正文");

        // 用户删除 H1 标题行（标记保留）：章节应保留、正文不丢、标题回退旧值
        let original = loader::open_project(project_dir.to_str().unwrap()).unwrap();
        let modified = original.main_md.replace("\n# 引言", "");

        let request = SaveDocumentRequest {
            project_path: project_dir.to_str().unwrap().into(),
            content: modified,
        };

        let result = save_document(request).unwrap();
        assert_eq!(result.sections.len(), 1);
        assert_eq!(result.sections[0].id, "sec-aaa11111"); // ID 稳定
        assert_eq!(result.sections[0].title, "引言"); // 标题回退旧值

        // 正文保留
        let sec_file = fs::read_to_string(
            project_dir.join("manuscript/sections/sec-aaa11111.md"),
        )
        .unwrap();
        assert!(sec_file.contains("引言正文"));

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn resolve_ids_prefers_marker_then_title_then_new() {
        let old_sections = vec![
            SectionMeta {
                id: "sec-aaa11111".into(),
                order: 0,
                title: "引言".into(),
                title_html: None,
                references: vec![],
            },
            SectionMeta {
                id: "sec-bbb22222".into(),
                order: 1,
                title: "方法".into(),
                title_html: None,
                references: vec![],
            },
        ];

        // 1) 标记存在 → 复用；2) 无标记同标题 → 复用；3) 全新标题 → 新 ID
        let blocks = vec![
            MainBlock {
                marker_id: Some("sec-aaa11111".into()),
                body: "# 绪论\n\n正文".into(), // 标题变了但标记在 → 仍复用
            },
            MainBlock {
                marker_id: None,
                body: "# 方法\n\n正文".into(),
            },
            MainBlock {
                marker_id: None,
                body: "# 全新章节\n\n正文".into(),
            },
        ];

        let resolved = resolve_section_ids(&blocks, &old_sections);
        assert_eq!(resolved.len(), 3);
        assert_eq!(resolved[0].id, "sec-aaa11111");
        assert!(resolved[0].old_meta.is_some());
        assert_eq!(resolved[1].id, "sec-bbb22222");
        assert_eq!(resolved[1].old_meta.as_ref().unwrap().title, "方法");
        assert!(resolved[2].id.starts_with("sec-"));
        assert_ne!(resolved[2].id, "sec-aaa11111");
        assert_ne!(resolved[2].id, "sec-bbb22222");
        assert!(resolved[2].old_meta.is_none());
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

    #[test]
    fn find_span_and_replace_span_work() {
        let md = main_md_with("sec-aaa11111", "引言", "引言正文")
            + "\n\n"
            + &main_md_with("sec-bbb22222", "方法", "方法正文");
        let (start, end) = find_section_span(&md, "sec-aaa11111").unwrap();
        assert_eq!(start, 0);
        assert!(end > 0);
        // span 只覆盖第一个章节
        let span = md.lines().skip(start).take(end - start).collect::<Vec<_>>().join("\n");
        assert!(span.contains("# 引言"));
        assert!(!span.contains("# 方法"));

        let replaced = replace_span(&md, start, end, "<!-- @sec_id:sec-aaa11111 -->\n# 绪论\n\n新正文");
        assert!(replaced.contains("# 绪论"));
        assert!(replaced.contains("# 方法"));
        assert!(!replaced.contains("# 引言"));
    }
}
