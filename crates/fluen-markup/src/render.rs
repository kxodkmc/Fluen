//! 渲染层：把解析后的 [`Document`] 输出为目标格式。
//!
//! 抽象为 [`Renderer`] trait，每种目标格式一个实现（[`html::HtmlRenderer`]、
//! [`text::TextRenderer`]）。新增格式（LaTeX/Pandoc/Word/Markdown）只需实现 trait，
//! 不触动解析与解析层。

use crate::context::{NumberingTable, Options, ReferenceProvider, RenderContext};
use crate::model::Document;

pub mod escape;
pub mod html;
pub mod text;

pub use html::HtmlRenderer;
pub use text::TextRenderer;

/// 渲染器接口。`render` 产出最终字符串；`render_to_file` 由默认实现落盘。
pub trait Renderer {
    /// 目标格式的人类可读名称（如 "html" / "text"）。
    fn format_name(&self) -> &'static str;

    /// 把已解析文档渲染为字符串。
    fn render(&self, doc: &Document, ctx: &RenderContext<'_>) -> crate::error::Result<String>;

    /// 渲染并写入文件。默认实现调用 [`render`] 后一次写入。
    fn render_to_file(
        &self,
        doc: &Document,
        ctx: &RenderContext<'_>,
        path: &std::path::Path,
    ) -> crate::error::Result<()> {
        let content = self.render(doc, ctx)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

/// 渲染所需"已解析文档"的打包（避免每次手传 numbering/options）。
/// 调用方通常由 [`crate::resolve`] 产出的 numbering 与原始 options 组装。
#[derive(Clone)]
pub struct RenderInput<'a> {
    pub doc: &'a Document,
    pub numbering: &'a NumberingTable,
    pub options: &'a Options,
    pub refs: &'a dyn ReferenceProvider,
}

impl<'a> std::fmt::Debug for RenderInput<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderInput")
            .field("doc", &self.doc)
            .field("numbering", &self.numbering)
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

impl<'a> RenderInput<'a> {
    /// 构造渲染上下文。
    pub fn context(&self) -> RenderContext<'a> {
        RenderContext { options: self.options, refs: self.refs }
    }
}
