use std::path::Path;

use crate::error::Result;
use crate::frontmatter::{build_markdown, WikiFrontMatter};
use crate::types::{WikiEntry, WikiTag, WikiType};
use crate::util::atomic_write;

/// index.md 的 frontmatter。
#[derive(Debug, Clone)]
pub struct IndexFrontMatter {
    pub title: String,
    pub created: String,
    pub updated: String,
}

/// 生成 index.md 的完整内容。
///
/// 结构：
/// ```markdown
/// ---
/// title: 标题
/// type: index
/// created: ...
/// updated: ...
/// ---
///
/// # index
/// ## Summaries
/// - [[summaries/wikiID-标题]]
/// ## Concepts
/// - <@wikiID>[[concepts/wikiID-标题]]
/// ## Entities
/// - <@wikiID>[[entities/wikiID-标题]]
/// # Tags
/// - tagID-名称
/// ```
pub fn build_index_md(
    fm: &IndexFrontMatter,
    summaries: &[&WikiEntry],
    concepts: &[&WikiEntry],
    entities: &[&WikiEntry],
    tags: &[WikiTag],
) -> Result<String> {
    let mut body = String::new();

    body.push_str("# index\n");

    // Summaries
    body.push_str("## Summaries\n");
    for entry in summaries {
        let link = build_entry_link(entry);
        body.push_str(&format!("- [[{}]]\n", link));
    }

    // Concepts
    body.push_str("## Concepts\n");
    for entry in concepts {
        let link = build_entry_link(entry);
        body.push_str(&format!("- <@{}>[[{}]]\n", entry.id, link));
    }

    // Entities
    body.push_str("## Entities\n");
    for entry in entities {
        let link = build_entry_link(entry);
        body.push_str(&format!("- <@{}>[[{}]]\n", entry.id, link));
    }

    // Tags
    body.push_str("# Tags\n");
    for tag in tags {
        body.push_str(&format!("- {}-{}\n", tag.id, tag.title));
    }

    // 构造 frontmatter（type 固定为 index）
    let wiki_fm = WikiFrontMatter {
        title: fm.title.clone(),
        wiki_type: "index".into(),
        source: None,
        authors: vec![],
        tags: vec![],
        created: fm.created.clone(),
        updated: fm.updated.clone(),
    };

    build_markdown(&wiki_fm, &body).map_err(Into::into)
}

/// 构造条目在 index.md 中的链接路径。
///
/// 格式：`{type_dir}/{filename_without_ext}`
/// 如：`concepts/wiki-xxx-数智化技术`
fn build_entry_link(entry: &WikiEntry) -> String {
    // file_path 形如 "wiki/concepts/wiki-xxx-标题.md"
    // 需要转为 "concepts/wiki-xxx-标题"
    let path = entry.file_path.trim_start_matches("wiki/");
    path.trim_end_matches(".md").to_string()
}

/// 读取 index.md 文件（若存在）。
pub fn read_index_md(wiki_dir: &Path) -> Result<Option<String>> {
    let path = wiki_dir.join("index.md");
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    Ok(Some(content))
}

/// 写入 index.md 文件（原子写入）。
pub fn write_index_md(wiki_dir: &Path, content: &str) -> Result<()> {
    let path = wiki_dir.join("index.md");
    atomic_write(&path, content.as_bytes())
}

/// 从 DB 数据重建并写入 index.md。
///
/// 需要传入所有条目（按类型分组）和所有标签。
pub fn rebuild_index_md(
    wiki_dir: &Path,
    title: &str,
    entries: &[WikiEntry],
    tags: &[WikiTag],
) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();

    let summaries: Vec<&WikiEntry> = entries.iter().filter(|e| e.wiki_type == WikiType::Summary).collect();
    let concepts: Vec<&WikiEntry> = entries.iter().filter(|e| e.wiki_type == WikiType::Concept).collect();
    let entities: Vec<&WikiEntry> = entries.iter().filter(|e| e.wiki_type == WikiType::Entity).collect();

    let fm = IndexFrontMatter {
        title: title.to_string(),
        created: now.clone(),
        updated: now,
    };

    let content = build_index_md(&fm, &summaries, &concepts, &entities, tags)?;
    write_index_md(wiki_dir, &content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_index_md() {
        let fm = IndexFrontMatter {
            title: "测试知识库".into(),
            created: "2026-06-14T03:31:33+00:00".into(),
            updated: "2026-06-14T03:35:00+00:00".into(),
        };

        let summary = WikiEntry {
            id: "wiki-aaa".into(),
            wiki_type: WikiType::Summary,
            title: "综述".into(),
            file_path: "wiki/summaries/wiki-aaa-综述.md".into(),
            source: Some("raw/ref-xxx.pdf".into()),
            authors: vec![],
            tags: vec![],
            relations: vec![],
            content: String::new(),
            created: "2026-06-14T03:31:33+00:00".into(),
            updated: "2026-06-14T03:35:00+00:00".into(),
        };

        let concept = WikiEntry {
            id: "wiki-bbb".into(),
            wiki_type: WikiType::Concept,
            title: "概念".into(),
            file_path: "wiki/concepts/wiki-bbb-概念.md".into(),
            source: None,
            authors: vec![],
            tags: vec![],
            relations: vec![],
            content: String::new(),
            created: "2026-06-14T03:31:33+00:00".into(),
            updated: "2026-06-14T03:35:00+00:00".into(),
        };

        let tags = vec![WikiTag {
            id: "tag-xxx".into(),
            title: "个性化学习".into(),
        }];

        let md = build_index_md(&fm, &[&summary], &[&concept], &[], &tags).unwrap();

        assert!(md.contains("type: index"));
        assert!(md.contains("## Summaries"));
        assert!(md.contains("[[summaries/wiki-aaa-综述]]"));
        assert!(md.contains("## Concepts"));
        assert!(md.contains("<@wiki-bbb>[[concepts/wiki-bbb-概念]]"));
        assert!(md.contains("# Tags"));
        assert!(md.contains("tag-xxx-个性化学习"));
    }
}
