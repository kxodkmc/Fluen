//! HTML 表子集解析（规范 §5.3 形态 C）。
//!
//! 限定标签集合：`<table>`/`<thead>`/`<tbody>`/`<tr>`/`<th>`/`<td>`，
//! 属性仅允许 `colspan`/`rowspan`（在 `<th>`/`<td>` 上）。
//! 其余标签或属性 → 解析报错（规范 §6.2）。

use std::collections::HashMap;

use crate::error::{MarkupError, Result};
use crate::md_inline::parse_inline;
use crate::model::{Cell, TableModel};

/// 属性集合（与 parse.rs 共享类型）。
pub(crate) type AttrMap = HashMap<String, String>;

/// 规范 §5.3 / §6.2 允许的标签白名单。
const ALLOWED_TAGS: &[&str] = &["table", "thead", "tbody", "tr", "th", "td"];

/// `<th>`/`<td>` 上允许的属性白名单。
const ALLOWED_CELL_ATTRS: &[&str] = &["colspan", "rowspan"];

/// 是否含 `<table` 子串（用于形态互斥校验）。
pub(crate) fn has_inner_table(body: &str) -> bool {
    body.contains("<table")
}

/// 找到首个 `<table>` 子串，返回其完整 `<table>...</table>` 片段（含标签）。
pub(crate) fn find_html_table(body: &str) -> Option<&str> {
    let start = body.find("<table")?;
    let open_end = body[start..].find('>')? + start + 1;
    let close_rel = body[open_end..].find("</table>")?;
    Some(&body[start..open_end + close_rel + "</table>".len()])
}

/// 第二个 `<table>` 的位置探测（用于互斥校验）。
pub(crate) fn find_html_table_2nd(body: &str) -> Option<&str> {
    let first_end = find_html_table(body)?;
    let rest = &body[body.find("<table")? + first_end.len()..];
    if rest.contains("<table") { Some(rest) } else { None }
}

/// 解析 HTML 表子集为 TableModel（规范 §5.3 形态 C）。
/// 非白名单标签/属性 → 报错（规范 §6.2）。
pub(crate) fn parse_html_table_subset(html: &str, line_no: usize) -> Result<TableModel> {
    let mut model = TableModel { header: Vec::new(), body: Vec::new() };
    let mut current_target_is_header = false;
    let mut current_row: Vec<Cell> = Vec::new();

    let mut tokens = tokenize_html(html, line_no)?;
    while let Some(tok) = tokens.next() {
        match tok {
            HtmlTok::Open(name, attrs) => {
                match name.as_str() {
                    "thead" => { current_target_is_header = true; }
                    "tbody" => { current_target_is_header = false; }
                    "tr" => { current_row.clear(); }
                    "th" | "td" => {
                        // 校验属性白名单（§6.2）
                        for key in attrs.keys() {
                            if !ALLOWED_CELL_ATTRS.contains(&key.as_str()) {
                                return Err(MarkupError::parse(
                                    format!("<f-tbl> HTML 表 `<{name} {key}>` 属性非法：仅允许 colspan/rowspan（规范 §6.2）"),
                                    line_no,
                                ));
                            }
                        }
                        let (text, _) = collect_until_close(&mut tokens, &name);
                        let colspan = attrs.get("colspan").and_then(|v| v.parse().ok()).unwrap_or(1);
                        let rowspan = attrs.get("rowspan").and_then(|v| v.parse().ok()).unwrap_or(1);
                        let cell = Cell { content: parse_inline(text.trim()), colspan, rowspan, line: line_no };
                        current_row.push(cell);
                    }
                    // 其他标签已在 tokenize_html 中校验，不会到达此处
                    _ => {}
                }
            }
            HtmlTok::Close(name) => {
                match name.as_str() {
                    "thead" | "tbody" => { current_target_is_header = false; }
                    "tr" => {
                        if current_target_is_header {
                            if model.header.is_empty() { model.header = std::mem::take(&mut current_row); }
                        } else {
                            model.body.push(std::mem::take(&mut current_row));
                        }
                    }
                    _ => {}
                }
            }
            HtmlTok::Text(_) => {}
        }
    }
    if model.header.is_empty() && model.body.is_empty() {
        return Err(MarkupError::parse("HTML 表为空", line_no));
    }
    Ok(model)
}

