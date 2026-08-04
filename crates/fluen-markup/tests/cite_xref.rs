//! 文献引用（`<f-cite>`）与交叉引用（`<f-xref>`）的集成测试。
//!
//! 覆盖：解析、编号解析、两种引用风格、降级 fallback、缺失报错、loc 位置。

use fluen_markup::{
    CiteStyle, HtmlRenderer, InMemoryReferences, LabelLang, Options, Pipeline, ReferenceEntry,
    TextRenderer,
};

fn opts_numeric() -> Options {
    Options {
        cite_style: CiteStyle::Numeric,
        label_lang: LabelLang::Zh,
        degrade_marker: false,
        ..Default::default()
    }
}

fn opts_author_year() -> Options {
    Options {
        cite_style: CiteStyle::AuthorYear,
        label_lang: LabelLang::Zh,
        degrade_marker: false,
        ..Default::default()
    }
}

fn make_refs() -> InMemoryReferences {
    let mut refs = InMemoryReferences::new();
    refs.insert("ref-a", ReferenceEntry {
        authors: vec!["Smith".into()],
        year: Some("2020".into()),
        title: Some("Example Paper".into()),
    });
    refs.insert("ref-b", ReferenceEntry {
        authors: vec!["Chen".into(), "Lee".into()],
        year: Some("2021".into()),
        title: None,
    });
    refs.insert("ref-c", ReferenceEntry {
        authors: vec!["Wang".into(), "Zhang".into(), "Liu".into()],
        year: Some("2022".into()),
        title: None,
    });
    refs
}

// ───────────────────────── 文献引用 ─────────────────────────

#[test]
fn cite_numeric_single() {
    let src = "如<f-cite ref=\"ref-a\"/>所示。";
    let resolved = Pipeline::new(opts_numeric())
        .references(make_refs())
        .parse(src)
        .unwrap()
        .resolve_or_degrade();

    let html = resolved.render(&HtmlRenderer::default().with_inline_style(false)).unwrap();
    assert!(html.contains("<sup class=\"f-cite\">[1]</sup>"), "html: {html}");
}

#[test]
fn cite_numeric_multiple() {
    let src = "参见<f-cite ref=\"ref-a,ref-b,ref-c\"/>。";
    let resolved = Pipeline::new(opts_numeric())
        .references(make_refs())
        .parse(src)
        .unwrap()
        .resolve_or_degrade();

    let html = resolved.render(&HtmlRenderer::default().with_inline_style(false)).unwrap();
    // 按首次出现顺序编号：a=1, b=2, c=3
    assert!(html.contains("<sup class=\"f-cite\">[1,2,3]</sup>"), "html: {html}");
}

#[test]
fn cite_numeric_with_loc() {
    let src = "详见<f-cite ref=\"ref-a\" loc=\"p. 42\"/>。";
    let resolved = Pipeline::new(opts_numeric())
        .references(make_refs())
        .parse(src)
        .unwrap()
        .resolve_or_degrade();

    let html = resolved.render(&HtmlRenderer::default().with_inline_style(false)).unwrap();
    assert!(html.contains("<sup class=\"f-cite\">[1, p. 42]</sup>"), "html: {html}");
}

#[test]
fn cite_author_year() {
    let src = "如<f-cite ref=\"ref-a\"/>与<f-cite ref=\"ref-c\"/>所述。";
    let resolved = Pipeline::new(opts_author_year())
        .references(make_refs())
        .parse(src)
        .unwrap()
        .resolve_or_degrade();

    let html = resolved.render(&HtmlRenderer::default().with_inline_style(false)).unwrap();
    assert!(html.contains("Smith, 2020"), "html: {html}");
    assert!(html.contains("Wang et al., 2022"), "html: {html}");
}

#[test]
fn cite_author_year_with_loc() {
    let src = "见<f-cite ref=\"ref-a\" loc=\"ch. 3\"/>。";
    let resolved = Pipeline::new(opts_author_year())
        .references(make_refs())
        .parse(src)
        .unwrap()
        .resolve_or_degrade();

    let html = resolved.render(&HtmlRenderer::default().with_inline_style(false)).unwrap();
    assert!(html.contains("Smith, 2020, ch. 3"), "html: {html}");
}

#[test]
fn cite_fallback_missing_ref() {
    let src = "见<f-cite ref=\"ref-missing\" fallback=\"Anonymous, 1999\"/>。";
    let resolved = Pipeline::new(opts_numeric())
        .references(make_refs())
        .parse(src)
        .unwrap()
        .resolve_or_degrade();

    let html = resolved.render(&HtmlRenderer::default().with_inline_style(false)).unwrap();
    assert!(html.contains("Anonymous, 1999"), "html: {html}");
    // 应产生 warning 级问题
    assert!(resolved.problems().iter().any(|p| p.to_string().contains("未命中")), "应有未命中问题");
}

#[test]
fn cite_missing_no_fallback_is_error() {
    let src = "见<f-cite ref=\"ref-missing\"/>。";
    let parsed = Pipeline::new(opts_numeric())
        .references(make_refs())
        .parse(src)
        .unwrap();

    let problems = parsed.resolve_or_degrade().problems();
    assert!(problems.iter().any(|p| p.to_string().contains("未命中") && !p.is_warning()),
        "缺失 fallback 的未命中文献应为 error");
}

