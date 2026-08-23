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
//!   < 引擎单轮 LLM 300s      （thinking_timeout，主/子引擎每轮生成上限）
//!   < LLM HTTP 360s          （请求层兜底，引擎超时先触发并可走引擎重试）
//!   < 委派 RPC 600s          （一次委派的总预算：多轮 LLM + 多轮工具）
//!   < Motis 工具 660s        （执行器包裹 delegate_agent，须大于 RPC）
//! ```
//!
//! 引擎默认 `thinking_timeout = 30s`（为轻量对话设计），学术写作的
//! 长文本生成远超该值，故 Motis 域统一经 [`motis_engine_config`] 放宽。

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

/// 构建 Motis 域引擎配置——仅放宽单轮 LLM 超时，其余保持默认。
///
/// Motis 总督与全部子智能体共用（见 `runtime.rs` / `agents.rs`）。
pub fn motis_engine_config() -> EngineConfig {
    EngineConfig {
        session: SessionConfig {
            timeout: TimeoutConfig {
                thinking_timeout: LLM_TURN_TIMEOUT,
                ..TimeoutConfig::default()
            },
            ..SessionConfig::default()
        },
        ..EngineConfig::default()
    }
}
