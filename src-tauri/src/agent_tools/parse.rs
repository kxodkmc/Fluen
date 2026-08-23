//! 论文内容解析——从 `main.md` 提取大纲与章节内容的纯函数层。
//!
//! 供智能体工具 [`crate::agent_tools::paper::outline::PaperOutlineTool`] /
//! [`crate::agent_tools::paper::section::PaperSectionTool`] 复用，
//! 解析规则与前端 `outlineParser.ts` 保持一致：
//! - 识别 `#` ~ `######` 开头的行为 H1-H6 标题
//! - 忽略代码块内的 `#` 行（``` / ~~~ 围栏包裹）
//! - 忽略章节标记行（`<!-- @sec_id:xxx -->`）
//!
//! 本模块不依赖 confluent，仅操作字符串与 `serde_json`，便于单元测试。

use serde::Serialize;

/// 章节标记行前缀（与 loader / section 保持一致）。
const SEC_MARKER_PREFIX: &str = "<!-- @sec_id:";

/// 标题行前缀（1-6 个 `#`）。
fn parse_heading(line: &str) -> Option<(u8, String)> {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &trimmed[hashes..];
    // 前端规则：`#{1,6}\s+...`，`#` 后必须跟空白
    let mut chars = rest.chars();
    if chars.next().map_or(true, |c| !c.is_whitespace()) {
        return None;
    }
    let text = rest.trim();
    if text.is_empty() {
        return None;
    }
    Some((hashes as u8, text.to_string()))
}

/// 判断是否为代码围栏行（3+ 反引号或波浪线）。
fn is_fence(line: &str) -> bool {
    let trimmed = line.trim();
    let backticks = trimmed.chars().take_while(|c| *c == '`').count();
    let tildes = trimmed.chars().take_while(|c| *c == '~').count();
    backticks >= 3 || tildes >= 3
}

/// 判断是否为章节标记行。
fn is_sec_marker(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with(SEC_MARKER_PREFIX) && trimmed.ends_with("-->")
}

