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
