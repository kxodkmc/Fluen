//! 纯文本渲染器（规范 §4.3：文本预览 `[Smith, 2020 ⚠]` / `图（损失曲线）⚠`）。
//!
//! 面向"快速预览 / 命令行 / 可访问性"场景：去掉所有标签，保留语义文字与编号。
//! 表格以 `|` 分隔的纯文本呈现；公式以 LaTeX 原文呈现。

use crate::context::{RenderContext, claim_label};
use crate::kinds::{TableSource, TableVariant};
use crate::model::*;
use crate::render::Renderer;

/// 纯文本渲染器。
#[derive(Debug, Clone, Default)]
pub struct TextRenderer;

impl Renderer for TextRenderer {
    fn format_name(&self) -> &'static str { "text" }

    fn render(&self, doc: &Document, ctx: &RenderContext<'_>) -> crate::error::Result<String> {
        let mut out = String::new();
        if let Some(t) = doc.front_matter.get("title") {
            out.push_str(t);
            out.push_str("\n\n");
        }
        for s in &doc.sections {
            for b in &s.blocks {
                render_block(b, ctx, &mut out);
            }
        }
        Ok(out.trim_end().to_string() + "\n")
    }
}

fn render_block(b: &Block, ctx: &RenderContext<'_>, out: &mut String) {
    match b {
        Block::Heading { level, text, .. } => {
            out.push_str(&"\u{2003}".repeat(*level as usize)); // 全角空格缩进
            render_inlines(text, ctx, out);
            out.push('\n');
        }
        Block::Paragraph(inls, _) => {
            render_inlines(inls, ctx, out);
            out.push_str("\n\n");
        }
        Block::List { ordered, items, .. } => {
            for (i, it) in items.iter().enumerate() {
                let marker = if *ordered { format!("{}. ", i + 1) } else { "• ".to_string() };
                out.push_str(&marker);
                render_inlines(&it.content, ctx, out);
                out.push('\n');
            }
            out.push('\n');
        }
        Block::BlockQuote(inner, _) => {
            for b2 in inner {
                let mut sub = String::new();
                render_block(b2, ctx, &mut sub);
                for line in sub.lines() {
                    out.push_str("| ");
                    out.push_str(line);
                    out.push('\n');
                }
            }
            out.push('\n');
        }
        Block::CodeBlock { code, .. } => {
            for line in code.lines() {
                out.push_str("    ");
                out.push_str(line);
                out.push('\n');
            }
            out.push('\n');
        }
        Block::ThematicBreak(_) => {
            out.push_str("────────────────\n\n");
        }
        Block::Equation(e) => {
            out.push_str("    ");
            out.push_str(&e.latex);
            if let Some(n) = e.resolved_number {
                out.push_str(&format!("    ({n})"));
            }
            out.push_str("\n\n");
        }
        Block::Figure(f) => {
            if let Some(n) = f.resolved_number {
                out.push_str(&format!("{} {n}: ", label_fig(ctx)));
            }
            render_inlines(&f.caption, ctx, out);
            out.push('\n');
            // 图片以占位描述呈现
            if let Some(alt) = &f.alt {
                out.push_str(&format!("    [图片：{}]\n", alt));
            } else {
                out.push_str("    [图片]\n");
            }
            out.push('\n');
        }
        Block::Table(t) => {
            if let Some(n) = t.resolved_number {
                out.push_str(&format!("{} {n}: ", label_tbl(ctx)));
            }
            render_inlines(&t.caption, ctx, out);
            out.push('\n');
            match &t.source {
                TableSource::External { path, .. } => {
                    out.push_str(&format!("    [外部数据：{}]\n", path));
                }
                TableSource::Markdown(m) | TableSource::Html(m) => {
                    render_table_text(m, ctx, out, t.variant);
                }
            }
            out.push('\n');
        }
        Block::Claim(c) => {
            let label = claim_label(c.ty, ctx.options.label_lang);
            let n = c.resolved_number.unwrap_or(0);
            out.push_str(&format!("{label} {n}. "));
            // claim 正文压平为单段
            let mut body = String::new();
            for b in &c.body { render_block(b, ctx, &mut body); }
            out.push_str(body.trim());
            out.push_str("\n\n");
        }
    }
}