#[test]
fn cite_text_renderer_numeric() {
    let src = "见<f-cite ref=\"ref-a\" loc=\"p. 42\"/>。";
    let resolved = Pipeline::new(opts_numeric())
        .references(make_refs())
        .parse(src)
        .unwrap()
        .resolve_or_degrade();

    let text = resolved.render(&TextRenderer).unwrap();
    assert!(text.contains("[1, p. 42]"), "text: {text}");
}

#[test]
fn cite_text_renderer_author_year() {
    let src = "见<f-cite ref=\"ref-a\" loc=\"p. 42\"/>。";
    let resolved = Pipeline::new(opts_author_year())
        .references(make_refs())
        .parse(src)
        .unwrap()
        .resolve_or_degrade();

    let text = resolved.render(&TextRenderer).unwrap();
    assert!(text.contains("(Smith, 2020, p. 42)"), "text: {text}");
}

// ───────────────────────── 交叉引用 ─────────────────────────

#[test]
fn xref_figure() {
    let src = r#"如<f-xref to="fig:overview"/>所示。

<f-fig id="fig:overview" src="assets/o.svg" alt="总览">
  <f-caption>总览图。</f-caption>
</f-fig>"#;
    let resolved = Pipeline::new(opts_numeric())
        .references(make_refs())
        .parse(src)
        .unwrap()
        .resolve_or_degrade();

    let html = resolved.render(&HtmlRenderer::default().with_inline_style(false)).unwrap();
    assert!(html.contains(r##"<a class="f-xref" href="#fig:overview">图 1</a>"##), "html: {html}");
}

#[test]
fn xref_table_equation_claim() {
    let src = r#"参见<f-xref to="tbl:results"/>、<f-xref to="eq:loss"/>与<f-xref to="thm:conv"/>。

<f-tbl id="tbl:results">
  <f-caption>结果。</f-caption>

  | A | B |
  |---|---|
  | 1 | 2 |
</f-tbl>

<f-eq id="eq:loss">
L = -\log p
</f-eq>

<f-claim id="thm:conv" type="theorem">收敛。</f-claim>"#;

    let resolved = Pipeline::new(opts_numeric())
        .references(make_refs())
        .parse(src)
        .unwrap()
        .resolve_or_degrade();

    let html = resolved.render(&HtmlRenderer::default().with_inline_style(false)).unwrap();
    assert!(html.contains("表 1"), "html: {html}");
    assert!(html.contains("式 (1)"), "html: {html}");
    assert!(html.contains("定理 1"), "html: {html}");
}

#[test]
fn xref_section() {
    let src = r#"详见<f-xref to="sec:method"/>。

## 方法 {#sec:method}"#;
    let resolved = Pipeline::new(opts_numeric())
        .references(make_refs())
        .parse(src)
        .unwrap()
        .resolve_or_degrade();

    let html = resolved.render(&HtmlRenderer::default().with_inline_style(false)).unwrap();
    assert!(html.contains(r##"<a class="f-xref" href="#sec:method">"##), "html: {html}");
}

#[test]
fn xref_fallback_missing_target() {
    let src = "见<f-xref to=\"fig:missing\" fallback=\"图（旧图）\"/>。";
    let resolved = Pipeline::new(opts_numeric())
        .references(make_refs())
        .parse(src)
        .unwrap()
        .resolve_or_degrade();

    let html = resolved.render(&HtmlRenderer::default().with_inline_style(false)).unwrap();
    assert!(html.contains("图（旧图）"), "html: {html}");
    assert!(resolved.problems().iter().any(|p| p.to_string().contains("目标不存在")), "应有目标不存在问题");
}

#[test]
fn xref_missing_no_fallback_is_error() {
    let src = "见<f-xref to=\"fig:missing\"/>。";
    let parsed = Pipeline::new(opts_numeric())
        .references(make_refs())
        .parse(src)
        .unwrap();

    let problems = parsed.resolve_or_degrade().problems();
    assert!(problems.iter().any(|p| p.to_string().contains("目标不存在") && !p.is_warning()),
        "缺失 fallback 的未命中交叉引用应为 error");
}

#[test]
fn xref_override_text() {
    let src = r#"<f-xref to="fig:x">该图</f-xref>说明了问题。"#;
    let resolved = Pipeline::new(opts_numeric())
        .references(make_refs())
        .parse(src)
        .unwrap()
        .resolve_or_degrade();

    let html = resolved.render(&HtmlRenderer::default().with_inline_style(false)).unwrap();
    assert!(html.contains(r##"<a class="f-xref f-xref-override" href="#fig:x">该图</a>"##), "html: {html}");
}

// ───────────────────────── 组合场景 ─────────────────────────

#[test]
fn cite_and_xref_together() {
    let src = r#"文献<f-cite ref="ref-a"/>中的方法如<f-xref to="fig:f"/>所示。

<f-fig id="fig:f" src="assets/f.svg" alt="图">
  <f-caption>示例图。</f-caption>
</f-fig>"#;

    let resolved = Pipeline::new(opts_numeric())
        .references(make_refs())
        .parse(src)
        .unwrap()
        .resolve_or_degrade();

    let html = resolved.render(&HtmlRenderer::default().with_inline_style(false)).unwrap();
    assert!(html.contains("[1]"), "html: {html}");
    assert!(html.contains("图 1"), "html: {html}");
}
