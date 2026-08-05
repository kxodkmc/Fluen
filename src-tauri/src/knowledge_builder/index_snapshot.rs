//! Index 快照实时提取（纯解析，无状态）。
//!
//! 每次构建任务从 `references/wiki/index.md` 实时解析为紧凑快照，
//! 屏蔽 wiki_id，仅保留类型标记 + 标题 + 标签名。
//!
//! ## 快照格式
//!
//! ```text
//! - [S] 综述标题
//! - [C] 数智化技术
//! - [E] 湖南农业大学
//! - #个性化学习
//! ```
//!
//! 每条约 8-15 tokens，320 条目约 3-5K tokens。
//!
//! ## 设计
//!
//! - **不存储、不缓存**：磁盘是唯一真相源，避免会话内缓存与磁盘不一致。
//! - **纯解析**：无 I/O 依赖，输入 `&str` 输出 [`IndexSnapshot`]，可单测。
//! - **屏蔽 ID**：AI 仅看到全局视野（标题 + 类型），不能直接操作 wiki_id。

use super::error::KnowledgeBuilderError;

/// 紧凑快照：分类的标题列表 + 标签列表。
///
/// 渲染后注入 Planning prompt，提供全局去重视图。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexSnapshot {
    pub summaries: Vec<String>,
    pub concepts: Vec<String>,
    pub entities: Vec<String>,
    pub tags: Vec<String>,
}

impl IndexSnapshot {
    /// 从 index.md 文本解析为快照。
    ///
    /// 解析规则：
    /// - `- [[summaries/...]]` → summaries
    /// - `- <@wiki-id>[[concepts/...]]` → concepts（屏蔽 `<@...>`）
    /// - `- <@wiki-id>[[entities/...]]` → entities
    /// - `- tag-id-名称` → tags（仅保留名称，去掉 `tag-id` 前缀）
    ///
    /// 标题提取：链接形如 `concepts/wiki-abc-数智化技术`，
    /// 去掉 `wiki-` 前缀与 16 位 UUID4，保留 `数智化技术`。
    pub fn parse(index_md: &str) -> Result<Self, KnowledgeBuilderError> {
        let mut snap = Self::default();
        for raw in index_md.lines() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(title) = extract_link_title(line, "summaries/") {
                snap.summaries.push(title);
            } else if let Some(title) = extract_link_title(line, "concepts/") {
                snap.concepts.push(title);
            } else if let Some(title) = extract_link_title(line, "entities/") {
                snap.entities.push(title);
            } else if let Some(tag) = extract_tag_name(line) {
                snap.tags.push(tag);
            }
        }
        Ok(snap)
    }

    /// 渲染为紧凑文本，注入 Planning prompt。
    ///
    /// 按类型分组，每行一条，类型标记 `[S]/[C]/[E]/#`。
    /// 空分类不输出。
    pub fn render(&self) -> String {
        let mut out = String::new();
        for t in &self.summaries {
            out.push_str("- [S] ");
            out.push_str(t);
            out.push('\n');
        }
        for t in &self.concepts {
            out.push_str("- [C] ");
            out.push_str(t);
            out.push('\n');
        }
        for t in &self.entities {
            out.push_str("- [E] ");
            out.push_str(t);
            out.push('\n');
        }
        for t in &self.tags {
            out.push_str("- #");
            out.push_str(t);
            out.push('\n');
        }
        out
    }

    /// 估算快照占用 tokens（粗略：每条 10 tokens）。
    pub fn estimated_tokens(&self) -> usize {
        let count = self.summaries.len()
            + self.concepts.len()
            + self.entities.len()
            + self.tags.len();
        count.saturating_mul(10)
    }

    /// 条目总数。
    pub fn total_entries(&self) -> usize {
        self.summaries.len() + self.concepts.len() + self.entities.len()
    }
}

/// 从 `[[type/...]]` 链接中提取标题。
///
/// 输入行可能形如：
/// - `- [[summaries/wiki-xyz-综述]]`
/// - `- <@wiki-def>[[entities/wiki-def-湖南农业大学]]`
///
/// 提取 `[[` 与 `]]` 之间的内容，按 `type_prefix` 匹配后剥离
/// `wiki-{16hex}-` 前缀，返回剩余标题。
fn extract_link_title(line: &str, type_prefix: &str) -> Option<String> {
    let start = line.find("[[")?;
    let end = line.rfind("]]")?;
    if end <= start {
        return None;
    }
    let inner = &line[start + 2..end];
    let trimmed = inner.strip_prefix(type_prefix)?;
    Some(strip_wiki_id_prefix(trimmed))
}

