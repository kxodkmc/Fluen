//! # Fluen Markup SDK
//!
//! 论文标记规范 v1.1 的 Rust 解析与渲染 SDK。
//!
//! 在 Markdown 之上叠加 7 个 `f-` 标签（编号浮动体、稳定交叉引用、文献库绑定、
//! 编号公式/定理），原生优先、失效可降级。本 crate 把"解析 → 校验 → 解析编号 → 渲染"
//! 四阶段解耦，各阶段可独立使用，亦可经 [`Pipeline`] 一站式完成。
//!
//! ## 快速开始
//!
//! ```no_run
//! use fluen_markup::{Pipeline, HtmlRenderer, Options, InMemoryReferences, ReferenceEntry};
//!
//! let src = r#"
//! ## 引言 {#sec:intro}
//!
//! 近年来，<f-cite ref="ref-a"/> 提出的方法表现突出（见 <f-xref to="fig:f"/>）。
//!
//! <f-eq id="eq:e">
//! e^{i\pi} + 1 = 0
//! </f-eq>
//!
//! <f-fig id="fig:f" src="assets/f.png" alt="示例">
//!   <f-caption>示例图。</f-caption>
//! </f-fig>
//! "#;
//!
//! let mut refs = InMemoryReferences::new();
//! refs.insert("ref-a", ReferenceEntry { authors: vec!["Chen".into()], year: Some("2020".into()), title: None });
//!
//! let html = Pipeline::new(Options::default())
//!     .references(refs)
//!     .parse(src).unwrap()
//!     .resolve_or_degrade()
//!     .render(&HtmlRenderer::default())
//!     .unwrap();
//! ```
//!
//! ## 模块化设计
//!
//! - [`parse`]    Markdown + f- 标签 → AST
//! - [`validate`] Linter（§6.2 / §8.5）
//! - [`resolve`]  自动编号 + 引用解析 + 降级
//! - [`render`]   AST → 目标格式（HTML / Text / 可扩展）
//! - [`context`]  选项、文献库 trait、编号表
//! - [`io`]       文件读写便利
//!
//! 零外部依赖（`serde` 为可选 feature）。全部实现为原生 Rust。

#![forbid(unsafe_code)]

// ── 模块声明 ──
pub mod context;
pub mod error;
pub mod html_table;
pub mod inline_tags;
pub mod io;
pub mod kinds;
pub mod md_block;
pub mod md_inline;
pub mod model;
pub mod parse;
pub mod render;
pub mod resolve;
pub mod validate;

// ── 公开再导出（扁平化最常用 API）──
pub use context::{
    AssetResolver, CiteStyle, InMemoryReferences, LabelLang, NumberingTable, Options,
    ReferenceEntry, ReferenceProvider, RenderContext, ResolvedDocument,
};
pub use error::{MarkupError, Problems, Result, Severity};
pub use kinds::{ClaimType, IdKind, TableFormat, TableSource, TableVariant};
pub use model::*;
pub use render::{HtmlRenderer, Renderer, RenderInput, TextRenderer};

use std::path::Path;

// ═══════════════════════════ Pipeline ═══════════════════════════

/// 一站式流水线：配置 → 解析 → 解析编号 → 渲染。
///
/// 采用构建器风格，每一步返回新的阶段结构，把"状态机"显式化：
///
/// - [`Pipeline`]         配置（选项、文献库）
/// - [`Parsed`]           已解析 AST（未解析编号）
/// - [`Resolved`]         已解析编号与引用（含编号表）
///
/// 这种设计让调用方可以在任意阶段介入：例如解析后做 Lint 报告、解析编号后导出目录。
#[derive(Debug, Clone)]
pub struct Pipeline {
    options: Options,
    refs: InMemoryReferences,
}

impl Default for Pipeline {
    fn default() -> Self { Self::new(Options::default()) }
}

impl Pipeline {
    /// 以给定选项创建。
    pub fn new(options: Options) -> Self {
        Self { options, refs: InMemoryReferences::new() }
    }

    /// 注入内存文献库。重复调用会合并。
    pub fn references(mut self, refs: InMemoryReferences) -> Self {
        // 合并：简单替换（典型用法是单次注入）
        self.refs = refs;
        self
    }

    /// 便捷：注入单个文献条目。
    pub fn reference(mut self, id: impl Into<String>, entry: ReferenceEntry) -> Self {
        self.refs.insert(id, entry);
        self
    }

    /// 解析文本为 [`Parsed`]。
    pub fn parse(self, src: &str) -> Result<Parsed> {
        let doc = parse::parse_document(src)?;
        Ok(Parsed { doc, options: self.options, refs: self.refs })
    }

    /// 从文件读取并解析。
    pub fn parse_file(self, path: &Path) -> Result<Parsed> {
        let doc = io::read_document(path)?;
        Ok(Parsed { doc, options: self.options, refs: self.refs })
    }
}

/// 已解析、尚未解析编号的文档。
#[derive(Debug, Clone)]
pub struct Parsed {
    pub doc: Document,
    pub options: Options,
    pub refs: InMemoryReferences,
}

impl Parsed {
    /// 运行 Linter（解析后结构性校验），返回问题清单。
    pub fn lint(&self) -> Problems {
        validate::lint(&self.doc)
    }

    /// 解析编号与引用。失效引用（无 fallback）会以 Err 返回首个错误；
    /// 若希望降级继续，使用 [`Self::resolve_or_degrade`]。
    pub fn resolve(self) -> Result<Resolved> {
        let mut doc = self.doc;
        let refs = self.refs;
        let numbering = resolve::resolve(&mut doc, &refs, &self.options)?;
        Ok(Resolved { doc, numbering, options: self.options, refs })
    }

    /// 解析编号，但失效引用降级为警告而非硬失败（适合预览场景）。
    /// 编号表始终有效，渲染可继续。
    pub fn resolve_or_degrade(self) -> Resolved {
        let mut doc = self.doc;
        let refs = self.refs;
        let (numbering, _problems) = resolve::resolve_lenient(&mut doc, &refs, &self.options);
        Resolved { doc, numbering, options: self.options, refs }
    }

    /// 只渲染（跳过解析编号；适合调试原始 AST）。
    pub fn render<R: Renderer>(&self, renderer: &R) -> Result<String> {
        let ctx = RenderContext { options: &self.options, refs: &self.refs as &dyn ReferenceProvider };
        renderer.render(&self.doc, &ctx)
    }
}

/// 已解析编号与引用的文档。可被任意 [`Renderer`] 消费。
#[derive(Debug, Clone)]
pub struct Resolved {
    pub doc: Document,
    pub numbering: NumberingTable,
    pub options: Options,
    pub refs: InMemoryReferences,
}

impl Resolved {
    /// 渲染为目标格式字符串。
    pub fn render<R: Renderer>(&self, renderer: &R) -> Result<String> {
        let ctx = RenderContext { options: &self.options, refs: &self.refs as &dyn ReferenceProvider };
        renderer.render(&self.doc, &ctx)
    }

    /// 渲染并写入文件。
    pub fn render_to_file<R: Renderer>(&self, renderer: &R, path: &Path) -> Result<()> {
        let ctx = RenderContext { options: &self.options, refs: &self.refs as &dyn ReferenceProvider };
        renderer.render_to_file(&self.doc, &ctx, path)
    }

    /// 完整性问题清单（resolve 后校验）。
    pub fn problems(&self) -> Problems {
        validate::lint_after_resolve(&self.doc, &self.numbering, &self.refs, &self.options)
    }
}


