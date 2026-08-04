//! 受控枚举与映射表：把规范 §3.1 的「id 前缀 ↔ claim type ↔ 显示标签」固化为类型，
//! 杜绝拼写歧义。所有映射集中在此一处，便于维护与扩展。

use std::fmt;

use crate::model::TableModel;

// ───────────────────────── Claim 类型 ─────────────────────────

/// `<f-claim type>` 的受控枚举（规范 §3.1）。
/// 取值集合是稳定契约：新增类型需同步更新 §3.1 表与本枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ClaimType {
    Theorem,
    Lemma,
    Definition,
    Proposition,
    Corollary,
    Example,
    Remark,
}

impl ClaimType {
    /// 从 `type=""` 属性原值解析；非法值返回 None（Linter 报错）。
    pub fn from_attr(raw: &str) -> Option<Self> {
        Some(match raw.trim() {
            "theorem" => Self::Theorem,
            "lemma" => Self::Lemma,
            "definition" => Self::Definition,
            "proposition" => Self::Proposition,
            "corollary" => Self::Corollary,
            "example" => Self::Example,
            "remark" => Self::Remark,
            _ => return None,
        })
    }

    /// 对应的 id 前缀（如 `thm:`）。
    pub const fn to_id_prefix(self) -> &'static str {
        match self {
            Self::Theorem => "thm:",
            Self::Lemma => "lem:",
            Self::Definition => "def:",
            Self::Proposition => "prop:",
            Self::Corollary => "cor:",
            Self::Example => "exa:",
            Self::Remark => "rem:",
        }
    }

    /// 对应的中文显示标签（如「定理」）。
    pub const fn label_zh(self) -> &'static str {
        match self {
            Self::Theorem => "定理",
            Self::Lemma => "引理",
            Self::Definition => "定义",
            Self::Proposition => "命题",
            Self::Corollary => "推论",
            Self::Example => "例",
            Self::Remark => "注",
        }
    }

    /// 对应的英文显示标签（如 "Theorem"）。
    pub const fn label_en(self) -> &'static str {
        match self {
            Self::Theorem => "Theorem",
            Self::Lemma => "Lemma",
            Self::Definition => "Definition",
            Self::Proposition => "Proposition",
            Self::Corollary => "Corollary",
            Self::Example => "Example",
            Self::Remark => "Remark",
        }
    }
}

impl fmt::Display for ClaimType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Theorem => "theorem",
            Self::Lemma => "lemma",
            Self::Definition => "definition",
            Self::Proposition => "proposition",
            Self::Corollary => "corollary",
            Self::Example => "example",
            Self::Remark => "remark",
        })
    }
}

// ───────────────────────── id 前缀族 ─────────────────────────

/// id 前缀族：用于编号流分组与交叉引用前缀文字生成（规范 §3.1 / §4.2）。
/// 一个 id 前缀唯一确定一个编号流。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum IdKind {
    Figure,    // fig:
    Table,     // tbl:
    Equation,  // eq:
    Section,   // sec:
    /// 定理类：内部按 ClaimType 进一步分子流。
    Claim(ClaimType),
}

impl IdKind {
    /// 由目标 id 推断其所属族（基于前缀）。未知前缀返回 None。
    pub fn from_id(id: &str) -> Option<Self> {
        if id.starts_with("fig:") {
            Some(Self::Figure)
        } else if id.starts_with("tbl:") {
            Some(Self::Table)
        } else if id.starts_with("eq:") {
            Some(Self::Equation)
        } else if id.starts_with("sec:") {
            Some(Self::Section)
        } else {
            // 定理类：遍历已知前缀
            ClaimType::VARIANTS
                .iter()
                .copied()
                .find(|&t| id.starts_with(t.to_id_prefix()))
                .map(Self::Claim)
        }
    }

    /// 是否属于定理类（用于"每种 type 独立计数"）。
    pub const fn is_claim(self) -> bool {
        matches!(self, Self::Claim(_))
    }
}

// 便于 IdKind::from_id 遍历的小工具：所有 ClaimType 的静态数组。
impl ClaimType {
    pub(crate) const VARIANTS: [ClaimType; 7] = [
        ClaimType::Theorem,
        ClaimType::Lemma,
        ClaimType::Definition,
        ClaimType::Proposition,
        ClaimType::Corollary,
        ClaimType::Example,
        ClaimType::Remark,
    ];
}

// ───────────────────────── 表格形态 ─────────────────────────

/// `<f-tbl>` 表体来源（规范 §5.3）。三种互斥。
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TableSource {
    /// 形态 A：外部数据源（CSV/JSON），相对 manuscript 根的路径。
    External { path: String, format: TableFormat },
    /// 形态 B：内嵌 Markdown 简单表（无合并）。
    Markdown(TableModel),
    /// 形态 C：内嵌 HTML `<table>` 子集（支持合并）。
    Html(TableModel),
}

/// 表格数据源文件格式（形态 A）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TableFormat {
    Csv,
    Json,
}

impl TableFormat {
    /// 由扩展名推断（小写）。未知返回 None。
    pub fn from_ext(ext: &str) -> Option<Self> {
        match ext.trim().trim_start_matches('.').to_ascii_lowercase().as_str() {
            "csv" => Some(Self::Csv),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}

/// 表格样式（规范 §5.3 三线表默认）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TableVariant {
    /// 三线表（默认，无需写出）。
    #[default]
    ThreeLine,
    /// 网格表。
    Grid,
}

impl TableVariant {
    pub fn from_attr(raw: &str) -> Option<Self> {
        match raw.trim() {
            "" | "threeline" => Some(Self::ThreeLine),
            "grid" => Some(Self::Grid),
            _ => None,
        }
    }
}
