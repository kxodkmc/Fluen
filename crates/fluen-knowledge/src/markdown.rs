use crate::types::EditOp;

/// 从 Markdown 正文中提取 `[[wiki/xxx/wikiID-title]]` 或 `[[wikiID]]` 风格的链接。
///
/// wiki.md 规范的链接语法：`[[concepts/wikiID-数智化技术]]` 或 `[[wikiID-数智化技术]]`。
/// 提取出的链接目标可能包含路径前缀，本函数返回原始链接内容（不含 `[[]]`）。
pub fn extract_wiki_links(body: &str) -> Vec<String> {
    let mut links = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'[' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            // 找到 [[，寻找 ]]
            let start = i + 2;
            if let Some(end_rel) = find_double_close(&body[start..]) {
                let link = &body[start..start + end_rel];
                let link = link.trim();
                if !link.is_empty() && !links.contains(&link.to_string()) {
                    links.push(link.to_string());
                }
                i = start + end_rel + 2; // 跳过 ]]
            } else {
                i += 2;
            }
        } else {
            i += 1;
        }
    }

    links
}

/// 寻找 `]]` 的位置（相对偏移）。
fn find_double_close(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b']' && bytes[i + 1] == b']' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// 从链接目标中提取 wikiID。
///
/// - `concepts/wikiID-数智化技术` → `wikiID`
/// - `wikiID-数智化技术` → `wikiID`
/// - `wikiID` → `wikiID`
pub fn extract_wiki_id_from_link(link: &str) -> Option<String> {
    // 去掉路径前缀
    let last_segment = link.rsplit('/').next()?;
    // wikiID 格式：wiki-xxxxxxxxxxxxxxxx
    if !last_segment.starts_with("wiki-") {
        return None;
    }
    // 取 wiki-xxxxxxxxxxxxxxxx 部分（在第一个 `-标题` 之前）
    let rest = &last_segment[5..]; // 去掉 "wiki-"
    let id_end = rest.find('-').unwrap_or(rest.len());
    if id_end == 0 {
        // rest 为空或以 - 开头（如 "wiki-" 或 "wiki--xxx"），不是合法 wikiID
        return None;
    }
    Some(format!("wiki-{}", &rest[..id_end]))
}

/// 执行单个 edit 操作，返回 (新正文, 是否成功)。
pub fn apply_edit(body: &str, op: &EditOp) -> (String, bool) {
    match op {
        EditOp::SearchReplace { search, replace } => {
            if body.contains(search.as_str()) {
                (body.replacen(search, replace, 1), true)
            } else {
                (body.to_string(), false)
            }
        }
        EditOp::InsertAfter { anchor, content } => {
            if let Some(pos) = body.find(anchor.as_str()) {
                let insert_pos = pos + anchor.len();
                let mut new_body = String::with_capacity(body.len() + content.len());
                new_body.push_str(&body[..insert_pos]);
                new_body.push_str(content);
                new_body.push_str(&body[insert_pos..]);
                (new_body, true)
            } else {
                (body.to_string(), false)
            }
        }
    }
}

/// 批量执行 edit 操作，返回 (新正文, 每个操作的结果)。
pub fn apply_edits(body: &str, ops: &[EditOp]) -> (String, Vec<bool>) {
    let mut current = body.to_string();
    let mut results = Vec::with_capacity(ops.len());

    for op in ops {
        let (new_body, success) = apply_edit(&current, op);
        current = new_body;
        results.push(success);
    }

    (current, results)
}

/// 在正文末尾追加或更新 `## 关联页面` 区。
///
/// 若已有 `## 关联页面` 区，在区内追加新链接（去重）；
/// 若无，则在正文末尾新建该区。
pub fn append_relations(body: &str, new_relations: &[String]) -> String {
    if new_relations.is_empty() {
        return body.to_string();
    }

    let header = "## 关联页面";
    let new_links: Vec<String> = new_relations
        .iter()
        .map(|id| format!("- [[{}]]", id))
        .collect();

    if let Some(pos) = body.find(header) {
        // 已有 关联页面 区，找到下一个 ## 或文末
        let after_header = &body[pos + header.len()..];
        let next_section = after_header.find("\n## ").map(|p| p).unwrap_or(after_header.len());
        let section_content = &after_header[..next_section];

        // 提取已有链接，去重追加
        let mut existing: Vec<String> = Vec::new();
        for line in section_content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("- [[") {
                existing.push(trimmed.to_string());
            }
        }

        for new_link in &new_links {
            if !existing.iter().any(|e| e == new_link) {
                existing.push(new_link.clone());
            }
        }

        // 重建该区
        let before = &body[..pos];
        let after = &body[pos + header.len() + next_section..];
        let section = format!("{}\n{}\n", header, existing.join("\n"));
        format!("{}{}{}", before, section, after)
    } else {
        // 无 关联页面 区，在文末新建
        let section = format!("\n\n{}\n{}\n", header, new_links.join("\n"));
        format!("{}{}", body.trim_end(), section)
    }
}

