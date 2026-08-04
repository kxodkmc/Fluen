//! AST 模型。Document → Block → Inline 三层，f-标签节点与原生 Markdown 节点统一树。
//!
//! 设计要点：
//! - 行内叶子（Inline）与块（Block）分离，分别由 `md_inline` 与 `md_block` 产出，
//!   也作为 f-标签内部内容的二次解析结果。
//! - 解析后填入的编号/引用结果（`resolved` 字段）与原始结构分离，便于：
//!     1) 渲染器只关心 `resolved`；
//!     2) 单元测试可分别断言结构与解析结果。
//! - 所有节点携带 1-based 起始行号 `line`，供 Linter 报告与降级标记定位。

use crate::kinds::{ClaimType, IdKind, TableSource, TableVariant};

// ───────────────────────── 文档 ─────────────────────────

/// 顶层文档：可选 front matter + 章节序列 + 标题。
///
/// 对单一 `.md` 文件而言，章节序列仅含一个 Section（该文件本身）。
/// 多章节组装（§8.3）由宿主按 `sections.json` 顺序拼接 `Document::sections` 后，
/// 再调用解析（resolve）重排编号。
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Document {
    /// YAML front matter（原始键值，未做强类型约束，留给宿主解释）。
    pub front_matter: FrontMatter,
    /// 章节（单文件解析通常为 1 个）。
    pub sections: Vec<Section>,
}

/// Front matter：宽松键值集合（与规范 §1「不侵入标签语法」一致）。
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FrontMatter {
    /// 原始键值对（顺序保留）。
    pub entries: Vec<(String, String)>,
}

impl FrontMatter {
    /// 取值（首次匹配）。
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    /// 便捷取标题（`title` 键）。
    pub fn title(&self) -> Option<&str> {
        self.get("title")
    }
}

// ───────────────────────── 章节 ─────────────────────────

/// 章节：对应一个 `sec-*.md` 文件（或组装后的一个片段）。
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Section {
    /// 章节标题（来自 front matter 或首个 H1）。
    pub title: String,
    /// 可选章节 id（front matter 或 `## … {#sec:…}`）。
    pub id: Option<String>,
    /// 块级内容。
    pub blocks: Vec<Block>,
    /// 该章节首行行号（1-based）。
    pub line: usize,
}

// ───────────────────────── 块级 ─────────────────────────

/// 块级节点。
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Block {
    /// 标题。`level` ∈ 1..=6，`id` 为可选的 `{#sec:…}` 锚点。
    Heading { level: u8, text: Vec<Inline>, id: Option<String>, line: usize },
    /// 段落。
    Paragraph(Vec<Inline>, usize),
    /// 列表（有序/无序）。items[i] 为一段落式行内序列；嵌套暂以平坦缩进表达。
    List { ordered: bool, items: Vec<ListItem>, line: usize },
    /// 引用块 `>`。
    BlockQuote(Vec<Block>, usize),
    /// 代码块（围栏 ``` 或缩进）。`lang` 可选。
    CodeBlock { lang: Option<String>, code: String, line: usize },
    /// 分隔线 `---`。
    ThematicBreak(usize),

    // ── f- 标签块 ──
    /// 编号公式（§5.1）。
    Equation(FluenEq),
    /// 图（§5.2）。
    Figure(FluenFig),
    /// 表（§5.3）。
    Table(FluenTbl),
    /// 定理/定义/引理…（§5.4）。
    Claim(FluenClaim),
}

// ───────────────────────── 行内 ─────────────────────────

/// 行内节点。
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Inline {
    /// 纯文本（已反转义）。
    Text(String),
    /// 强调 `*x*` / `_x_`。
    Emphasis(Vec<Inline>),
    /// 强调 `**x**` / `__x__`。
    Strong(Vec<Inline>),
    /// 删除线 `~~x~~`。
    Strikethrough(Vec<Inline>),
    /// 行内代码 `` `x` ``。内容为原始文本（不二次解析）。
    Code(String),
    /// 行内数学 `$...$`。内容为 LaTeX 源（原样）。
    Math(String),
    /// 链接 `[text](url)`。
    Link { text: Vec<Inline>, url: String, title: Option<String> },
    /// 图片 `![alt](src)`（原生 Markdown 图片）。
    Image { alt: String, src: String, title: Option<String> },
    /// 换行（行尾两空格 / 反斜杠）。
    SoftBreak,
    /// 文献引用（§4.1）。
    Cite(FluenCite),
    /// 交叉引用（§4.2）。
    Xref(FluenXref),
}