fn render_table_text(m: &TableModel, ctx: &RenderContext<'_>, out: &mut String, _variant: TableVariant) {
    let header_strs: Vec<String> = m.header.iter().map(|c| inlines_to_text(&c.content, ctx)).collect();
    if !header_strs.is_empty() {
        out.push_str("    | ");
        out.push_str(&header_strs.join(" | "));
        out.push_str(" |\n");
        // 分隔行（仅文本场景，表达三线表分隔）
        let widths: Vec<usize> = header_strs.iter().map(|s| s.chars().count()).collect();
        out.push_str("    |");
        for w in &widths { out.push_str(&"-".repeat(w + 2)); out.push('|'); }
        out.push('\n');
    }
    for row in &m.body {
        let cells: Vec<String> = row.iter().map(|c| inlines_to_text(&c.content, ctx)).collect();
        out.push_str("    | ");
        out.push_str(&cells.join(" | "));
        out.push_str(" |\n");
    }
}

fn inlines_to_text(inls: &[Inline], ctx: &RenderContext<'_>) -> String {
    let mut s = String::new();
    for i in inls { render_inline(i, ctx, &mut s); }
    s.trim().to_string()
}

fn render_inlines(inls: &[Inline], ctx: &RenderContext<'_>, out: &mut String) {
    for i in inls { render_inline(i, ctx, out); }
}

fn render_inline(i: &Inline, ctx: &RenderContext<'_>, out: &mut String) {
    match i {
        Inline::Text(t) => out.push_str(t),
        Inline::Emphasis(v) => { render_inlines(v, ctx, out); }
        Inline::Strong(v) => { out.push_str("**"); render_inlines(v, ctx, out); out.push_str("**"); }
        Inline::Strikethrough(v) => { render_inlines(v, ctx, out); }
        Inline::Underline(v) => { render_inlines(v, ctx, out); }
        Inline::Code(c) => { out.push('`'); out.push_str(c); out.push('`'); }
        Inline::Math(m) => { out.push('$'); out.push_str(m); out.push('$'); }
        Inline::Link { text, url, .. } => {
            render_inlines(text, ctx, out);
            out.push_str(&format!("（{}）", url));
        }
        Inline::Image { alt, .. } => { out.push_str(alt); }
        Inline::SoftBreak => out.push('\n'),
        Inline::Cite(c) => render_cite(c, ctx, out),
        Inline::Xref(x) => render_xref(x, ctx, out),
    }
}

fn render_cite(c: &FluenCite, ctx: &RenderContext<'_>, out: &mut String) {
    let parts = if c.resolved.all_hit {
        c.resolved.entries.iter()
            .filter_map(|e| e.as_ref().map(|e| e.display.clone()))
            .collect()
    } else {
        let mut parts: Vec<String> = Vec::new();
        for (i, e) in c.resolved.entries.iter().enumerate() {
            match e {
                Some(en) => parts.push(en.display.clone()),
                None => {
                    let mark = if ctx.options.degrade_marker { " ⚠" } else { "" };
                    if let Some(fb) = &c.fallback {
                        parts.push(format!("{fb}{mark}"));
                    } else {
                        let r = c.refs.get(i).cloned().unwrap_or_default();
                        parts.push(format!("?{r}?{mark}"));
                    }
                }
            }
        }
        parts
    };
    out.push_str(&join_cite(&parts, ctx, c.loc.as_deref()));
}

fn join_cite(parts: &[String], ctx: &RenderContext<'_>, loc: Option<&str>) -> String {
    if parts.is_empty() { return String::new(); }
    match ctx.options.cite_style {
        crate::context::CiteStyle::Numeric => {
            let nums: Vec<String> = parts.iter()
                .map(|p| p.trim_start_matches('[').trim_end_matches(']').to_string())
                .collect();
            let mut inner = nums.join(",");
            if let Some(l) = loc { inner.push_str(&format!(", {l}")); }
            format!("[{inner}]")
        }
        crate::context::CiteStyle::AuthorYear => {
            let mut inner = parts.join("; ");
            if let Some(l) = loc { inner.push_str(&format!(", {l}")); }
            format!("({inner})")
        }
    }
}

fn render_xref(x: &FluenXref, ctx: &RenderContext<'_>, out: &mut String) {
    if let Some(ov) = &x.override_text {
        render_inlines(ov, ctx, out);
        return;
    }
    if x.hit {
        if let Some(t) = &x.resolved_text { out.push_str(t); }
    } else if let Some(fb) = &x.fallback {
        let mark = if ctx.options.degrade_marker { "⚠" } else { "" };
        out.push_str(fb);
        out.push_str(mark);
    } else {
        out.push_str(&x.to);
    }
}

fn label_fig(ctx: &RenderContext<'_>) -> &'static str {
    match ctx.options.label_lang { crate::context::LabelLang::Zh => "图", crate::context::LabelLang::En => "Figure" }
}
fn label_tbl(ctx: &RenderContext<'_>) -> &'static str {
    match ctx.options.label_lang { crate::context::LabelLang::Zh => "表", crate::context::LabelLang::En => "Table" }
}
