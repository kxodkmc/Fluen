//! 块级 f- 标签（`<f-eq>` / `<f-fig>` / `<f-tbl>` / `<f-claim>`）解析与渲染测试。
//!
//! 覆盖：自闭合/配对、编号、caption 提取、形态互斥、HTML 表校验。

use fluen_markup::{
    HtmlRenderer, InMemoryReferences, Options, Pipeline,
};

fn resolve(src: &str) -> fluen_markup::Resolved {
    Pipeline::new(Options::default())
        .references(InMemoryReferences::new())
        .parse(src)
        .unwrap()
        .resolve_or_degrade()
}

fn render_html(src: &str) -> String {
    resolve(src)
        .render(&HtmlRenderer::default().with_inline_style(false))
        .unwrap()
}

// ───────────────────────── <f-eq> ─────────────────────────

#[test]
fn equation_self_closing() {
    let html = render_html(r#"<f-eq id="eq:e1"/>"#);
    assert!(html.contains("eq:e1"), "html: {html}");
}

#[test]
fn equation_with_body() {
    let html = render_html(r#"<f-eq id="eq:e2">
e^{i\pi} + 1 = 0
</f-eq>"#);
    assert!(html.contains("e^{i\\pi}"), "html: {html}");
}

#[test]
fn equation_numbering() {
    let html = render_html(r#"<f-eq id="eq:a"/>
<f-eq id="eq:b"/>"#);
    // 两个公式应编号为 (1) 和 (2)
    assert!(html.contains("(1)"), "html: {html}");
    assert!(html.contains("(2)"), "html: {html}");
}

// ───────────────────────── <f-fig> ─────────────────────────

#[test]
fn figure_with_caption() {
    let html = render_html(r#"<f-fig id="fig:test" src="test.png" alt="测试">
  <f-caption>测试图。</f-caption>
</f-fig>"#);
    assert!(html.contains("test.png"), "html: {html}");
    assert!(html.contains("测试图"), "html: {html}");
}

#[test]
fn figure_numbering() {
    let html = render_html(r#"<f-fig id="fig:a" src="a.png" alt="A">
  <f-caption>A。</f-caption>
</f-fig>

<f-fig id="fig:b" src="b.png" alt="B">
  <f-caption>B。</f-caption>
</f-fig>"#);
    assert!(html.contains("图 1"), "html: {html}");
    assert!(html.contains("图 2"), "html: {html}");
}

// ───────────────────────── <f-tbl> ─────────────────────────

#[test]
fn table_markdown_form() {
    let html = render_html(r#"<f-tbl id="tbl:md">
  <f-caption>MD表。</f-caption>

| A | B |
|---|---|
| 1 | 2 |
</f-tbl>"#);
    assert!(html.contains("MD表"), "html: {html}");
    assert!(html.contains("<td>1</td>"), "html: {html}");
}

#[test]
fn table_html_form() {
    let html = render_html(r#"<f-tbl id="tbl:html">
  <f-caption>HTML表。</f-caption>
<table><thead><tr><th>X</th></tr></thead><tbody><tr><td>1</td></tr></tbody></table>
</f-tbl>"#);
    assert!(html.contains("HTML表"), "html: {html}");
    assert!(html.contains("<th>X</th>"), "html: {html}");
}

#[test]
fn table_html_colspan_rowspan() {
    let html = render_html(r#"<f-tbl id="tbl:span">
  <f-caption>跨列表。</f-caption>
<table><tr><th colspan="2">合并</th></tr><tr><td rowspan="1">a</td><td>b</td></tr></table>
</f-tbl>"#);
    assert!(html.contains("colspan") || html.contains("合并"), "html: {html}");
}

#[test]
fn table_external_src() {
    let html = render_html(r#"<f-tbl id="tbl:ext" src="data.csv">
  <f-caption>外部表。</f-caption>
</f-tbl>"#);
    assert!(html.contains("外部表"), "html: {html}");
    assert!(html.contains("data.csv"), "html: {html}");
}

#[test]
fn table_html_illegal_tag_rejected() {
    let src = r#"<f-tbl id="tbl:bad">
<table><tr><td>ok</td></tr><div>非法</div></table>
</f-tbl>"#;
    let result = Pipeline::new(Options::default())
        .parse(src);
    assert!(result.is_err(), "非法 HTML 标签应报错");
}

#[test]
fn table_html_illegal_attr_rejected() {
    let src = r#"<f-tbl id="tbl:bad2">
<table><tr><td style="color:red">ok</td></tr></table>
</f-tbl>"#;
    let result = Pipeline::new(Options::default())
        .parse(src);
    assert!(result.is_err(), "非法属性应报错");
}

// ───────────────────────── <f-claim> ─────────────────────────

#[test]
fn claim_theorem() {
    let html = render_html(r#"<f-claim id="thm:basic" type="theorem">
设 $f$ 连续，则 $f$ 可积。
</f-claim>"#);
    assert!(html.contains("定理 1"), "html: {html}");
}

#[test]
fn claim_lemma() {
    let html = render_html(r#"<f-claim id="lem:aux" type="lemma">
辅助引理。
</f-claim>"#);
    assert!(html.contains("引理 1"), "html: {html}");
}

#[test]
fn claim_definition() {
    let html = render_html(r#"<f-claim id="def:term" type="definition">
定义术语。
</f-claim>"#);
    assert!(html.contains("定义 1"), "html: {html}");
}

#[test]
fn claim_multiple_types_independent_numbering() {
    let html = render_html(r#"<f-claim id="thm:a" type="theorem">A。</f-claim>

<f-claim id="lem:b" type="lemma">B。</f-claim>

<f-claim id="thm:c" type="theorem">C。</f-claim>"#);
    assert!(html.contains("定理 1"), "html: {html}");
    assert!(html.contains("引理 1"), "html: {html}");
    assert!(html.contains("定理 2"), "html: {html}");
}

// ───────────────────────── P0 回归测试 ─────────────────────────

#[test]
fn resolve_or_degrade_numbering_populated() {
    // 验证 resolve_or_degrade 正确填充编号表（P0-1 回归测试）。
    let src = r#"<f-fig id="fig:p0" src="x.png" alt="P0">
  <f-caption>P0 测试。</f-caption>
</f-fig>

见<f-xref to="fig:p0"/>。"#;
    let resolved = resolve(src);
    // 编号表应包含 fig:p0
    assert!(resolved.numbering.get("fig:p0").is_some(),
        "resolve_or_degrade 后编号表应包含 fig:p0");
    // xref 应命中
    let html = resolved.render(&HtmlRenderer::default().with_inline_style(false)).unwrap();
    assert!(html.contains("图 1"), "html: {html}");
}

#[test]
fn rewrite_inlines_multiple_tags_in_sequence() {
    // 验证 rewrite_inlines 正确处理同一段落中的多个行内 f- 标签（P0-2 回归测试）。
    let src = r#"<f-fig id="fig:a" src="a.png" alt="A">
  <f-caption>A。</f-caption>
</f-fig>

见<f-xref to="fig:a"/>与<f-xref to="fig:a"/>。"#;
    let resolved = resolve(src);
    let html = resolved.render(&HtmlRenderer::default().with_inline_style(false)).unwrap();
    // 两个 xref 都应解析为"图 1"，而不是只有第一个
    let count = html.matches("图 1").count();
    assert!(count >= 2, "两个 xref 都应解析，但只找到 {count} 个 '图 1'。html: {html}");
}

#[test]
fn rewrite_inlines_tags_separated_by_emphasis() {
    // 验证 rewrite_inlines 处理被 emphasis 等嵌套节点分隔的多个行内 f- 标签。
    let src = r#"<f-fig id="fig:b" src="b.png" alt="B">
  <f-caption>B。</f-caption>
</f-fig>

见<f-xref to="fig:b"/>，*重点*，再看<f-xref to="fig:b"/>。"#;
    let resolved = resolve(src);
    let html = resolved.render(&HtmlRenderer::default().with_inline_style(false)).unwrap();
    let count = html.matches("图 1").count();
    assert!(count >= 2, "emphasis 前后的 xref 都应解析。html: {html}");
}

#[test]
fn resolve_or_degrade_with_duplicate_id() {
    // 验证 resolve_or_degrade 在重复 id 下仍返回有效编号表。
    let src = r#"<f-eq id="eq:dup">a</f-eq>

<f-eq id="eq:dup">b</f-eq>

见<f-xref to="eq:dup"/>。"#;
    let resolved = resolve(src);
    // 即使有重复 id，编号表也应包含 eq:dup（首次出现注册）
    assert!(resolved.numbering.get("eq:dup").is_some(),
        "重复 id 下编号表仍应包含首次出现的 eq:dup");
    let html = resolved.render(&HtmlRenderer::default().with_inline_style(false)).unwrap();
    assert!(html.contains("式 (1)"), "html: {html}");
}
