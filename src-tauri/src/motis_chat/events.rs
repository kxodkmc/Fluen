//! Motis 聊天的 Tauri 事件定义。
//!
//! 定义事件名常量与每个事件 payload 的序列化结构，
//! 供前端通过 `@tauri-apps/api/event` 的 `listen` 监听。
//!
//! ## 事件流
//!
//! ```text
//! motis:thought          →  思考增量（可选，依配置展示）
//! motis:text             →  文本增量（主输出流）
//! motis:tool-call        →  工具调用通知
//! motis:tool-result      →  工具执行结果（当前用于 delegate_agent 委派结果）
//! motis:approval-request →  工具审批请求（写操作需用户确认）
//! motis:finish           →  完成事件（携带最终结果与 token 用量）
//! motis:error            →  错误事件（流终止时发送）
//! ```
//!
//! ## 子智能体事件（委派过程可观测）
//!
//! ```text
//! motis:agent-started      →  委派发起（目标子智能体 + 任务 + 超时预算）
//! motis:agent-thought      →  子智能体 LLM 思考增量（依配置收集）
//! motis:agent-text         →  子智能体 LLM 输出增量（实时写作内容）
//! motis:agent-tool-call    →  子智能体内部工具调用开始
//! motis:agent-tool-result  →  子智能体内部工具调用结束（含结果/耗时）
//! motis:agent-finished     →  委派结束（成功带 token，失败带原因与耗时）
//! ```
//!
//! 子智能体事件均携带父级 `delegate_agent` 的 `tool_call_id`，
//! 前端据此关联到对应工具调用消息（见 `agent_reporter`）。

use serde::Serialize;

// ===== 事件名常量 =====

/// 思考增量事件——模型推理过程的增量文本。
pub const EVENT_THOUGHT: &str = "motis:thought";

/// 文本增量事件——模型输出的增量文本。
pub const EVENT_TEXT: &str = "motis:text";

/// 工具调用事件——模型发起工具调用的通知。
pub const EVENT_TOOL_CALL: &str = "motis:tool-call";

/// 工具执行结果事件——工具返回的结果（当前覆盖 delegate_agent 委派结果）。
pub const EVENT_TOOL_RESULT: &str = "motis:tool-result";

/// 工具审批请求事件——声明 OnExecute 的工具（如写操作）调用前征求用户确认。
pub const EVENT_APPROVAL_REQUEST: &str = "motis:approval-request";

/// 完成事件——认知循环结束，携带最终结果与 token 用量。
pub const EVENT_FINISH: &str = "motis:finish";

/// 错误事件——流式执行中出现错误。
pub const EVENT_ERROR: &str = "motis:error";

/// 子智能体委派发起事件——目标子智能体开始执行任务。
pub const EVENT_AGENT_STARTED: &str = "motis:agent-started";

/// 子智能体思考增量事件——子智能体 LLM 推理过程的增量文本
/// （引擎 `EngineObserver` 钩子透传，仅委派子会话产生）。
pub const EVENT_AGENT_THOUGHT: &str = "motis:agent-thought";

/// 子智能体文本增量事件——子智能体最终输出的增量文本（实时写作内容）。
pub const EVENT_AGENT_TEXT: &str = "motis:agent-text";

/// 子智能体内部工具调用开始事件。
pub const EVENT_AGENT_TOOL_CALL: &str = "motis:agent-tool-call";

/// 子智能体内部工具调用结束事件——携带结果与耗时。
pub const EVENT_AGENT_TOOL_RESULT: &str = "motis:agent-tool-result";

/// 子智能体委派结束事件——成功带 token 用量，失败带原因，均带耗时。
pub const EVENT_AGENT_FINISHED: &str = "motis:agent-finished";

/// 论文正文变更事件——AI 经 `manuscript` 工具写入正文成功后通知
/// 前端重新拉取项目内容（编辑器 / 预览 / 大纲自动跟随）。
pub const EVENT_PROJECT_UPDATED: &str = "motis:project-updated";

// ===== 事件 payload =====

/// 思考增量 payload。
#[derive(Debug, Clone, Serialize)]
pub struct ThoughtPayload {
    /// 思考内容增量。
    pub delta: String,
}

/// 文本增量 payload。
#[derive(Debug, Clone, Serialize)]
pub struct TextPayload {
    /// 文本内容增量。
    pub delta: String,
}