/// 剥离 `wiki-{16hex}-` 前缀，返回标题。
///
/// 形如 `wiki-abc12345def67890-数智化技术` → `数智化技术`。
/// 若不符合该模式（无 16 位 UUID），原样返回。
fn strip_wiki_id_prefix(s: &str) -> String {
    if let Some(rest) = s.strip_prefix("wiki-") {
        // wiki- 后接 16 位 hex 与一个分隔符 `-`
        let hex_len = 16usize;
        if rest.len() > hex_len + 1 {
            let (hex, tail) = rest.split_at(hex_len);
            if hex.chars().all(|c| c.is_ascii_hexdigit()) {
                if let Some(title) = tail.strip_prefix('-') {
                    return title.to_string();
                }
            }
        }
    }
    s.to_string()
}

/// 从 `- tag-id-名称` 行提取标签名（去掉 `tag-{16hex}-` 前缀）。
fn extract_tag_name(line: &str) -> Option<String> {
    let line = line.strip_prefix("- ")?;
    let rest = line.strip_prefix("tag-")?;
    let hex_len = 16usize;
    if rest.len() > hex_len + 1 {
        let (hex, tail) = rest.split_at(hex_len);
        if hex.chars().all(|c| c.is_ascii_hexdigit()) {
            if let Some(name) = tail.strip_prefix('-') {
                return Some(name.to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_index_md() -> &'static str {
        r#"---
title: 知识库索引
type: index
---

# index
## Summaries
- [[summaries/wiki-abc12345def67890-文献综述]]
## Concepts
- <@wiki-def12345abc67890>[[concepts/wiki-def12345abc67890-数智化技术]]
- <@wiki-1111222233334444>[[concepts/wiki-1111222233334444-机器学习]]
## Entities
- <@wiki-aaaabbbbccccdddd>[[entities/wiki-aaaabbbbccccdddd-湖南农业大学]]
# Tags
- tag-aaaa1111bbbb2222-个性化学习
- tag-cccc3333dddd4444-中职生
"#
    }

    #[test]
    fn parse_extracts_all_categories() {
        let snap = IndexSnapshot::parse(sample_index_md()).unwrap();
        assert_eq!(snap.summaries, vec!["文献综述"]);
        assert_eq!(snap.concepts, vec!["数智化技术", "机器学习"]);
        assert_eq!(snap.entities, vec!["湖南农业大学"]);
        assert_eq!(snap.tags, vec!["个性化学习", "中职生"]);
    }

    #[test]
    fn parse_empty_string_yields_empty_snapshot() {
        let snap = IndexSnapshot::parse("").unwrap();
        assert_eq!(snap, IndexSnapshot::default());
    }

    #[test]
    fn parse_skips_non_link_lines() {
        let md = "# index\n## Concepts\n一些描述文本\n- <@wiki-abc12345def67890>[[concepts/wiki-abc12345def67890-测试]]";
        let snap = IndexSnapshot::parse(md).unwrap();
        assert_eq!(snap.concepts, vec!["测试"]);
    }

    #[test]
    fn render_uses_compact_format() {
        let snap = IndexSnapshot {
            summaries: vec!["综述".into()],
            concepts: vec!["数智化".into()],
            entities: vec!["湖南农大".into()],
            tags: vec!["个性化".into()],
        };
        let rendered = snap.render();
        assert!(rendered.contains("- [S] 综述"));
        assert!(rendered.contains("- [C] 数智化"));
        assert!(rendered.contains("- [E] 湖南农大"));
        assert!(rendered.contains("- #个性化"));
    }

    #[test]
    fn render_skips_empty_categories() {
        let snap = IndexSnapshot {
            concepts: vec!["机器学习".into()],
            ..Default::default()
        };
        let rendered = snap.render();
        assert!(rendered.contains("- [C] 机器学习"));
        assert!(!rendered.contains("[S]"));
        assert!(!rendered.contains("[E]"));
        assert!(!rendered.contains("#"));
    }

    #[test]
    fn estimated_tokens_is_count_times_ten() {
        let snap = IndexSnapshot {
            summaries: vec!["a".into()],
            concepts: vec!["b".into(), "c".into()],
            entities: vec!["d".into()],
            tags: vec!["e".into(), "f".into()],
        };
        assert_eq!(snap.estimated_tokens(), 60);
    }

    #[test]
    fn total_entries_excludes_tags() {
        let snap = IndexSnapshot {
            summaries: vec!["a".into()],
            concepts: vec!["b".into()],
            entities: vec!["c".into()],
            tags: vec!["t1".into(), "t2".into()],
        };
        assert_eq!(snap.total_entries(), 3);
    }

    #[test]
    fn strip_wiki_id_prefix_handles_short_strings() {
        // 无 16 位 hex 前缀的链接原样保留标题部分
        assert_eq!(strip_wiki_id_prefix("短标题"), "短标题");
    }

    #[test]
    fn extract_tag_name_rejects_non_tag_lines() {
        assert!(extract_tag_name("- [S] 综述").is_none());
        assert!(extract_tag_name("普通文本").is_none());
    }
}