/// 去除 `main.md` 中的章节标记行。
pub fn strip_sec_markers(md: &str) -> String {
    md.lines()
        .filter(|line| !is_sec_marker(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 扁平标题节点（含行号，供章节提取定位）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// 标题层级（1-6）。
    pub level: u8,
    /// 标题纯文本。
    pub text: String,
    /// 在文档中的行号（0-based）。
    pub line: usize,
}

/// 从 `main.md` 中解析出扁平的标题列表（按行序）。
pub fn parse_headings(md: &str) -> Vec<Heading> {
    let mut headings = Vec::new();
    let mut in_fence = false;

    for (i, line) in md.lines().enumerate() {
        if is_fence(line) {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || is_sec_marker(line) {
            continue;
        }
        if let Some((level, text)) = parse_heading(line) {
            headings.push(Heading { level, text, line: i });
        }
    }
    headings
}

/// 大纲树节点。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OutlineNode {
    /// 标题层级（1-6）。
    pub level: u8,
    /// 标题纯文本。
    pub text: String,
    /// 子节点（更深层级的后续标题）。
    pub children: Vec<OutlineNode>,
}

/// 将扁平标题列表组装为大纲树（栈算法，与前端 `buildTree` 一致）。
///
/// 两阶段构建：先用索引记录父子关系，再据此物化树节点，
/// 避免在克隆副本上挂子节点导致子树丢失。
fn build_tree(headings: &[Heading]) -> Vec<OutlineNode> {
    // 阶段 1：计算每个节点的父索引
    let mut parents: Vec<Option<usize>> = vec![None; headings.len()];
    let mut stack: Vec<usize> = Vec::new();

    for idx in 0..headings.len() {
        // 弹出栈顶直到找到比当前层级更浅的节点
        while let Some(&top) = stack.last() {
            if headings[top].level >= headings[idx].level {
                stack.pop();
            } else {
                break;
            }
        }
        parents[idx] = stack.last().copied();
        stack.push(idx);
    }

    // 阶段 2：按父关系物化节点（children 数组先建好占位）
    let mut children_of: Vec<Vec<usize>> = vec![Vec::new(); headings.len()];
    for (idx, parent) in parents.iter().enumerate() {
        if let Some(parent) = parent {
            children_of[*parent].push(idx);
        }
    }

    fn materialize(idx: usize, headings: &[Heading], children_of: &[Vec<usize>]) -> OutlineNode {
        OutlineNode {
            level: headings[idx].level,
            text: headings[idx].text.clone(),
            children: children_of[idx]
                .iter()
                .map(|&c| materialize(c, headings, children_of))
                .collect(),
        }
    }

    (0..headings.len())
        .filter(|&i| parents[i].is_none())
        .map(|i| materialize(i, headings, &children_of))
        .collect()
}

/// 从 `main.md` 构建完整大纲树。
pub fn build_outline(md: &str) -> Vec<OutlineNode> {
    build_tree(&parse_headings(md))
}

/// 章节提取结果。
#[derive(Debug, Clone)]
pub struct ExtractedSection {
    /// 匹配到的标题。
    pub heading: Heading,
    /// 章节内容（含标题行、全部子章节，去除了章节标记）。
    pub content: String,
}

/// 章节提取错误（含可用标题列表，便于智能体重试）。
#[derive(Debug, Clone)]
pub struct SectionError {
    /// 错误描述。
    pub message: String,
}

impl std::fmt::Display for SectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// 解析章节引用，如 `# 引言`（level=1）或 `## 背景`（level=2）。
///
/// 不带 `#` 前缀时 `level` 为 `None`（匹配任意层级），`text` 为去除前缀后的标题文本。
fn parse_heading_reference(reference: &str) -> (Option<u8>, String) {
    let trimmed = reference.trim();
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if hashes == 0 {
        (None, trimmed.to_string())
    } else {
        let rest = trimmed[hashes..].trim();
        (Some(hashes as u8), rest.to_string())
    }
}

/// 从 `main.md` 中提取指定章节（含全部子章节）的内容。
///
/// `reference` 形如 `# 大章节`（获取该一级章节及子章节）或 `## 二级章节`
/// （获取该二级章节及子章节）。也可省略 `#` 前缀仅按标题文本匹配。
/// 返回从匹配标题行到下一个同级或更浅标题行（不含）之间的内容。
pub fn extract_section(md: &str, reference: &str) -> Result<ExtractedSection, SectionError> {
    let (level_hint, text) = parse_heading_reference(reference);
    if text.is_empty() {
        return Err(SectionError {
            message: "章节引用不能为空".into(),
        });
    }

    let headings = parse_headings(md);
    let matched = headings.iter().find(|h| {
        h.text == text && (level_hint.is_none_or(|lv| h.level == lv))
    });

    let Some(matched) = matched else {
        let available = headings
            .iter()
            .map(|h| format!("{} {}", "#".repeat(h.level as usize), h.text))
            .collect::<Vec<_>>()
            .join("、");
        let available = if available.is_empty() {
            "（当前论文没有任何标题）".to_string()
        } else {
            format!("可用标题：{}", available)
        };
        return Err(SectionError {
            message: format!("未找到章节「{}」({})", reference.trim(), available),
        });
    };

    // 结束行：下一个同级或更浅的标题行（不含该行）
    let end_line = headings
        .iter()
        .find(|h| h.line > matched.line && h.level <= matched.level)
        .map(|h| h.line)
        .unwrap_or_else(|| md.lines().count());

    let content = md
        .lines()
        .enumerate()
        .filter(|(i, _)| *i >= matched.line && *i < end_line)
        .map(|(_, line)| line)
        .collect::<Vec<_>>()
        .join("\n");

    Ok(ExtractedSection {
        heading: matched.clone(),
        content: strip_sec_markers(&content),
    })
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_MD: &str = "<!-- @sec_id:sec-aaa11111 -->\n\
# 引言\n\
引言正文第一行\n\
\n\
## 研究背景\n\
背景内容\n\
\n\
### 国内现状\n\
国内部分\n\
\n\
## 研究意义\n\
意义内容\n\
\n\
<!-- @sec_id:sec-bbb22222 -->\n\
# 方法\n\
方法正文\n\
\n\
```\n\
# 代码块中的标题不应解析\n\
```\n";

    #[test]
    fn parse_headings_skips_fence_and_markers() {
        let headings = parse_headings(SAMPLE_MD);
        let texts: Vec<&str> = headings.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["引言", "研究背景", "国内现状", "研究意义", "方法"]
        );
        assert_eq!(headings[0].level, 1);
        assert_eq!(headings[1].level, 2);
        assert_eq!(headings[4].level, 1);
    }

    #[test]
    fn build_outline_builds_tree() {
        let tree = build_outline(SAMPLE_MD);
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].text, "引言");
        assert_eq!(tree[0].children.len(), 2);
        assert_eq!(tree[0].children[0].text, "研究背景");
        assert_eq!(tree[0].children[0].children[0].text, "国内现状");
        assert_eq!(tree[1].text, "方法");
        assert!(tree[1].children.is_empty());
    }

    #[test]
    fn extract_h1_section_includes_subsections() {
        let section = extract_section(SAMPLE_MD, "# 引言").unwrap();
        assert!(section.content.starts_with("# 引言"));
        assert!(section.content.contains("## 研究背景"));
        assert!(section.content.contains("### 国内现状"));
        assert!(section.content.contains("## 研究意义"));
        assert!(!section.content.contains("# 方法"));
    }

    #[test]
    fn extract_h2_section_includes_its_children() {
        let section = extract_section(SAMPLE_MD, "## 研究背景").unwrap();
        assert!(section.content.starts_with("## 研究背景"));
        assert!(section.content.contains("### 国内现状"));
        assert!(!section.content.contains("## 研究意义"));
    }

    #[test]
    fn extract_section_without_hash_matches_any_level() {
        let section = extract_section(SAMPLE_MD, "研究意义").unwrap();
        assert!(section.content.starts_with("## 研究意义"));
    }

    #[test]
    fn extract_section_not_found_lists_available() {
        let err = extract_section(SAMPLE_MD, "# 不存在").unwrap_err();
        assert!(err.message.contains("未找到章节"));
        assert!(err.message.contains("可用标题"));
        assert!(err.message.contains("引言"));
    }

    #[test]
    fn extract_section_empty_reference_rejected() {
        let err = extract_section(SAMPLE_MD, "   ").unwrap_err();
        assert!(err.message.contains("不能为空"));
    }

    #[test]
    fn strip_markers_removes_sec_id_lines() {
        let cleaned = strip_sec_markers(SAMPLE_MD);
        assert!(!cleaned.contains("@sec_id"));
        assert!(cleaned.contains("# 引言"));
    }

    #[test]
    fn is_none_or_available() {
        // 验证 Option::is_none_or 语义在匹配中的使用（level 精确匹配）
        let (level, _) = parse_heading_reference("# 引言");
        assert_eq!(level, Some(1));
        let (level, text) = parse_heading_reference("引言");
        assert_eq!(level, None);
        assert_eq!(text, "引言");
    }
}