// ───────────────────────── 列表项 ─────────────────────────

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ListItem {
    /// 该项的行内内容（块级嵌套暂不展开，保留为单一"段落"序列）。
    pub content: Vec<Inline>,
}

// ═══════════════════════════ f- 标签节点 ═══════════════════════════

/// `<f-eq>` 编号公式。
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FluenEq {
    /// 可选 id（如 `eq:euler`）。
    pub id: Option<String>,
    /// LaTeX 源（已去首尾空白；未做任何解析/反转义）。
    pub latex: String,
    pub line: usize,
    /// 解析后：自动编号（从 1 起）。解析前为 None。
    pub resolved_number: Option<usize>,
}

/// `<f-cite>` 文献引用。
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FluenCite {
    /// `ref` 拆分后的文献 id 列表（逗号分隔）。
    pub refs: Vec<String>,
    /// 可选 `loc`（页/节定位，原样透传）。
    pub loc: Option<String>,
    /// 可选 `fallback`（降级后备文字）。
    pub fallback: Option<String>,
    /// 行号。
    pub line: usize,
    /// 解析后：是否命中文献库。未命中且有 fallback → 降级。
    /// `resolved_keys` 为命中文献的"渲染键"（如 `[1]` 或作者-年），顺序与 refs 对齐；
    /// 未命中项在该位置为 None，渲染时由 fallback 兜底。
    pub resolved: ResolvedCite,
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ResolvedCite {
    /// 每条 ref 的解析结果。长度与 `FluenCite::refs` 相同。
    pub entries: Vec<Option<CiteResolvedEntry>>,
    /// 是否整体命中（所有 ref 都命中）。
    pub all_hit: bool,
}

/// 单条文献引用的解析结果。
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CiteResolvedEntry {
    /// 用于渲染的显示键，如 `[1]`（数字制）或 `Smith, 2020`（作者-年）。
    pub display: String,
    /// 是否命中。
    pub hit: bool,
}

/// `<f-xref>` 交叉引用。
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FluenXref {
    /// `to` 目标 id。
    pub to: String,
    /// 可选 `fallback`。
    pub fallback: Option<String>,
    /// 可选手写覆盖内容（`<f-xref to="x">该图</f-xref>`）。存在则失去自动更新。
    pub override_text: Option<Vec<Inline>>,
    pub line: usize,
    /// 解析后：目标存在时的渲染文字（如「图 1」）。
    pub resolved_text: Option<String>,
    /// 解析后：目标是否存在。
    pub hit: bool,
}

/// `<f-fig>` 图。
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FluenFig {
    pub id: Option<String>,
    /// 相对 manuscript 根的路径。
    pub src: String,
    /// 图片描述（§4.4）。
    pub alt: Option<String>,
    /// 有且仅有一个 `<f-caption>`。
    pub caption: Vec<Inline>,
    pub line: usize,
    /// 解析后：自动编号。
    pub resolved_number: Option<usize>,
}

/// `<f-tbl>` 表。
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FluenTbl {
    pub id: Option<String>,
    /// 表体来源（形态 A/B/C 互斥）。
    pub source: TableSource,
    /// 表格样式。
    pub variant: TableVariant,
    pub caption: Vec<Inline>,
    pub line: usize,
    /// 解析后：自动编号。
    pub resolved_number: Option<usize>,
}

/// 表格数据模型（形态 B/C 与形态 A 解析后共用）。
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TableModel {
    /// 表头单元格（每格内容为行内序列，二次解析后）。
    pub header: Vec<Cell>,
    /// 表体行。
    pub body: Vec<Vec<Cell>>,
}

/// 表格单元格。`colspan`/`rowspan` 默认 1。
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Cell {
    pub content: Vec<Inline>,
    pub colspan: u32,
    pub rowspan: u32,
    pub line: usize,
}

impl Cell {
    pub fn new(content: Vec<Inline>, line: usize) -> Self {
        Self { content, colspan: 1, rowspan: 1, line }
    }
}

/// `<f-claim>` 定理/定义/引理…
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FluenClaim {
    pub id: Option<String>,
    pub ty: ClaimType,
    /// 正文块（Markdown 二次解析后的段落/列表等）。
    pub body: Vec<Block>,
    pub line: usize,
    /// 解析后：自动编号（按 type 独立计数）。
    pub resolved_number: Option<usize>,
}

// ── 便利：从 id 推断族（多处使用）────────
impl FluenClaim {
    /// 推断该 claim 的 id 族（基于 type）。
    pub fn id_kind(&self) -> IdKind {
        IdKind::Claim(self.ty)
    }
}
