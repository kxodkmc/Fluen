use crate::error::{KnowledgeError, Result};

/// 生成 wikiID：`wiki-` + 16 位 UUID4（无连字符）。
pub fn generate_wiki_id() -> String {
    format!(
        "wiki-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..16]
    )
}

/// 生成 tagID：`tag-` + 16 位 UUID4（无连字符）。
pub fn generate_tag_id() -> String {
    format!(
        "tag-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..16]
    )
}

/// 生成 refID：`ref-` + 16 位 UUID4（无连字符）。
pub fn generate_ref_id() -> String {
    format!(
        "ref-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..16]
    )
}

/// 判断是否为合法 wikiID（`wiki-` + 至少 1 位字符）。
pub fn is_wiki_id(s: &str) -> bool {
    s.starts_with("wiki-") && s.len() > 5
}

/// 判断是否为合法 tagID（`tag-` + 至少 1 位字符）。
pub fn is_tag_id(s: &str) -> bool {
    s.starts_with("tag-") && s.len() > 4
}

/// 判断是否为合法 refID（`ref-` + 至少 1 位字符）。
pub fn is_ref_id(s: &str) -> bool {
    s.starts_with("ref-") && s.len() > 4
}

/// 判断输入是否为任意类型的 ID（wiki- / tag- / ref-）。
pub fn is_any_id(s: &str) -> bool {
    is_wiki_id(s) || is_tag_id(s) || is_ref_id(s)
}

/// 校验 wikiID 格式，非法则返回错误。
pub fn validate_wiki_id(s: &str) -> Result<()> {
    if !is_wiki_id(s) {
        return Err(KnowledgeError::Invalid(format!(
            "invalid wikiID: {s} (expected format: wiki-xxxxxxxxxxxxxxxx)"
        )));
    }
    Ok(())
}

/// 从文件名提取 wikiID。
///
/// 文件名格式：`wiki-xxxxxxxxxxxxxxxx-标题.md` 或 `wiki-xxxxxxxxxxxxxxxx.md`
/// 返回 wikiID 部分。
pub fn extract_id_from_filename(filename: &str) -> Option<String> {
    let stem = filename.strip_suffix(".md")?;
    if !stem.starts_with("wiki-") {
        return None;
    }
    // wiki-xxxxxxxxxxxxxxxx-标题 → 取 wiki-xxxxxxxxxxxxxxxx
    // wiki-xxxxxxxxxxxxxxxx → 整个
    let rest = &stem[5..]; // 去掉 "wiki-"
    let id_end = rest.find('-').unwrap_or(rest.len());
    if id_end == 0 {
        return None;
    }
    Some(format!("wiki-{}", &rest[..id_end]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_wiki_id_format() {
        let id = generate_wiki_id();
        assert!(id.starts_with("wiki-"));
        assert_eq!(id.len(), 21); // "wiki-" (5) + 16 chars
    }

    #[test]
    fn test_generate_tag_id_format() {
        let id = generate_tag_id();
        assert!(id.starts_with("tag-"));
        assert_eq!(id.len(), 20); // "tag-" (4) + 16 chars
    }

    #[test]
    fn test_is_wiki_id() {
        assert!(is_wiki_id("wiki-91f2c43bf4de401e"));
        assert!(!is_wiki_id("wiki-"));
        assert!(!is_wiki_id("kw-91f2c43bf4de401e"));
        assert!(!is_wiki_id("random"));
    }

    #[test]
    fn test_extract_id_from_filename() {
        assert_eq!(
            extract_id_from_filename("wiki-91f2c43bf4de401e-数智化技术.md"),
            Some("wiki-91f2c43bf4de401e".into())
        );
        assert_eq!(
            extract_id_from_filename("wiki-91f2c43bf4de401e.md"),
            Some("wiki-91f2c43bf4de401e".into())
        );
        assert_eq!(extract_id_from_filename("random.md"), None);
    }
}
