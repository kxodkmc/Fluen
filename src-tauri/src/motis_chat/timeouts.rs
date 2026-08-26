//! # Motis 域超时分层——单一事实来源
//!
//! Motis（总督）与子智能体链路上的全部超时常量集中于此，
//! 保证各层超时**单调递增、语义清晰**，避免上游先于下游切断。
//!
//! ## 分层链（每层必须大于其内部所有等待的上界）
//!
//! ```text
//! 审批等待 300s            （motis_chat/approval.rs，写工具前的用户确认）
//!   < 子代理工具 360s        （审批 300s + 执行余量）
//!   ≤ 子代理批次 540s        （awaiting_calls_timeout：单轮等待类工具批次总 deadline，
//!                             ≥ 单工具上界、< 委派 RPC，保证慢批次在预算内降级）
//!   < 引擎单轮 LLM 300s      （thinking_timeout，主/子引擎每轮生成上限）
//!   < LLM HTTP 360s          （请求层兜底，引擎超时先触发并可走引擎重试）
//!   < 委派 RPC 600s          （一次委派的总预算：多轮 LLM + 多轮工具）
//!   < Motis 工具/批次 660s   （执行器包裹 delegate_agent，须大于 RPC；
//!                             批次 deadline = 单工具上界，仅约束多工具批次）
//! ```
//!
//! 引擎默认 `thinking_timeout = 30s`、`awaiting_calls_timeout = 60s`
//! （为轻量对话设计），学术写作的长文本与审批型长工具远超该值，
//! 故 Motis 域统一经 [`motis_engine_config`] 放宽。

use std::time::Duration;

use referee_ai::engine::EngineConfig;
use referee_ai::session::{SessionConfig, TimeoutConfig};

/// 引擎单轮 LLM 生成上限（thinking_timeout 覆盖值）。
///
/// 学术写作单轮输出常达数千 token，referee 默认 30 秒会频繁
/// 切断为 "turn timeout"；取 5 分钟并低于 HTTP 兜底，
/// 让引擎层超时先于请求层触发（可走引擎重试）。
pub const LLM_TURN_TIMEOUT: Duration = Duration::from_secs(300);

/// Motis 域 LLM HTTP 请求超时（兜底，应大于 [`LLM_TURN_TIMEOUT`]）。
pub const LLM_HTTP_TIMEOUT: Duration = Duration::from_secs(360);

/// 子智能体运行时的单工具执行超时（审批等待 300s + 执行余量）。
pub const SUBAGENT_TOOL_TIMEOUT: Duration = crate::agent_runtime::approval::APPROVAL_EXECUTOR_TIMEOUT;

/// Motis 总督运行时的单工具执行超时。
///
/// 必须大于 [`DELEGATE_RPC_TIMEOUT_MS`]（600s）：`delegate_agent` 被
/// 执行器超时包裹，若执行器先切断会吞掉 RPC 的明确超时错误，
/// 且委派 future 被 drop 导致子会话追踪泄漏。
pub const MOTIS_TOOL_TIMEOUT: Duration = Duration::from_secs(660);

/// 委派 RPC 超时（毫秒）——一次子智能体委派的总预算。
///
/// 论文写作含多轮工具调用与最长 5 分钟审批等待，
/// referee 默认的 30 秒远不够，放宽至 10 分钟。
pub const DELEGATE_RPC_TIMEOUT_MS: u64 = 600_000;

/// 子智能体引擎的等待类工具批次总 deadline（`awaiting_calls_timeout`）。
///
/// referee 0.4 起该配置实际生效：约束单轮等待类工具批次的总时长，
/// 超时项收敛、会话恢复一致状态。取值须 ≥ 单工具上界
/// [`SUBAGENT_TOOL_TIMEOUT`]（360s），且 < [`DELEGATE_RPC_TIMEOUT_MS`]
/// （600s），保证慢批次在委派预算内先降级（留 60s 回信余量）。
pub const SUBAGENT_AWAITING_TIMEOUT: Duration = Duration::from_secs(540);

/// Motis 总督引擎的等待类工具批次总 deadline。
///
/// 取 [`MOTIS_TOOL_TIMEOUT`]（660s）：与单工具执行器超时持平，
/// 单工具行为不变，仅对「多工具慢批次」新增总上限
/// （主会话无外层 RPC 预算，无需再留回信余量）。
pub const MOTIS_AWAITING_TIMEOUT: Duration = MOTIS_TOOL_TIMEOUT;

/// 构建 Motis 域引擎配置——放宽单轮 LLM 超时与等待类批次 deadline。
///
/// Motis 总督与全部子智能体共用（见 `runtime.rs` / `agents.rs`），
/// 仅 `awaiting_calls_timeout` 按域取值不同。
pub fn motis_engine_config(awaiting_calls_timeout: Duration) -> EngineConfig {
    EngineConfig {
        session: SessionConfig {
            timeout: TimeoutConfig {
                thinking_timeout: LLM_TURN_TIMEOUT,
                awaiting_calls_timeout,
                ..TimeoutConfig::default()
            },
            ..SessionConfig::default()
        },
        ..EngineConfig::default()
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn awaiting_timeouts_respect_layering() {
        // 子代理：单工具 ≤ 批次 deadline < 委派 RPC（留回信余量）
        assert!(SUBAGENT_AWAITING_TIMEOUT >= SUBAGENT_TOOL_TIMEOUT);
        assert!((SUBAGENT_AWAITING_TIMEOUT.as_millis() as u64) < DELEGATE_RPC_TIMEOUT_MS);
        // 主会话：批次 deadline = 单工具上界，仅约束多工具批次
        assert_eq!(MOTIS_AWAITING_TIMEOUT, MOTIS_TOOL_TIMEOUT);
    }
}
