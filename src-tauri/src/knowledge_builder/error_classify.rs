//! LLM 错误分类与重试策略（无状态）。
//!
//! V2.1 修正：放弃"任何异常销毁会话"的刚性策略，改为错误分类 + 分级处理。
//!
//! ## 错误分类
//!
//! - **瞬态错误**（[`LlmErrorKind::Transient`]）：429 / 503 / Timeout / Network
//!   - 保留会话（保护上下文缓存）
//!   - 指数退避重试，最多 [`MAX_TRANSIENT_RETRIES`] 次
//! - **结构性错误**（[`LlmErrorKind::Structural`]）：Context Overflow / Schema / Parse / Auth
//!   - 销毁会话（会话已不可用）
//!   - 不重试，标记 Failed
//! - **用户取消**：由 [`KnowledgeBuilderError::Cancelled`] 单独处理，保留会话
//!
//! ## 退避策略
//!
//! 1s → 2s → 4s，上限 [`MAX_RETRY_DELAY_MS`]（30s）。

use super::error::KnowledgeBuilderError;

/// 瞬态错误重试次数上限。
pub const MAX_TRANSIENT_RETRIES: u32 = 3;

/// 退避延迟上限（30 秒）。
pub const MAX_RETRY_DELAY_MS: u64 = 30_000;

/// 退避基数（1 秒）。
pub const BACKOFF_BASE_MS: u64 = 1_000;

/// LLM 错误分类——决定会话保留还是销毁。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmErrorKind {
    /// 瞬态错误：可重试，保留会话。
    ///
    /// `retry_after_ms` 为建议等待时间（来自 `Retry-After` 头或默认退避）。
    Transient { retry_after_ms: u64 },

    /// 结构性错误：不可重试，销毁会话。
    Structural(StructuralReason),
}

/// 结构性错误的具体原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructuralReason {
    /// 上下文超限（会话已不可用）。
    ContextOverflow,

    /// 请求格式错误（schema 不匹配、参数无效）。
    InvalidRequest,

    /// 响应解析失败（数据损坏、AI 输出无法解析）。
    ParseError,

    /// 认证失败（api_key 失效、权限不足）。
    AuthError,

    /// 其他不可恢复错误。
    Other,
}

/// 从错误信息分类。
///
/// 分类规则（基于错误字符串匹配，覆盖主流厂商错误文案）：
///
/// | 关键词 | 分类 |
/// |--------|------|
/// | `429` / `rate limit` / `too many requests` | Transient |
/// | `503` / `service unavailable` / `502` / `504` / `gateway` | Transient |
/// | `timeout` / `timed out` / `connection` / `network` / `econnreset` | Transient |
/// | `context length` / `context window` / `too long` / `maximum context` | Structural(ContextOverflow) |
/// | `401` / `unauthorized` / `auth` / `api key` / `forbidden` / `403` | Structural(AuthError) |
/// | `400` / `bad request` / `invalid` / `schema` | Structural(InvalidRequest) |
/// | `parse` / `deserialize` / `json` / `未调用` | Structural(ParseError) |
/// | 其他 | Structural(Other) |
pub fn classify_error(err: &KnowledgeBuilderError) -> LlmErrorKind {
    match err {
        // 用户取消：单独路径，不进入此函数。但若误入，按瞬态处理（保留会话）。
        KnowledgeBuilderError::Cancelled => LlmErrorKind::Transient { retry_after_ms: 0 },

        // AI 输出解析失败：结构性（数据已损坏）
        KnowledgeBuilderError::AiOutput(_) => {
            LlmErrorKind::Structural(StructuralReason::ParseError)
        }

        // 状态机错误：结构性（不可恢复）
        KnowledgeBuilderError::StateMachine(_) => {
            LlmErrorKind::Structural(StructuralReason::Other)
        }

        // 配置错误：结构性（认证/配置问题）
        KnowledgeBuilderError::Config(_) => {
            LlmErrorKind::Structural(StructuralReason::AuthError)
        }

        // 文献缺失：结构性（数据问题，重试无用）
        KnowledgeBuilderError::MdNotFound(_) => {
            LlmErrorKind::Structural(StructuralReason::Other)
        }

        // I/O 错误：通常为瞬态（文件锁、临时不可访问）
        KnowledgeBuilderError::Io(_) => LlmErrorKind::Transient { retry_after_ms: 0 },

        // 知识库底层错误（fluen-kb）：按消息细分
        KnowledgeBuilderError::Kb(e) => classify_by_message(&e.to_string()),

        // LLM 调用错误：按消息细分
        KnowledgeBuilderError::Llm(msg) => classify_by_message(msg),

        // JSON 解析错误：结构性（数据损坏）
        KnowledgeBuilderError::Parse(_) => {
            LlmErrorKind::Structural(StructuralReason::ParseError)
        }

        // 任务队列错误：结构性
        KnowledgeBuilderError::TaskQueue(_) => {
            LlmErrorKind::Structural(StructuralReason::Other)
        }
    }
}

