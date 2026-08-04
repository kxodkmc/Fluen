//! HTML 渲染器（规范 §10：HTML 预览映射）。
//!
//! 映射要点：
//!   - `<f-fig>` → `<figure>`；`<f-tbl>` → `<table>`（含三线表样式）；`<f-eq>` → `<div class="f-eq">`
//!   - `<f-xref>` → `<a href="#id">`；`<f-cite>` → `<sup>`；降级 → 警告样式 + `title`
//!   - 行内数学 `$...$` → `<span class="math">…</span>`（前端可接 KaTeX/MathJax）
//!   - 编号公式 LaTeX → 透传到 `$$ ... $$` span
//!
//! 风格：三线表由 CSS 默认渲染（`variant="threeline"`，规范 §5.3）。

use crate::context::{RenderContext, claim_label};
use crate::kinds::{TableSource, TableVariant};
use crate::model::*;
use crate::render::Renderer;
use crate::render::escape::{html_escape_attr, html_escape_text, sanitize_url};

/// HTML 渲染器。可配置是否内联默认 CSS（便于一次性输出独立 HTML 文件）。
#[derive(Debug, Clone)]
pub struct HtmlRenderer {
    /// 是否在 `<head>` 注入默认样式（三线表、降级警告等）。
    pub inline_style: bool,
    /// 文档 `<title>`（取自 front matter 或首个 H1）。
    pub title: Option<String>,
}

impl Default for HtmlRenderer {
    fn default() -> Self { Self { inline_style: true, title: None } }
}

impl HtmlRenderer {
    pub fn new() -> Self { Self::default() }
    pub fn with_inline_style(mut self, v: bool) -> Self { self.inline_style = v; self }
    pub fn title(mut self, t: impl Into<String>) -> Self { self.title = Some(t.into()); self }
}

impl Renderer for HtmlRenderer {
    fn format_name(&self) -> &'static str { "html" }

    fn render(&self, doc: &Document, ctx: &RenderContext<'_>) -> crate::error::Result<String> {
        let mut out = String::new();
        // 文档头
        out.push_str("<!DOCTYPE html>\n<html lang=\"");
        out.push_str(match ctx.options.label_lang {
            crate::context::LabelLang::Zh => "zh",
            crate::context::LabelLang::En => "en",
        });
        out.push_str("\">\n<head>\n<meta charset=\"utf-8\">\n");
        let title = self.title.clone().or_else(|| doc.front_matter.get("title").map(str::to_string)).unwrap_or_default();
        out.push_str("<title>");
        out.push_str(&html_escape_text(&title));
        out.push_str("</title>\n");
        if self.inline_style {
            out.push_str("<style>\n");
            out.push_str(DEFAULT_CSS);
            out.push_str("\n</style>\n");
        }
        out.push_str("</head>\n<body>\n<article class=\"fluen-doc\">\n");

        // 章节
        for s in &doc.sections {
            // 章节容器：若有 id 加锚点
            if let Some(id) = &s.id {
                out.push_str(&format!("<section id=\"{}\">\n", html_escape_attr(id)));
            } else {
                out.push_str("<section>\n");
            }
            for b in &s.blocks {
                render_block(b, ctx, &mut out);
            }
            out.push_str("</section>\n");
        }
        out.push_str("</article>\n</body>\n</html>\n");
        Ok(out)
    }
}

fn render_block(b: &Block, ctx: &RenderContext<'_>, out: &mut String) {
    match b {
        Block::Heading { level, text, id, line } => {
            let tag = match level {
                1 => "h1", 2 => "h2", 3 => "h3", 4 => "h4", 5 => "h5", _ => "h6",
            };
            if let Some(idv) = id {
                out.push_str(&format!("<{tag} id=\"{}\" data-source-line=\"{line}\">", html_escape_attr(idv)));
            } else {
                out.push_str(&format!("<{tag} data-source-line=\"{line}\">"));
            }
            render_inlines(text, ctx, out);
            out.push_str(&format!("</{tag}>\n"));
        }
        Block::Paragraph(inls, line) => {
            out.push_str(&format!("<p data-source-line=\"{line}\">"));
            render_inlines(inls, ctx, out);
            out.push_str("</p>\n");
        }
        Block::List { ordered, items, line } => {
            let tag = if *ordered { "ol" } else { "ul" };
            out.push_str(&format!("<{tag} data-source-line=\"{line}\">\n"));
            for (idx, it) in items.iter().enumerate() {
                // 列表项没有独立行号，使用列表起始行 + 项索引作为可编辑定位锚点
                let item_line = line + idx;
                out.push_str(&format!("<li data-source-line=\"{item_line}\">"));
                render_inlines(&it.content, ctx, out);
                out.push_str("</li>\n");
            }
            out.push_str(&format!("</{tag}>\n"));
        }
        Block::BlockQuote(inner, line) => {
            out.push_str(&format!("<blockquote data-source-line=\"{line}\">\n"));
            for b2 in inner { render_block(b2, ctx, out); }
            out.push_str("</blockquote>\n");
        }
        Block::CodeBlock { lang, code, line } => {
            let cls = lang.as_deref().map(|l| format!(" class=\"language-{}\"", html_escape_attr(l))).unwrap_or_default();
            out.push_str(&format!("<pre data-source-line=\"{line}\"><code{cls}>"));
            out.push_str(&html_escape_text(code));
            out.push_str("</code></pre>\n");
        }
        Block::ThematicBreak(line) => out.push_str(&format!("<hr data-source-line=\"{line}\"/>\n")),
        // ── f- 标签 ──
        Block::Equation(e) => render_eq(e, out),
        Block::Figure(f) => render_fig(f, ctx, out),
        Block::Table(t) => render_tbl(t, ctx, out),
        Block::Claim(c) => render_claim(c, ctx, out),
    }
}