/// 清理正文中指向已删除 wikiID 的链接。
///
/// 将 `[[wikiID-xxx]]` 和 `[[path/wikiID-xxx]]` 形式的链接移除。
pub fn remove_relations(body: &str, removed_ids: &[String]) -> String {
    if removed_ids.is_empty() {
        return body.to_string();
    }

    let mut result = body.to_string();
    for id in removed_ids {
        // 移除整行：- [[...id...]]
        let lines: Vec<&str> = result.lines().collect();
        let filtered: Vec<&str> = lines
            .iter()
            .filter(|line| {
                let trimmed = line.trim();
                if trimmed.starts_with("- [[") && trimmed.contains(id.as_str()) {
                    false
                } else {
                    true
                }
            })
            .copied()
            .collect();
        result = filtered.join("\n");
    }

    result
}

/// 将标题转为文件名安全的字符串。
///
/// 去除文件系统非法字符（/ \ : * ? " < > |），限制长度 50 字符。
pub fn sanitize_title_for_filename(title: &str) -> String {
    let sanitized: String = title
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect();
    let truncated: String = sanitized.chars().take(50).collect();
    truncated.trim().to_string()
}

/// 构造条目文件名：`{wikiID}-{sanitized_title}.md`。
pub fn build_entry_filename(wiki_id: &str, title: &str) -> String {
    let safe_title = sanitize_title_for_filename(title);
    if safe_title.is_empty() {
        format!("{}.md", wiki_id)
    } else {
        format!("{}-{}.md", wiki_id, safe_title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_wiki_links() {
        let body = "参见 [[concepts/wiki-xxx-数智化技术]] 和 [[wiki-yyy]] 以及 [[summaries/wiki-zzz-综述]]";
        let links = extract_wiki_links(body);
        assert_eq!(links.len(), 3);
        assert!(links.contains(&"concepts/wiki-xxx-数智化技术".to_string()));
        assert!(links.contains(&"wiki-yyy".to_string()));
    }

    #[test]
    fn test_extract_wiki_id_from_link() {
        assert_eq!(
            extract_wiki_id_from_link("concepts/wiki-91f2c43bf4de401e-数智化技术"),
            Some("wiki-91f2c43bf4de401e".into())
        );
        assert_eq!(
            extract_wiki_id_from_link("wiki-91f2c43bf4de401e"),
            Some("wiki-91f2c43bf4de401e".into())
        );
    }

    #[test]
    fn test_search_replace() {
        let body = "## 基本信息\n- **研究领域**: 个性化学习";
        let op = EditOp::SearchReplace {
            search: "- **研究领域**: 个性化学习".into(),
            replace: "- **研究领域**: 个性化学习、学习分析".into(),
        };
        let (new_body, success) = apply_edit(body, &op);
        assert!(success);
        assert!(new_body.contains("学习分析"));
    }

    #[test]
    fn test_search_replace_not_found() {
        let body = "正文";
        let op = EditOp::SearchReplace {
            search: "不存在".into(),
            replace: "替换".into(),
        };
        let (_, success) = apply_edit(body, &op);
        assert!(!success);
    }

    #[test]
    fn test_insert_after() {
        let body = "## 代表性论文\n\n## 关联页面";
        let op = EditOp::InsertAfter {
            anchor: "## 代表性论文".into(),
            content: "\n- [[summaries/wiki-xxx-综述]]".into(),
        };
        let (new_body, success) = apply_edit(body, &op);
        assert!(success);
        assert!(new_body.contains("[[summaries/wiki-xxx-综述]]"));
    }

    #[test]
    fn test_append_relations_new_section() {
        let body = "# 牟智佳\n\n## 基本信息\n- 单位: 江南大学";
        let result = append_relations(body, &["wiki-xxx".into(), "wiki-yyy".into()]);
        assert!(result.contains("## 关联页面"));
        assert!(result.contains("[[wiki-xxx]]"));
        assert!(result.contains("[[wiki-yyy]]"));
    }

    #[test]
    fn test_append_relations_existing_section() {
        let body = "# 牟智佳\n\n## 关联页面\n- [[wiki-aaa]]\n";
        let result = append_relations(body, &["wiki-bbb".into()]);
        assert!(result.contains("[[wiki-aaa]]"));
        assert!(result.contains("[[wiki-bbb]]"));
    }

    #[test]
    fn test_sanitize_title() {
        assert_eq!(sanitize_title_for_filename("数智化技术"), "数智化技术");
        assert_eq!(sanitize_title_for_filename("a/b\\c:d*e?f"), "abcdef");
        let long = "x".repeat(100);
        assert_eq!(sanitize_title_for_filename(&long).len(), 50);
    }

    #[test]
    fn test_build_entry_filename() {
        assert_eq!(
            build_entry_filename("wiki-91f2c43bf4de401e", "数智化技术"),
            "wiki-91f2c43bf4de401e-数智化技术.md"
        );
        assert_eq!(
            build_entry_filename("wiki-91f2c43bf4de401e", ""),
            "wiki-91f2c43bf4de401e.md"
        );
    }
}