/// 基于错误消息字符串分类。
///
/// 提取为内部函数，便于 [`KnowledgeError`] 与 [`Llm`] 错误复用。
fn classify_by_message(msg: &str) -> LlmErrorKind {
    let lower = msg.to_lowercase();

    // 瞬态：限流 / 服务不可用 / 超时 / 网络
    if contains_any(
        &lower,
        &[
            "429",
            "rate limit",
            "too many requests",
            "503",
            "service unavailable",
            "502",
            "504",
            "gateway",
            "timeout",
            "timed out",
            "connection",
            "network",
            "econnreset",
            "connection reset",
            "connection refused",
        ],
    ) {
        return LlmErrorKind::Transient { retry_after_ms: 0 };
    }

    // 结构性：上下文超限
    if contains_any(
        &lower,
        &[
            "context length",
            "context window",
            "too long",
            "maximum context",
            "context_overflow",
            "token limit",
            "max_tokens",
        ],
    ) {
        return LlmErrorKind::Structural(StructuralReason::ContextOverflow);
    }

    // 结构性：认证失败
    if contains_any(
        &lower,
        &[
            "401",
            "unauthorized",
            "auth",
            "api key",
            "api_key",
            "forbidden",
            "403",
            "permission",
        ],
    ) {
        return LlmErrorKind::Structural(StructuralReason::AuthError);
    }

    // 结构性：请求格式错误
    if contains_any(
        &lower,
        &[
            "400",
            "bad request",
            "invalid",
            "schema",
            "invalid_request",
            "invalid_request_error",
        ],
    ) {
        return LlmErrorKind::Structural(StructuralReason::InvalidRequest);
    }

    // 结构性：解析错误
    if contains_any(
        &lower,
        &[
            "parse",
            "deserialize",
            "json",
            "未调用",
            "未提交",
            "tool_call",
        ],
    ) {
        return LlmErrorKind::Structural(StructuralReason::ParseError);
    }

    // 默认：结构性（其他不可恢复）
    LlmErrorKind::Structural(StructuralReason::Other)
}

/// 检查 `text` 是否包含 `keywords` 中任一词（大小写不敏感）。
fn contains_any(text: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|k| text.contains(k))
}

/// 计算指数退避延迟。
///
/// 第 1 次重试（attempt=0）→ 1s，第 2 次 → 2s，第 3 次 → 4s，上限 30s。
pub fn backoff_delay(attempt: u32) -> u64 {
    if attempt == 0 {
        return BACKOFF_BASE_MS;
    }
    let delay = BACKOFF_BASE_MS.saturating_mul(2u64.saturating_pow(attempt));
    delay.min(MAX_RETRY_DELAY_MS)
}