// ───────────────────────── HTML tokenizer ─────────────────────────

enum HtmlTok { Open(String, AttrMap), Close(String), Text(String) }

fn tokenize_html(html: &str, line_no: usize) -> Result<std::vec::IntoIter<HtmlTok>> {
    let mut tokens = Vec::new();
    let bytes = html.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if html[i..].starts_with("<!--") {
            if let Some(end) = html[i..].find("-->") { i += end + 3; } else { break; }
            continue;
        }
        if bytes[i] == b'<' {
            // 闭合标签
            if i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                if let Some(gt) = html[i..].find('>') {
                    let name = html[i + 2..i + gt].trim().to_string();
                    tokens.push(HtmlTok::Close(name));
                    i += gt + 1;
                    continue;
                }
            }
            // 开始 / 自闭合
            if let Some(gt) = html[i..].find('>') {
                let inner = html[i + 1..i + gt].trim();
                let self_closing = inner.ends_with('/');
                let _ = self_closing;
                let inner = inner.trim_end_matches('/').trim();
                let name_end = inner
                    .char_indices()
                    .take_while(|(_, c)| c.is_ascii_alphanumeric())
                    .last()
                    .map(|(i, c)| i + c.len_utf8())
                    .unwrap_or(0);
                if name_end == 0 { i += gt + 1; continue; }
                let name = inner[..name_end].to_string();
                // 标签白名单校验（§6.2）
                if !ALLOWED_TAGS.contains(&name.as_str()) {
                    return Err(MarkupError::parse(
                        format!("<f-tbl> HTML 表含非法标签 `<{name}>`：仅允许 table/thead/tbody/tr/th/td（规范 §6.2）"),
                        line_no,
                    ));
                }
                let attrs = parse_attrs_simple(&inner[name_end..]);
                tokens.push(HtmlTok::Open(name, attrs));
                i += gt + 1;
                continue;
            } else {
                break;
            }
        }
        // 文本：到下一个 `<`
        let next = html[i..].find('<').map(|x| i + x).unwrap_or(html.len());
        let text = &html[i..next];
        if !text.trim().is_empty() {
            tokens.push(HtmlTok::Text(text.to_string()));
        }
        i = next;
    }
    Ok(tokens.into_iter())
}

fn parse_attrs_simple(s: &str) -> AttrMap {
    let mut attrs = AttrMap::new();
    let mut rest = s.trim();
    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() { break; }
        let name_end = rest
            .char_indices()
            .take_while(|(_, c)| c.is_ascii_alphanumeric() || *c == '-')
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        if name_end == 0 { break; }
        let name = rest[..name_end].to_string();
        let mut r = rest[name_end..].trim_start();
        if r.starts_with('=') {
            r = r[1..].trim_start();
            if let Some(q) = r.chars().next().filter(|c| *c == '"' || *c == '\'') {
                let after = &r[q.len_utf8()..];
                if let Some(end) = after.find(q) {
                    attrs.insert(name, after[..end].to_string());
                    rest = &after[end + q.len_utf8()..];
                    continue;
                }
            }
            let end = r.find(|c: char| c.is_whitespace()).unwrap_or(r.len());
            attrs.insert(name, r[..end].to_string());
            rest = &r[end..];
        } else {
            attrs.insert(name, String::new());
            rest = &rest[name_end..];
        }
    }
    attrs
}

fn collect_until_close(tokens: &mut std::vec::IntoIter<HtmlTok>, tag: &str) -> (String, bool) {
    let mut text = String::new();
    let mut depth = 1;
    while let Some(t) = tokens.next() {
        match t {
            HtmlTok::Text(s) => text.push_str(&s),
            HtmlTok::Open(n, _) => if n == tag { depth += 1; },
            HtmlTok::Close(n) => {
                if n == tag {
                    depth -= 1;
                    if depth == 0 { return (text, true); }
                }
            }
        }
    }
    (text, false)
}