/// 工具调用 payload。
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallPayload {
    /// 工具调用 ID。
    pub id: String,
    /// 工具名称。
    pub name: String,
    /// 工具调用参数（JSON）。
    pub input: serde_json::Value,
}

/// 工具执行结果 payload。
///
/// 通过 `tool_call_id` 与前端已有的 tool_call 消息关联。
#[derive(Debug, Clone, Serialize)]
pub struct ToolResultPayload {
    /// 对应工具调用的 ID（与 tool_call 事件的 id 一致）。
    pub tool_call_id: String,
    /// 工具名称。
    pub name: String,
    /// 工具执行结果。
    pub result: serde_json::Value,
}

/// 工具审批请求 payload。
///
/// 前端展示后通过 `motis_chat_resolve_approval` 命令回传决策。
#[derive(Debug, Clone, Serialize)]
pub struct ApprovalRequestPayload {
    /// 审批请求 ID（唯一，用于回传决策）。
    pub id: String,
    /// 工具名称。
    pub tool_name: String,
    /// 工具调用参数（JSON，含 path / action / content 等）。
    pub input: serde_json::Value,
}

/// 完成事件 payload。
#[derive(Debug, Clone, Serialize)]
pub struct FinishPayload {
    /// 最终结果。
    pub result: serde_json::Value,
    /// 总 token 用量。
    pub total_tokens: usize,
}

/// 错误事件 payload。
#[derive(Debug, Clone, Serialize)]
pub struct ErrorPayload {
    /// 错误信息。
    pub message: String,
}

// ===== 子智能体事件 payload =====

/// 子智能体委派发起 payload。
#[derive(Debug, Clone, Serialize)]
pub struct AgentStartedPayload {
    /// 父级 `delegate_agent` 工具调用 ID（与 tool-call 事件的 id 一致）。
    pub tool_call_id: String,
    /// 目标子智能体 ID（如 `essay_writing`）。
    pub agent_id: String,
    /// 派发的任务描述。
    pub task: String,
    /// 委派超时预算（毫秒）。
    pub timeout_ms: u64,
}

/// 子智能体输出增量 payload（思考 / 文本增量事件共用）。
#[derive(Debug, Clone, Serialize)]
pub struct AgentDeltaPayload {
    /// 父级 `delegate_agent` 工具调用 ID（与 agent-started 事件的 id 一致）。
    pub tool_call_id: String,
    /// 子智能体 ID。
    pub agent_id: String,
    /// 本帧增量文本。
    pub delta: String,
}

/// 子智能体内部工具调用 payload。
#[derive(Debug, Clone, Serialize)]
pub struct AgentToolCallPayload {
    /// 父级 `delegate_agent` 工具调用 ID。
    pub tool_call_id: String,
    /// 子智能体 ID。
    pub agent_id: String,
    /// 子智能体会话内的工具调用 ID（与 tool-result 事件关联）。
    pub id: String,
    /// 工具名称。
    pub name: String,
    /// 工具调用参数（JSON）。
    pub input: serde_json::Value,
}

/// 子智能体内部工具调用结果 payload。
#[derive(Debug, Clone, Serialize)]
pub struct AgentToolResultPayload {
    /// 父级 `delegate_agent` 工具调用 ID。
    pub tool_call_id: String,
    /// 子智能体 ID。
    pub agent_id: String,
    /// 子智能体会话内的工具调用 ID（与 tool-call 事件关联）。
    pub id: String,
    /// 工具名称。
    pub name: String,
    /// 是否成功。
    pub ok: bool,
    /// 执行耗时（毫秒）。
    pub duration_ms: u64,
    /// 工具输出（文本或错误信息，超长已截断）。
    pub result: serde_json::Value,
}

/// 子智能体委派结束 payload。
#[derive(Debug, Clone, Serialize)]
pub struct AgentFinishedPayload {
    /// 父级 `delegate_agent` 工具调用 ID。
    pub tool_call_id: String,
    /// 子智能体 ID。
    pub agent_id: String,
    /// 是否成功完成。
    pub ok: bool,
    /// 委派总耗时（毫秒）。
    pub duration_ms: u64,
    /// 成功时的 token 用量（失败/超时为 `None`）。
    pub tokens_used: Option<usize>,
    /// 失败原因（成功为 `None`）。
    pub error: Option<String>,
}