fn render_eq(e: &FluenEq, out: &mut String) {
    out.push_str("<div class=\"f-eq\"");
    if let Some(id) = &e.id {
        out.push_str(&format!(" id=\"{}\"", html_escape_attr(id)));
    }
    out.push_str(">");
    out.push_str(&html_escape_text(&e.latex));
    if let Some(n) = e.resolved_number {
        out.push_str(&format!("<span class=\"f-eq-num\">({n})</span>"));
    }
    out.push_str("</div>\n");
}

fn render_fig(f: &FluenFig, ctx: &RenderContext<'_>, out: &mut String) {
    out.push_str("<figure");
    if let Some(id) = &f.id {
        out.push_str(&format!(" id=\"{}\"", html_escape_attr(id)));
    }
    out.push_str(">\n");
    let alt = f.alt.as_deref().unwrap_or("");
    out.push_str(&format!("<img src=\"{}\" alt=\"{}\"/>", html_escape_attr(&sanitize_url(&f.src)), html_escape_attr(alt)));
    // caption：图 N + 内容
    if let Some(n) = f.resolved_number {
        out.push_str(&format!("<figcaption><span class=\"f-label\">{} {n}</span> ", label_fig(ctx)));
    } else {
        out.push_str("<figcaption>");
    }
    render_inlines(&f.caption, ctx, out);
    out.push_str("</figcaption>\n</figure>\n");
}

fn render_tbl(t: &FluenTbl, ctx: &RenderContext<'_>, out: &mut String) {
    out.push_str("<figure class=\"f-tbl");
    if t.variant == TableVariant::Grid { out.push_str(" f-tbl-grid"); }
    out.push('"');
    if let Some(id) = &t.id {
        out.push_str(&format!(" id=\"{}\"", html_escape_attr(id)));
    }
    out.push_str(">\n");
    if let Some(n) = t.resolved_number {
        out.push_str(&format!("<figcaption><span class=\"f-label\">{} {n}</span> ", label_tbl(ctx)));
    } else {
        out.push_str("<figcaption>");
    }
    render_inlines(&t.caption, ctx, out);
    out.push_str("</figcaption>\n");
    // 表体
    match &t.source {
        TableSource::External { path, .. } => {
            // 外部数据：渲染占位（实际数据由宿主在导出时填充）
            out.push_str(&format!("<table data-src=\"{}\"><caption class=\"f-tbl-src\">{}</caption></table>\n",
                html_escape_attr(path),
                html_escape_text(path)));
        }
        TableSource::Markdown(m) | TableSource::Html(m) => {
            render_table_model(m, ctx, out);
        }
    }
    out.push_str("</figure>\n");
}

fn render_table_model(m: &TableModel, ctx: &RenderContext<'_>, out: &mut String) {
    out.push_str("<table>\n");
    if !m.header.is_empty() {
        out.push_str("<thead><tr>");
        for c in &m.header {
            out.push_str("<th");
            if c.colspan > 1 { out.push_str(&format!(" colspan=\"{}\"", c.colspan)); }
            if c.rowspan > 1 { out.push_str(&format!(" rowspan=\"{}\"", c.rowspan)); }
            out.push('>');
            render_inlines(&c.content, ctx, out);
            out.push_str("</th>");
        }
        out.push_str("</tr></thead>\n");
    }
    if !m.body.is_empty() {
        out.push_str("<tbody>\n");
        for row in &m.body {
            out.push_str("<tr>");
            for c in row {
                out.push_str("<td");
                if c.colspan > 1 { out.push_str(&format!(" colspan=\"{}\"", c.colspan)); }
                if c.rowspan > 1 { out.push_str(&format!(" rowspan=\"{}\"", c.rowspan)); }
                out.push('>');
                render_inlines(&c.content, ctx, out);
                out.push_str("</td>");
            }
            out.push_str("</tr>\n");
        }
        out.push_str("</tbody>\n");
    }
    out.push_str("</table>\n");
}

