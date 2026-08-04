//! 渲染/解析上下文：选项、文献库提供者、id 编号表、引用风格。
//!
//! 这是连接「解析」与「渲染」的中枢：
//! - 解析阶段把所有可编号元素与 `<f-xref>`/`<f-cite>` 目标填入 [`NumberingTable`]，
//!   再由 `resolve` 模块回写 `resolved_*` 字段。
//! - 渲染阶段读取 [`RenderContext`] 决定风格（数字制 / 作者-年、中/英标签、表格样式）。

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::kinds::{ClaimType, IdKind};
use crate::model::Document;

// ───────────────────────── 解析/渲染选项 ─────────────────────────

/// 顶层选项。零依赖默认值与规范默认严格对齐。
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Options {
    /// Linter 严格模式（§4.3 / §8.5）：开启后 fallback 与实际解析值不一致会告警。
    pub strict_lint: bool,
    /// 引用风格。
    pub cite_style: CiteStyle,
    /// 标签语言（影响图/表/定理显示文字）。
    pub label_lang: LabelLang,
    /// 是否允许在降级显示时附加 `⚠` 标记（§4.3 渲染标记）。
    pub degrade_marker: bool,
    /// 定理类是否共享单一计数流（§3.2，front matter 可配置）。
    pub shared_claim_counter: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            strict_lint: false,
            cite_style: CiteStyle::Numeric,
            label_lang: LabelLang::Zh,
            degrade_marker: true,
            shared_claim_counter: false,
        }
    }
}

/// 引用风格。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CiteStyle {
    /// 数字制：`[1]` / `[1,2,3]`。
    #[default]
    Numeric,
    /// 作者-年：`Smith, 2020`。
    AuthorYear,
}

/// 标签语言。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LabelLang {
    #[default]
    Zh,
    En,
}

// ───────────────────────── 文献库提供者 ─────────────────────────

/// 一条文献元数据（最小集，足够生成两种风格的显示）。
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ReferenceEntry {
    /// 作者列表（姓 / 全名均可，由宿主填充）。
    pub authors: Vec<String>,
    /// 年份。
    pub year: Option<String>,
    /// 标题（用于作者-年降级或悬停提示）。
    pub title: Option<String>,
}

impl ReferenceEntry {
    /// 生成作者-年显示文字（如 `Smith et al., 2020` / `Smith & Jones, 2020`）。
    pub fn author_year(&self) -> String {
        let year = self.year.as_deref().unwrap_or("n.d.");
        match self.authors.len() {
            0 => format!("Anonymous, {year}"),
            1 => format!("{}, {year}", self.authors[0]),
            2 => format!("{} & {}, {year}", self.authors[0], self.authors[1]),
            _ => format!("{} et al., {year}", self.authors[0]),
        }
    }
}

/// 文献库提供者 trait：把 `ref` id 解析为元数据。
/// 宿主可实现此 trait 接入任意来源（JSON 文件、数据库、远程）。
/// 本 crate 内置 [`InMemoryReferences`] 与（在 io 模块）从 JSON 加载的实现。
pub trait ReferenceProvider {
    fn get(&self, id: &str) -> Option<&ReferenceEntry>;
}

/// 内存文献库：`Vec` + 简单 map。用于测试与小型场景。
#[derive(Debug, Clone, Default)]
pub struct InMemoryReferences {
    map: HashMap<String, ReferenceEntry>,
}
impl InMemoryReferences {
    pub fn new() -> Self { Self { map: HashMap::new() } }
    pub fn insert(&mut self, id: impl Into<String>, entry: ReferenceEntry) -> &mut Self {
        self.map.insert(id.into(), entry);
        self
    }
}
impl ReferenceProvider for InMemoryReferences {
    fn get(&self, id: &str) -> Option<&ReferenceEntry> { self.map.get(id) }
}

// ───────────────────────── 编号表 ─────────────────────────

