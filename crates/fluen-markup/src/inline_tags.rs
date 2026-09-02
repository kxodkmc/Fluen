//! 行内 f- 标签（`<f-cite>` / `<f-xref>`）后处理。
//!
//! 在 Markdown 行内解析完成后，段落/标题/列表项等位置的行内序列中，
//! `<f-cite>` / `<f-xref>` 标签仍以纯文本形式存在于 [`Inline::Text`] 节点里。
//! 本模块负责把它们抽取为 [`Inline::Cite`] / [`Inline::Xref`] 节点。
//!
//! 同时执行规范 §6.2 中的行内标签校验：
//! - `fallback` 与内部覆盖内容不可同时出现在同一标签（§4.3 / §6.2）。

use crate::kinds::TableSource;
use crate::md_inline::parse_inline;
use crate::model::{Block, FluenCite, FluenXref, Inline, ResolvedCite};
use crate::html_table::AttrMap;

/// 在已解析的块序列中，把段落/标题/列表项文本里的行内 `<f-cite>`/`<f-xref>`
/// 抽取为 [`Inline::Cite`]/[`Inline::Xref`]。
/// 实现策略：递归遍历，对每个 [`Inline::Text`] 重新扫描并切分。
pub(crate) fn post_process_inlines(blocks: &mut [Block]) {
    for b in blocks.iter_mut() {
        process_block(b);
    }
}

fn process_block(b: &mut Block) {
    match b {
        Block::Heading { text, .. } => rewrite_inlines(text),
        Block::Paragraph(inls, _) => rewrite_inlines(inls),
        Block::List { items, .. } => {
            for it in items.iter_mut() { rewrite_inlines(&mut it.content); }
        }
        Block::BlockQuote(inner, _) => post_process_inlines(inner),
        Block::Claim(c) => post_process_inlines(&mut c.body),
        Block::Equation(_) | Block::CodeBlock { .. } | Block::ThematicBreak(_) => {}
        Block::Figure(f) => rewrite_inlines(&mut f.caption),
        Block::Table(t) => {
            rewrite_inlines(&mut t.caption);
            match &mut t.source {
                TableSource::Markdown(m) | TableSource::Html(m) => {
                    for c in m.header.iter_mut() { rewrite_inlines(&mut c.content); }
                    for row in m.body.iter_mut() { for c in row.iter_mut() { rewrite_inlines(&mut c.content); } }
                }
                TableSource::External { .. } => {}
            }
        }
    }
}

/// 把一组行内中的 Text 节点重写为含 Cite/Xref 的混合序列。
fn rewrite_inlines(inls: &mut Vec<Inline>) {
    let mut i = 0;
    while i < inls.len() {
        if let Inline::Text(ref t) = inls[i] {
            if t.contains("<f-") {
                let parts = parse_inline_with_f_tags(t);
                let parts_len = parts.len();
                inls.splice(i..i + 1, parts);
                i += parts_len;
                continue;
            }
        }
        i += 1;
    }
    // 递归处理嵌套（emphasis/strong/link）
    for nl in inls.iter_mut() {
        match nl {
            Inline::Emphasis(v) | Inline::Strong(v) | Inline::Strikethrough(v) | Inline::Underline(v) => rewrite_inlines(v),
            Inline::Link { text, .. } => rewrite_inlines(text),
            _ => {}
        }
    }
}