fn render_claim(c: &FluenClaim, ctx: &RenderContext<'_>, out: &mut String) {
    out.push_str("<div class=\"f-claim\"");
    if let Some(id) = &c.id {
        out.push_str(&format!(" id=\"{}\"", html_escape_attr(id)));
    }
    out.push_str(">\n");
    let label = claim_label(c.ty, ctx.options.label_lang);
    let n = c.resolved_number.unwrap_or(0);
    out.push_str(&format!("<div class=\"f-claim-head\"><span class=\"f-label\">{} {n}</span></div>\n", html_escape_text(label)));
    out.push_str("<div class=\"f-claim-body\">\n");
    for b in &c.body { render_block(b, ctx, out); }
    out.push_str("</div>\n</div>\n");
}

// ── 行内 ──
fn render_inlines(inls: &[Inline], ctx: &RenderContext<'_>, out: &mut String) {
    for i in inls { render_inline(i, ctx, out); }
}

fn render_inline(i: &Inline, ctx: &RenderContext<'_>, out: &mut String) {
    match i {
        Inline::Text(t) => out.push_str(&html_escape_text(t)),
        Inline::Emphasis(v) => { out.push_str("<em>"); render_inlines(v, ctx, out); out.push_str("</em>"); }
        Inline::Strong(v) => { out.push_str("<strong>"); render_inlines(v, ctx, out); out.push_str("</strong>"); }
        Inline::Strikethrough(v) => { out.push_str("<del>"); render_inlines(v, ctx, out); out.push_str("</del>"); }
        Inline::Code(c) => { out.push_str("<code>"); out.push_str(&html_escape_text(c)); out.push_str("</code>"); }
        Inline::Math(m) => {
            // 行内数学：交给前端 KaTeX/MathJax；用 $$ 包裹供后者识别
            out.push_str("<span class=\"math inline\">");
            out.push_str(&html_escape_text(m));
            out.push_str("</span>");
        }
        Inline::Link { text, url, title } => {
            let t = title.as_deref().map(|t| format!(" title=\"{}\"", html_escape_attr(t))).unwrap_or_default();
            out.push_str(&format!("<a href=\"{}\"{t}>", html_escape_attr(&sanitize_url(url))));
            render_inlines(text, ctx, out);
            out.push_str("</a>");
        }
        Inline::Image { alt, src, title } => {
            let t = title.as_deref().map(|t| format!(" title=\"{}\"", html_escape_attr(t))).unwrap_or_default();
            out.push_str(&format!("<img src=\"{}\" alt=\"{}\"{t}/>", html_escape_attr(&sanitize_url(src)), html_escape_attr(alt)));
        }
        Inline::SoftBreak => out.push('\n'),
        Inline::Cite(c) => render_cite(c, ctx, out),
        Inline::Xref(x) => render_xref(x, ctx, out),
    }
}

fn render_cite(c: &FluenCite, ctx: &RenderContext<'_>, out: &mut String) {
    out.push_str("<sup class=\"f-cite\">");
    let parts = if c.resolved.all_hit {
        // 合并显示：[1] 或 [1,2,3]（数字制）；作者-年用 ; 连接
        c.resolved.entries.iter()
            .filter_map(|e| e.as_ref().map(|e| e.display.clone()))
            .collect()
    } else {
        // 降级：逐条显示，未命中用 fallback
        let mut parts: Vec<String> = Vec::new();
        for (i, e) in c.resolved.entries.iter().enumerate() {
            match e {
                Some(en) => parts.push(en.display.clone()),
                None => {
                    if let Some(fb) = &c.fallback {
                        let mark = if ctx.options.degrade_marker { " ⚠" } else { "" };
                        parts.push(format!("{fb}{mark}"));
                    } else {
                        let r = c.refs.get(i).cloned().unwrap_or_default();
                        let mark = if ctx.options.degrade_marker { " ⚠" } else { "" };
                        parts.push(format!("?{r}?{mark}"));
                    }
                }
            }
        }
        parts
    };
    let text = join_cite_display(&parts, ctx, c.loc.as_deref());
    if c.resolved.all_hit {
        out.push_str(&html_escape_text(&text));
    } else {
        let title = format!("部分文献未命中：{}", c.refs.join(", "));
        out.push_str(&format!("<span class=\"f-degraded\" title=\"{}\">", html_escape_attr(&title)));
        out.push_str(&html_escape_text(&text));
        out.push_str("</span>");
    }
    out.push_str("</sup>");
}