/// 全文 id → 编号信息。组装后（§8.3）统一收集并重排。
#[derive(Debug, Clone, Default)]
pub struct NumberingTable {
    /// id → (族, 编号)。
    by_id: HashMap<String, (IdKind, usize)>,
    /// 章节多级号缓存：sec id → "2.1"。
    section_numbers: HashMap<String, String>,
}
impl NumberingTable {
    pub fn new() -> Self { Self::default() }

    /// 注册一个 id（编号由调用方按出现顺序给定）。
    pub fn set(&mut self, id: impl Into<String>, kind: IdKind, number: usize) {
        self.by_id.insert(id.into(), (kind, number));
    }

    /// 注册章节多级号。
    pub fn set_section(&mut self, id: impl Into<String>, number: impl Into<String>) {
        self.section_numbers.insert(id.into(), number.into());
    }

    /// 查询 id 的 (族, 编号)。
    pub fn get(&self, id: &str) -> Option<(IdKind, usize)> { self.by_id.get(id).copied() }

    /// 查询章节多级号。
    pub fn section_of(&self, id: &str) -> Option<&str> {
        self.section_numbers.get(id).map(|s| s.as_str())
    }

    /// 是否已注册该 id（用于重复检测）。
    pub fn contains(&self, id: &str) -> bool { self.by_id.contains_key(id) }
}

// ───────────────────────── 渲染上下文 ─────────────────────────

/// 渲染时所需的一切上下文。由 `parse → resolve` 流程产出的 [`ResolvedDocument`]
/// 连同 [`Options`] 一并传入渲染器。
#[derive(Clone)]
pub struct RenderContext<'a> {
    pub options: &'a Options,
    pub refs: &'a dyn ReferenceProvider,
}

impl<'a> fmt::Debug for RenderContext<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RenderContext")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

/// 解析后的文档 + 编号表 + 选项的打包类型，便于渲染器一次性消费。
#[derive(Debug, Clone)]
pub struct ResolvedDocument {
    pub document: Document,
    pub numbering: NumberingTable,
    pub options: Options,
}

// ───────────────────────── id 前缀文字（§4.2）─────────────────

/// 按 id 族与语言生成前缀文字，如「图 1」「式 (1)」「第 2.1 节」「定理 1」。
/// 括号/编号风格集中于此一处。
pub fn format_xref_text(kind: IdKind, number: usize, sec: Option<&str>, lang: LabelLang) -> String {
    match lang {
        LabelLang::Zh => match kind {
            IdKind::Figure => format!("图 {number}"),
            IdKind::Table => format!("表 {number}"),
            IdKind::Equation => format!("式 ({number})"),
            IdKind::Section => format!("第 {} 节", sec.unwrap_or(&format!("{number}"))),
            IdKind::Claim(t) => format!("{} {}", claim_label(t, lang), number),
        },
        LabelLang::En => match kind {
            IdKind::Figure => format!("Figure {number}"),
            IdKind::Table => format!("Table {number}"),
            IdKind::Equation => format!("Eq. ({number})"),
            IdKind::Section => format!("§{}", sec.unwrap_or(&format!("{number}"))),
            IdKind::Claim(t) => format!("{} {}", claim_label(t, lang), number),
        },
    }
}

/// claim 标签文字。
pub fn claim_label(t: ClaimType, lang: LabelLang) -> &'static str {
    match lang {
        LabelLang::Zh => t.label_zh(),
        LabelLang::En => t.label_en(),
    }
}

// ───────────────────────── 资源解析（src 路径）─────────────────

/// 资源（图/表数据）相对 manuscript 根的解析。
/// 本 crate 不强制文件系统；宿主可提供 base 路径用于 IO 校验与导出拷贝。
#[derive(Debug, Clone, Default)]
pub struct AssetResolver {
    /// manuscript 根（绝对或相对路径）。
    pub base: Option<PathBuf>,
}
impl AssetResolver {
    pub fn new(base: impl AsRef<Path>) -> Self { Self { base: Some(base.as_ref().to_path_buf()) } }
    /// 把相对路径解析为（可能的）绝对路径。
    pub fn resolve(&self, rel: &str) -> Option<PathBuf> {
        self.base.as_ref().map(|b| b.join(rel))
    }
}