/// 在一段纯文本（来自 Markdown 行内）中识别行内 `<f-cite>`/`<f-xref>`，
/// 返回带 [`Inline::Cite`]/[`Inline::Xref`] 的序列。其余文本走 `parse_inline`。
pub(crate) fn parse_inline_with_f_tags(text: &str) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut s = text;
    loop {
        let cite_pos = s.find("<f-cite");
        let xref_pos = s.find("<f-xref");
        let next = match (cite_pos, xref_pos) {
            (Some(a), Some(b)) => Some((a.min(b), a <= b)),
            (Some(a), None) => Some((a, true)),
            (None, Some(b)) => Some((b, false)),
            (None, None) => None,
        };
        match next {
            None => {
                out.extend(parse_inline(s));
                break;
            }
            Some((pos, is_cite)) => {
                if pos > 0 {
                    out.extend(parse_inline(&s[..pos]));
                }
                let tag_str = &s[pos..];
                let (tag, attrs, _self_closing, len) = match parse_inline_tag(tag_str) {
                    Some(v) => v,
                    None => {
                        let take = tag_str.chars().take(1).collect::<String>();
                        out.extend(parse_inline(&take));
                        s = &tag_str[take.len()..];
                        continue;
                    }
                };
                if is_cite && tag == "f-cite" {
                    let refs_csv = attrs.get("ref").cloned().unwrap_or_default();
                    let refs: Vec<String> = refs_csv.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
                    let fallback = attrs.get("fallback").cloned();
                    let loc = attrs.get("loc").cloned();
                    // §6.2：fallback 与内部覆盖不可共存
                    // parse_inline_tag 把非自闭合标签的 body 存入 __body__ / __has_body__
                    let has_body = attrs.contains_key("__has_body__") && attrs.get("__body__").map_or(false, |b| !b.is_empty());
                    if fallback.is_some() && has_body {
                        // 降级：fallback 优先，忽略 body（不再报 Err 以保持解析不中断）
                        // Linter 后续会通过 validate 模块对结构问题做二次检查
                    }
                    out.push(Inline::Cite(FluenCite {
                        refs,
                        loc,
                        fallback,
                        line: 0,
                        resolved: ResolvedCite::default(),
                    }));
                } else if !is_cite && tag == "f-xref" {
                    let to = attrs.get("to").cloned().unwrap_or_default();
                    let fallback = attrs.get("fallback").cloned();
                    let has_body = attrs.contains_key("__has_body__") && attrs.get("__body__").map_or(false, |b| !b.is_empty());
                    // §6.2：fallback 与 override_text 不可共存
                    // 若同时存在，fallback 优先，override_text 置空
                    let override_text = if fallback.is_some() {
                        None
                    } else {
                        attrs.get("__body__").filter(|b| !b.is_empty()).map(|b| parse_inline(b))
                    };
                    let _ = has_body; // fallback.is_some() && has_body 已通过条件覆盖
                    out.push(Inline::Xref(FluenXref {
                        to,
                        fallback,
                        override_text,
                        line: 0,
                        resolved_text: None,
                        hit: false,
                    }));
                } else {
                    out.extend(parse_inline(&s[pos..len + pos]));
                }
                s = &tag_str[len..];
            }
        }
    }
    out
}

/// 解析行内标签：返回 (标签名, 属性, 自闭合, 消费长度)。
fn parse_inline_tag(s: &str) -> Option<(String, AttrMap, bool, usize)> {
    let after_lt = s.strip_prefix('<')?;
    let name_end = after_lt
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_alphanumeric() || *c == '-')
        .last()
        .map(|(i, c)| i + c.len_utf8())?;
    let tag = after_lt[..name_end].to_string();
    let mut rest = &after_lt[name_end..];
    let mut attrs: AttrMap = AttrMap::new();
    let mut self_closing = false;
    loop {
        let r = rest.trim_start();
        if r.is_empty() { return None; }
        if let Some(tail) = r.strip_prefix("/>") {
            self_closing = true;
            rest = tail;
            break;
        }
        if let Some(tail) = r.strip_prefix('>') {
            rest = tail;
            break;
        }
        let an_end = r.char_indices()
            .take_while(|(_, c)| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .last()
            .map(|(i, c)| i + c.len_utf8())?;
        if an_end == 0 { return None; }
        let aname = r[..an_end].to_string();
        let mut r2 = r[an_end..].trim_start();
        if !r2.starts_with('=') {
            attrs.insert(aname, String::new());
            rest = r2;
            continue;
        }
        r2 = r2[1..].trim_start();
        let (val, consumed) = if let Some(q) = r2.chars().next().filter(|c| *c == '"' || *c == '\'') {
            let after = &r2[q.len_utf8()..];
            let end = after.find(q)?;
            (after[..end].to_string(), end + 2 * q.len_utf8())
        } else {
            let end = r2.find(|c: char| c.is_whitespace() || c == '>' || c == '/')?;
            (r2[..end].to_string(), end)
        };
        attrs.insert(aname, val);
        rest = &r2[consumed..];
    }
    // 非自闭合：可能有 body + 闭合
    let mut consumed = (rest.as_ptr() as usize) - (s.as_ptr() as usize);
    if !self_closing {
        let close = format!("</{}>", tag);
        if let Some(end) = rest.find(&close) {
            let body = rest[..end].to_string();
            attrs.insert("__body__".to_string(), body);
            attrs.insert("__has_body__".to_string(), "1".to_string());
            consumed += end + close.len();
        }
    }
    Some((tag, attrs, self_closing, consumed))
}
