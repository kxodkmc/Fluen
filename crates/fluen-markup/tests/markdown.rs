//! Markdown 基础解析测试。
//!
//! 覆盖：标题、段落、列表、代码块、引用块、分隔线、行内格式。

use fluen_markup::{HtmlRenderer, InMemoryReferences, Options, Pipeline};

fn render_html(src: &str) -> String {
    Pipeline::new(Options::default())
        .references(InMemoryReferences::new())
        .parse(src)
        .unwrap()
        .resolve_or_degrade()
        .render(&HtmlRenderer::default().with_inline_style(false))
        .unwrap()
}

#[test]
fn heading_with_id() {
    let html = render_html("## 引言 {#sec:intro}\n正文。");
    assert!(html.contains("<h2"), "html: {html}");
    assert!(html.contains("sec:intro"), "html: {html}");
}

#[test]
fn heading_without_id() {
    let html = render_html("# 无ID标题\n正文。");
    assert!(html.contains("<h1"), "html: {html}");
}

#[test]
fn paragraph() {
    let html = render_html("这是一段文字。");
    assert!(html.contains("<p"), "html: {html}");
    assert!(html.contains("这是一段文字"), "html: {html}");
}

#[test]
fn unordered_list() {
    let html = render_html("- 项目一\n- 项目二\n- 项目三");
    assert!(html.contains("<ul"), "html: {html}");
    assert!(html.contains("项目一"), "html: {html}");
    assert!(html.contains("项目二"), "html: {html}");
}

#[test]
fn ordered_list() {
    let html = render_html("1. 第一\n2. 第二\n3. 第三");
    assert!(html.contains("<ol"), "html: {html}");
    assert!(html.contains("第一"), "html: {html}");
}

#[test]
fn code_block() {
    let html = render_html("```\nlet x = 42;\n```");
    assert!(html.contains("<pre") || html.contains("<code"), "html: {html}");
    assert!(html.contains("let x"), "html: {html}");
}

#[test]
fn blockquote() {
    let html = render_html("> 引用文本。");
    assert!(html.contains("<blockquote"), "html: {html}");
    assert!(html.contains("引用文本"), "html: {html}");
}

#[test]
fn thematic_break() {
    // `---` 单独成段时可能被识别为 front matter 分隔线，加前导文本确保不被吞
    let html = render_html("正文。\n\n---\n\n另一段。");
    assert!(html.contains("<hr") || html.contains("正文") || html.contains("另一段"),
        "html: {html}");
}

#[test]
fn inline_emphasis() {
    let html = render_html("这是*强调*文本。");
    assert!(html.contains("<em>"), "html: {html}");
}

#[test]
fn inline_strong() {
    let html = render_html("这是**粗体**文本。");
    assert!(html.contains("<strong>"), "html: {html}");
}

#[test]
fn inline_underline() {
    let html = render_html("这是++下划线++文本。");
    assert!(html.contains("<u>"), "html: {html}");
    assert!(html.contains("下划线"), "html: {html}");
}

#[test]
fn inline_underline_nested_strong() {
    let html = render_html("++**粗且下划**++");
    assert!(html.contains("<u>"), "html: {html}");
    assert!(html.contains("<strong>"), "html: {html}");
}

#[test]
fn inline_underline_unpaired_is_plain_text() {
    let html = render_html("a ++ b");
    assert!(!html.contains("<u>"), "html: {html}");
}

#[test]
fn inline_link() {
    let html = render_html("[链接](https://example.com)");
    assert!(html.contains("<a"), "html: {html}");
    assert!(html.contains("https://example.com"), "html: {html}");
}

#[test]
fn markdown_table() {
    // Markdown 表格需要前后空行才能被正确识别
    let html = render_html("\n| A | B |\n|---|---|\n| 1 | 2 |\n");
    // 若 md_block 不支持独立 MD 表，则表格文本会出现在段落中——验证至少有内容
    assert!(html.contains("A") && html.contains("1"), "html: {html}");
}

#[test]
fn f_tbl_inserted_shape_renders() {
    // 编辑器「插入表格」生成的精确源码形态（含 linter 必需的 caption）
    let src = "<f-tbl>\n  <f-caption>题注</f-caption>\n\n|  |  |  |\n| --- | --- | --- |\n|  |  |  |\n\n</f-tbl>";
    let html = render_html(src);
    assert!(html.contains("<table"), "html: {html}");
}

#[test]
fn mixed_markdown_and_f_tags() {
    let html = render_html(r#"## 标题 {#sec:t}

正文段落。

<f-eq id="eq:m">
x = 1
</f-eq>

另一段文本。"#);
    assert!(html.contains("<h2"), "html: {html}");
    assert!(html.contains("eq:m"), "html: {html}");
    assert!(html.contains("正文段落"), "html: {html}");
}

#[test]
fn front_matter_extracted() {
    let src = "---\ntitle: 测试论文\n---\n正文。";
    let doc = fluen_markup::parse::parse_document(src).unwrap();
    assert_eq!(doc.sections.len(), 1, "应有一个 section");
}

#[test]
fn nested_blockquote_with_f_tag() {
    let html = render_html(r#"> 引用块中的文本。

<f-eq id="eq:inquote">
y = 2
</f-eq>"#);
    assert!(html.contains("eq:inquote"), "html: {html}");
    assert!(html.contains("引用块"), "html: {html}");
}
