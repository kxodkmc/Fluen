//! 错误类型。单一枚举贯穿解析、校验、渲染全流程，对外统一 [`MarkupError`]。

use std::fmt;
use std::io;

use crate::kinds::ClaimType;

/// Fluen 解析器统一错误。
///
/// - [`Self::Parse`]   解析阶段：标签结构、属性、自闭合等语法问题。
/// - [`Self::Lint`]    校验阶段：§6.2 / §8.5 中"必须报错"的语义问题。
/// - [`Self::Resolve`] 解析阶段：编号 / 交叉引用无法处理的硬错误（如 id 重复）。
/// - [`Self::Render`]  渲染阶段：后端不支持或上下文不完整。
/// - [`Self::Io`]      文件读写。
#[derive(Debug)]
pub enum MarkupError {
    /// 语法/结构错误。`span` 为 1-based 起止行（end≥start，未知时 end=start）。
    Parse {
        message: String,
        line: usize,
    },
    /// Linter 强制规则违反（§6.2）。`severity` 区分 Error / Warning。
    Lint {
        message: String,
        line: usize,
        severity: Severity,
    },
    /// 编号 / 引用解析硬错误（如重复 id）。
    Resolve { message: String },
    /// 渲染错误。
    Render { message: String },
    /// 文件 IO 错误。
    Io(io::Error),
}

/// 校验严重程度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    /// 警告：不影响输出，但需提示作者（如降级触发、严格模式不一致）。
    Warning,
    /// 错误：违反强制规则，调用方可据此中止流程。
    Error,
}

impl MarkupError {
    /// 构造解析错误（便捷构造器）。
    pub fn parse(message: impl Into<String>, line: usize) -> Self {
        Self::Parse { message: message.into(), line }
    }

    /// 构造 Lint 错误（便捷构造器）。
    pub fn lint(message: impl Into<String>, line: usize, severity: Severity) -> Self {
        Self::Lint { message: message.into(), line, severity }
    }

    /// 是否为可降级的"软"问题（Warning），便于调用方区分硬错误与提示。
    pub fn is_warning(&self) -> bool {
        matches!(self, Self::Lint { severity: Severity::Warning, .. })
    }
}

impl fmt::Display for MarkupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse { message, line } => write!(f, "parse error (line {line}): {message}"),
            Self::Lint { message, line, severity } => {
                let tag = match severity {
                    Severity::Warning => "lint warning",
                    Severity::Error => "lint error",
                };
                write!(f, "{tag} (line {line}): {message}")
            }
            Self::Resolve { message } => write!(f, "resolve error: {message}"),
            Self::Render { message } => write!(f, "render error: {message}"),
            Self::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for MarkupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for MarkupError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// 校验产生的多条问题。空表示通过；非空时调用方可遍历逐条处理。
/// 注意：降级（fallback）触发的 Warning 已记录在此，但渲染仍可继续。
pub type Problems = Vec<MarkupError>;

/// 模块统一 Result 别名。
pub type Result<T, E = MarkupError> = std::result::Result<T, E>;

// ── ClaimType 在 kinds.rs 之后定义；这里仅借用其 Display，避免循环依赖 ──
// 说明：ClaimType 的 to_id_prefix / to_label 见 kinds.rs。

/// 给 Linter 构造类型-前缀不一致错误时使用（集中文案）。
pub(crate) fn claim_prefix_mismatch(ty: &ClaimType, id: &str) -> String {
    format!(
        "<f-claim type=\"{ty}\"> 的 id `{id}` 应以前缀 `{}` 开头（见规范 §3.1）",
        ty.to_id_prefix()
    )
}