fn join_cite_display(parts: &[String], ctx: &RenderContext<'_>, loc: Option<&str>) -> String {
    if parts.is_empty() { return String::new(); }
    match ctx.options.cite_style {
        crate::context::CiteStyle::Numeric => {
            // 数字制合并为 [1,2,3]：剥离每项的 [ ] 后重组
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
            inner
        }
    }
}

fn render_xref(x: &FluenXref, ctx: &RenderContext<'_>, out: &mut String) {
    // 手写覆盖
    if let Some(ov) = &x.override_text {
        out.push_str("<a class=\"f-xref f-xref-override\"");
        if !x.to.is_empty() {
            out.push_str(&format!(" href=\"#{}\"", html_escape_attr(&x.to)));
        }
        out.push_str(">");
        render_inlines(ov, ctx, out);
        out.push_str("</a>");
        return;
    }
    if x.hit {
        if let Some(text) = &x.resolved_text {
            out.push_str(&format!("<a class=\"f-xref\" href=\"#{}\">", html_escape_attr(&x.to)));
            out.push_str(&html_escape_text(text));
            out.push_str("</a>");
        }
    } else if let Some(fb) = &x.fallback {
        let mark = if ctx.options.degrade_marker { " ⚠" } else { "" };
        let title = format!("交叉引用目标 `{}` 不存在", x.to);
        out.push_str(&format!("<a class=\"f-xref f-degraded\" title=\"{}\">", html_escape_attr(&title)));
        out.push_str(&html_escape_text(fb));
        out.push_str(mark);
        out.push_str("</a>");
    } else {
        // 无 fallback：渲染器到此应已被 Linter 拦截；兜底显示原始 to
        out.push_str(&format!("<a class=\"f-xref f-broken\" href=\"#{}\">", html_escape_attr(&x.to)));
        out.push_str(&html_escape_text(&x.to));
        out.push_str("</a>");
    }
}

fn label_fig(ctx: &RenderContext<'_>) -> &'static str {
    match ctx.options.label_lang { crate::context::LabelLang::Zh => "图", crate::context::LabelLang::En => "Figure" }
}
fn label_tbl(ctx: &RenderContext<'_>) -> &'static str {
    match ctx.options.label_lang { crate::context::LabelLang::Zh => "表", crate::context::LabelLang::En => "Table" }
}

/// 默认 CSS（三线表 + 降级警告 + 行内数学）。
const DEFAULT_CSS: &str = r#"
:root { color-scheme: light; }
body { font-family: -apple-system, "Segoe UI", "Noto Sans CJK SC", sans-serif; line-height: 1.7; max-width: 820px; margin: 2rem auto; padding: 0 1rem; color: #1a1a1a; }
.fluen-doc section { margin: 1.5rem 0; }
.f-eq { text-align: center; margin: 1.2rem 0; font-style: italic; }
.f-eq-num { display: inline-block; margin-left: 1em; }
figure { margin: 1.5rem 0; text-align: center; }
figcaption { font-size: 0.92em; color: #444; margin-top: 0.4em; }
.f-label { font-weight: 600; }
/* 三线表（默认） */
.f-tbl table { border-collapse: collapse; margin: 0.8rem auto; width: 100%; }
.f-tbl table thead tr { border-top: 2px solid #222; border-bottom: 1px solid #888; }
.f-tbl table tbody tr:last-child { border-bottom: 2px solid #222; }
.f-tbl table th, .f-tbl table td { padding: 0.4em 0.7em; text-align: center; }
.f-tbl.f-tbl-grid table th, .f-tbl.f-tbl-grid table td { border: 1px solid #aaa; }
.f-tbl-src { font-size: 0.8em; color: #888; }
.f-claim { margin: 1.2rem 0; padding: 0.6rem 1rem; background: #f7f7f9; border-left: 3px solid #555; }
.f-claim-head { font-weight: 600; margin-bottom: 0.4rem; }
.f-cite { font-size: 0.8em; vertical-align: super; }
.f-degraded { color: #b00020; text-decoration: underline dashed; }
.f-xref { color: #1565c0; text-decoration: none; }
.f-xref:hover { text-decoration: underline; }
.f-broken { color: #b00020; }
.math.inline { font-style: italic; }
"#;


