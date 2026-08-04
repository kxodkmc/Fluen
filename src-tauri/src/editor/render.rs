//! HTML 渲染封装——委托 fluen-markup Pipeline 将 MD 渲染为 HTML。
//!
//! 渲染为只读派生产物，不回写 source_md。
//! 使用 `resolve_or_degrade` 降级渲染，失效引用不报错。

use fluen_markup::{HtmlRenderer, Options, Pipeline};

use super::error::{EditorError, Result};

/// 将 MD 文本渲染为 HTML。
///
/// 使用 `Pipeline::new(options).parse(src).resolve_or_degrade().render(&HtmlRenderer::default())` 流程。
/// 失效引用（无 fallback）会降级渲染，不报错。
pub fn render_to_html(md: &str, options: &Options) -> Result<String> {
    Pipeline::new(options.clone())
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

    #[test]
    fn render_simple_markdown() {
        let html = render_to_html("# 标题\n\n正文", &Options::default()).unwrap();
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
        let html = render_to_html(md, &Options::default()).unwrap();
        // 应包含公式相关 HTML
        assert!(!html.is_empty());
    }

    #[test]
    fn render_degraded_reference() {
        // 引用不存在的文献，应降级而非报错
        let md = r#"引用<f-cite ref="ref-not-exist"/>测试"#;
        let result = render_to_html(md, &Options::default());
        // 降级渲染应成功（可能含警告标记）
        assert!(result.is_ok());
    }

    #[test]
    fn render_empty_md() {
        let html = render_to_html("", &Options::default()).unwrap();
        // 空 MD 应返回有效（可能为空）HTML
        assert!(html.is_empty() || html.contains("<"));
    }
}
