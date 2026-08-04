use serde::{Deserialize, Serialize};

use crate::error::{KnowledgeError, Result};
use crate::types::WikiType;

/// YAML frontmatter 结构（对应 wiki.md 规范）。
///
/// ```yaml
/// ---
/// title: 数智化技术
/// type: concept
/// source: raw/ref-xxx.pdf       # 仅 summaries
/// authors: ["wiki-xxx"]         # 仅 summaries
/// tags: [tag-xxx, tag-yyy]
/// created: 2026-06-14T03:31:33+00:00
/// updated: 2026-06-14T03:35:00+00:00
/// ---
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiFrontMatter {
    pub title: String,
    #[serde(rename = "type")]
    pub wiki_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub created: String,
    pub updated: String,
}

/// 将完整 Markdown 文件内容拆分为 (frontmatter_str, body_str)。
///
/// frontmatter 以 `---` 开头和结尾。若无 frontmatter，返回 ("", 原文)。
pub fn split_front_matter(content: &str) -> (&str, &str) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return ("", content);
    }

    let rest = &trimmed[3..]; // 跳过开头 "---"
    // 寻找闭合的 "\n---"
    if let Some(end) = rest.find("\n---") {
        let front_matter = &rest[..end];
        // 跳过 "\n---" 后的内容
        let after = &rest[end + 4..];
        // 跳过紧跟的换行
        let body = after.trim_start_matches('\n');
        (front_matter.trim(), body)
    } else {
        ("", content)
    }
}

/// 解析 frontmatter 字符串为 `WikiFrontMatter`。
pub fn parse_front_matter(fm_str: &str) -> Result<WikiFrontMatter> {
    let fm: WikiFrontMatter = serde_yaml::from_str(fm_str)?;
    Ok(fm)
}

/// 序列化 `WikiFrontMatter` 为 YAML 字符串（不含 `---` 包裹）。
pub fn serialize_front_matter(fm: &WikiFrontMatter) -> Result<String> {
    Ok(serde_yaml::to_string(fm)?)
}

/// 构造完整的 Markdown 文件内容（frontmatter + body）。
pub fn build_markdown(fm: &WikiFrontMatter, body: &str) -> Result<String> {
    let fm_str = serialize_front_matter(fm)?;
    Ok(format!("---\n{}---\n\n{}", fm_str, body))
}

/// 从 frontmatter 字符串解析 wiki_type。
pub fn parse_wiki_type(fm_str: &str) -> Result<WikiType> {
    let fm = parse_front_matter(fm_str)?;
    WikiType::from_str(&fm.wiki_type).ok_or_else(|| {
        KnowledgeError::Invalid(format!("unknown wiki type: {}", fm.wiki_type))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_front_matter() {
        let content = "---\ntitle: Test\ntype: concept\n---\n\n# Body";
        let (fm, body) = split_front_matter(content);
        assert!(fm.contains("title: Test"));
        assert!(body.starts_with("# Body"));
    }

    #[test]
    fn test_split_no_front_matter() {
        let content = "# Just body";
        let (fm, body) = split_front_matter(content);
        assert_eq!(fm, "");
        assert_eq!(body, "# Just body");
    }

    #[test]
    fn test_parse_front_matter() {
        let fm_str = "title: 数智化技术\ntype: concept\ntags:\n  - tag-xxx\ncreated: '2026-06-14T03:31:33+00:00'\nupdated: '2026-06-14T03:35:00+00:00'";
        let fm = parse_front_matter(fm_str).unwrap();
        assert_eq!(fm.title, "数智化技术");
        assert_eq!(fm.wiki_type, "concept");
        assert_eq!(fm.tags, vec!["tag-xxx"]);
    }

    #[test]
    fn test_build_markdown_roundtrip() {
        let test_title = "测试";
        let test_type = "concept";
        let test_body = "正文";
        let test_tag = "tag-xxx";
        let created_time = "2026-06-14T03:31:33+00:00";
        let updated_time = "2026-06-14T03:35:00+00:00";

        let front_matter = WikiFrontMatter {
            title: test_title.into(),
            wiki_type: test_type.into(),
            source: None,
            authors: vec![],
            tags: vec![test_tag.into()],
            created: created_time.into(),
            updated: updated_time.into(),
        };

        let markdown_content = format!("# {}\n\n{}", test_title, test_body);
        let generated_markdown = build_markdown(&front_matter, &markdown_content).unwrap();

        let (front_matter_str, body_content) = split_front_matter(&generated_markdown);
        let parsed_front_matter = parse_front_matter(front_matter_str).unwrap();

        assert_eq!(parsed_front_matter.title, test_title);
        assert_eq!(parsed_front_matter.wiki_type, test_type);
        assert!(body_content.contains(test_body));
    }
}