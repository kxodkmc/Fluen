//! IO 模块测试：文件读写便利。

use std::io::Write;

use fluen_markup::{io, parse::parse_document, HtmlRenderer, Options, Pipeline, InMemoryReferences};

#[test]
fn read_document_from_file() {
    // 创建临时文件
    let dir = std::env::temp_dir();
    let path = dir.join("fluen_io_test.md");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        write!(f, "## 测试\n\n正文。").unwrap();
    }

    let doc = io::read_document(&path).unwrap();
    assert_eq!(doc.sections.len(), 1);
    assert!(!doc.sections[0].blocks.is_empty());

    let _ = std::fs::remove_file(&path);
}

#[test]
fn write_rendered_to_file() {
    let src = r#"## 引言 {#sec:intro}

正文。<f-cite ref="ref-a"/>
"#;
    let mut refs = InMemoryReferences::new();
    refs.insert("ref-a", fluen_markup::ReferenceEntry {
        authors: vec!["Smith".into()],
        year: Some("2020".into()),
        title: None,
    });

    let resolved = Pipeline::new(Options::default())
        .references(refs)
        .parse(src)
        .unwrap()
        .resolve_or_degrade();

    let path = std::env::temp_dir().join("fluen_io_render.html");
    resolved.render_to_file(&HtmlRenderer::default().with_inline_style(false), &path).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("<h2"), "content: {content}");
    assert!(content.contains("sec:intro"), "content: {content}");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn parse_document_basic() {
    let doc = parse_document("## 标题\n\n段落。").unwrap();
    assert_eq!(doc.sections.len(), 1);
}

#[test]
fn parse_document_with_front_matter() {
    let src = "---\ntitle: 测试\nauthor: 张三\n---\n正文。";
    let doc = parse_document(src).unwrap();
    assert_eq!(doc.sections.len(), 1);
    // front matter 解析后不影响正文
    assert!(!doc.sections[0].blocks.is_empty());
}

#[test]
fn read_nonexistent_file_errors() {
    let path = std::path::Path::new("nonexistent_fluen_test_file.md");
    assert!(io::read_document(path).is_err());
}
