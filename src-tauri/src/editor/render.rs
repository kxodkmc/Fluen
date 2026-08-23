//! HTML 渲染封装——委托 fluen-markup Pipeline 将 MD 渲染为 HTML。
//!
//! 渲染为只读派生产物，不回写 source_md。
//! 使用 `resolve_or_degrade` 降级渲染，失效引用不报错。
//!
//! 本模块为纯函数（不做 IO）：文献库由调用方经 [`super::reflib`] 加载后传入。

use fluen_markup::{HtmlRenderer, InMemoryReferences, Options, Pipeline};

use super::error::{EditorError, Result};

/// 将 MD 文本渲染为 HTML。
///
/// `refs` 为文献库（引用键 → 元数据），决定 `<f-cite>` 能否命中；
/// 空库时所有引用降级显示 fallback。由调用方决定来源（典型：当前项目索引）。
///
/// 使用 `Pipeline::new(options).references(refs).parse(src).resolve_or_degrade()
/// .render(&HtmlRenderer::default())` 流程。失效引用（无 fallback）会降级渲染，不报错。
pub fn render_to_html(md: &str, options: &Options, refs: &InMemoryReferences) -> Result<String> {
    Pipeline::new(options.clone())
        .references(refs.clone())
        .parse(md)
        .map_err(|e| EditorError::RenderError(format!("解析失败: {}", e)))?
        .resolve_or_degrade()
        .render(&HtmlRenderer::default())
        .map_err(|e| EditorError::RenderError(format!("渲染失败: {}", e)))
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use fluen_markup::InMemoryReferences;

    /// 空文献库（引用全部降级）。
    fn empty_refs() -> InMemoryReferences {
        InMemoryReferences::new()
    }

    #[test]
    fn render_simple_markdown() {
        let html = render_to_html("# 标题\n\n正文", &Options::default(), &empty_refs()).unwrap();
        assert!(html.contains("<h1"));
        assert!(html.contains("标题"));
        assert!(html.contains("正文"));
    }

    #[test]
    fn render_with_f_tags() {
        let md = r#"
## 引言 {#sec:intro}

<f-eq id="eq:e">
e^{i\pi} + 1 = 0
</f-eq>
"#;
        let html = render_to_html(md, &Options::default(), &empty_refs()).unwrap();
        // 应包含公式相关 HTML
        assert!(!html.is_empty());
    }

    #[test]
    fn render_degraded_reference() {
        // 引用不存在的文献，应降级而非报错
        let md = r#"引用<f-cite ref="ref-not-exist"/>测试"#;
        let result = render_to_html(md, &Options::default(), &empty_refs());
        // 降级渲染应成功（可能含警告标记）
        assert!(result.is_ok());
    }

    #[test]
    fn render_resolves_reference_from_library() {
        // 命中文献库的引用显示作者-年，不降级
        let mut refs = InMemoryReferences::new();
        refs.insert(
            "ref-hit",
            fluen_markup::ReferenceEntry {
                authors: vec!["张三".into()],
                year: Some("2025".into()),
                title: Some("标题".into()),
            },
        );
        let md = r#"引用<f-cite ref="ref-hit" fallback="后备"/>测试"#;
        let mut options = Options::default();
        options.cite_style = fluen_markup::CiteStyle::AuthorYear;
        let html = render_to_html(md, &options, &refs).unwrap();
        assert!(html.contains("张三, 2025"));
        assert!(!html.contains("⚠"));
        assert!(!html.contains("后备"));
    }

    #[test]
    fn render_empty_md() {
        let html = render_to_html("", &Options::default(), &empty_refs()).unwrap();
        // 空 MD 应返回有效（可能为空）HTML
        assert!(html.is_empty() || html.contains("<"));
    }

    #[test]
    fn render_hides_section_marker_comment() {
        // 章节标记是 HTML 注释，预览中不应出现可见文本
        let md = "<!-- @sec_id:sec-abc12345 -->\n# 标题\n\n正文";
        let html = render_to_html(md, &Options::default(), &empty_refs()).unwrap();
        assert!(html.contains("标题"));
        assert!(html.contains("正文"));
        assert!(!html.contains("@sec_id"));
        assert!(!html.contains("sec-abc12345"));
    }
}