/// 计算实际等待时间：取 `backoff_delay` 与 `retry_after_ms` 的较大值。
///
/// `retry_after_ms` 来自厂商的 `Retry-After` 头，应优先尊重。
pub fn effective_delay(attempt: u32, retry_after_ms: u64) -> u64 {
    backoff_delay(attempt).max(retry_after_ms)
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn classify_rate_limit_as_transient() {
        let err = KnowledgeBuilderError::Llm("Rate limit exceeded (429)".into());
        assert_eq!(
            classify_error(&err),
            LlmErrorKind::Transient { retry_after_ms: 0 }
        );
    }

    #[test]
    fn classify_503_as_transient() {
        let err = KnowledgeBuilderError::Llm("Service Unavailable (503)".into());
        assert_eq!(
            classify_error(&err),
            LlmErrorKind::Transient { retry_after_ms: 0 }
        );
    }

    #[test]
    fn classify_timeout_as_transient() {
        let err = KnowledgeBuilderError::Llm("Request timed out".into());
        assert_eq!(
            classify_error(&err),
            LlmErrorKind::Transient { retry_after_ms: 0 }
        );
    }

    #[test]
    fn classify_network_error_as_transient() {
        let err = KnowledgeBuilderError::Llm("connection reset by peer".into());
        assert_eq!(
            classify_error(&err),
            LlmErrorKind::Transient { retry_after_ms: 0 }
        );
    }

    #[test]
    fn classify_context_overflow_as_structural() {
        let err = KnowledgeBuilderError::Llm("context length exceeded".into());
        assert_eq!(
            classify_error(&err),
            LlmErrorKind::Structural(StructuralReason::ContextOverflow)
        );
    }

    #[test]
    fn classify_auth_error_as_structural() {
        let err = KnowledgeBuilderError::Llm("401 Unauthorized: invalid api key".into());
        assert_eq!(
            classify_error(&err),
            LlmErrorKind::Structural(StructuralReason::AuthError)
        );
    }

    #[test]
    fn classify_invalid_request_as_structural() {
        let err = KnowledgeBuilderError::Llm("400 Bad Request: invalid schema".into());
        let kind = classify_error(&err);
        // 同时含 invalid 与 schema，应分类为 InvalidRequest 或 AuthError 之一（按检查顺序）
        assert!(matches!(
            kind,
            LlmErrorKind::Structural(StructuralReason::InvalidRequest)
                | LlmErrorKind::Structural(StructuralReason::AuthError)
        ));
    }

    #[test]
    fn classify_ai_output_as_parse_error() {
        let err = KnowledgeBuilderError::AiOutput("AI 未调用 submit_plan".into());
        assert_eq!(
            classify_error(&err),
            LlmErrorKind::Structural(StructuralReason::ParseError)
        );
    }

    #[test]
    fn classify_io_as_transient() {
        let err = KnowledgeBuilderError::Io(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "interrupted",
        ));
        assert_eq!(
            classify_error(&err),
            LlmErrorKind::Transient { retry_after_ms: 0 }
        );
    }

    #[test]
    fn classify_md_not_found_as_structural() {
        let err = KnowledgeBuilderError::MdNotFound(PathBuf::from("/tmp/missing.md"));
        assert_eq!(
            classify_error(&err),
            LlmErrorKind::Structural(StructuralReason::Other)
        );
    }

    #[test]
    fn classify_unknown_llm_error_as_structural_other() {
        let err = KnowledgeBuilderError::Llm("some weird unknown error".into());
        assert_eq!(
            classify_error(&err),
            LlmErrorKind::Structural(StructuralReason::Other)
        );
    }

    #[test]
    fn backoff_delay_sequence() {
        assert_eq!(backoff_delay(0), 1_000);
        assert_eq!(backoff_delay(1), 2_000);
        assert_eq!(backoff_delay(2), 4_000);
        assert_eq!(backoff_delay(3), 8_000);
        // 上限 30s
        assert_eq!(backoff_delay(10), MAX_RETRY_DELAY_MS);
    }

    #[test]
    fn effective_delay_takes_max() {
        // backoff=1000, retry_after=5000 → 5000
        assert_eq!(effective_delay(0, 5_000), 5_000);
        // backoff=4000, retry_after=1000 → 4000
        assert_eq!(effective_delay(2, 1_000), 4_000);
    }
}
