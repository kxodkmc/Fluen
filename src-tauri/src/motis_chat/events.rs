//! Motis 聊天的 Tauri 事件定义。
//!
//! 定义事件名常量与每个事件 payload 的序列化结构，
//! 供前端通过 `@tauri-apps/api/event` 的 `listen` 监听。
//!
//! ## 事件流
//!
//! ```text
//! motis:thought   →  思考增量（可选，依配置展示）
//! motis:text       →  文本增量（主输出流）
//! motis:tool-call  →  工具调用通知
//! motis:finish     →  完成事件（携带最终结果与 token 用量）
//! motis:error      →  错误事件（流终止时发送）
//! ```

use serde::Serialize;

// ===== 事件名常量 =====

/// 思考增量事件——模型推理过程的增量文本。
pub const EVENT_THOUGHT: &str = "motis:thought";

/// 文本增量事件——模型输出的增量文本。
pub const EVENT_TEXT: &str = "motis:text";

/// 工具调用事件——模型发起工具调用的通知。
pub const EVENT_TOOL_CALL: &str = "motis:tool-call";

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
