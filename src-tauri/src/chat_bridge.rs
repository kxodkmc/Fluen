//! 聊天流式桥接——referee [`FluenRuntime`] 流式会话 → Tauri 前端事件。
//!
//! Motis 聊天与学术助手共用的流式适配层，职责：
//!
//! 1. [`start_chat_session`]：回放前端历史（`restore_session_history`）+ 启动流式回合
//!    （`chat_stream`），返回 [`ChatHandle`] 供调用方注册取消
//! 2. [`consume_stream`]：消费 `StreamChunk` 流并映射为 Tauri 事件
//!    （思考增量 / 文本增量 / 工具调用 / 完成 / 错误）
//!
//! ## 事件映射
//!
//! | StreamChunk | Tauri 事件 |
//! |-------------|-----------|
//! | `Delta { reasoning_content }` | `{prefix}:thought` |
//! | `Delta { content }` | `{prefix}:text` |
//! | `Delta { tool_calls }`（参数累积完整时） | `{prefix}:tool-call` |
//! | 流正常结束（记录到 Finish usage） | `{prefix}:finish`（恰好一次） |
//! | 流错误 | `{prefix}:error` |
//!
//! 工具调用事件：referee 的 `ToolCallDelta` 是分片增量（index 定位、
//! arguments 逐段拼接），桥接层累积并在参数可解析为完整 JSON 时 emit 一次，
//! 流结束时冲刷未发送的（兜底）。
//!
//! `finish` 事件保证**恰好一次**：多轮工具调用会产生多次 `Finish` chunk，
//! 仅在流结束时携带最后一次的 usage emit。

use std::collections::HashMap;

use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Window};

use crate::agent_runtime::FluenRuntime;

use referee_ai::engine::ChatHandle;
use referee_ai::provider::{Message, Role, StreamChunk, ThinkingConfig, TokenUsage};
use referee_ai::session::{ChatOptions, ChatPayload, SessionId};

/// 前端历史消息（`motis_chat_send` / `ai_assistant_send` 的 `history` 参数）。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HistoryMessage {
    /// 消息角色（`"user"` 或 `"assistant"`）。
    pub role: String,
    /// 消息内容。
    pub content: String,
}

/// 一组前缀化的聊天事件名（如 `motis:*` / `ai-assistant:*`）。
#[derive(Debug, Clone, Copy)]
pub struct ChatEvents {
    /// 思考增量事件。
    pub thought: &'static str,
    /// 文本增量事件。
    pub text: &'static str,
    /// 工具调用事件。
    pub tool_call: &'static str,
    /// 完成事件。
    pub finish: &'static str,
    /// 错误事件。
    pub error: &'static str,
}

/// 事件 payload（与前端契约一致）。
mod payload {
    use referee_ai::provider::TokenUsage;
    use serde::Serialize;

    #[derive(Serialize, Clone)]
    pub struct Thought<'a> {
        pub delta: &'a str,
    }

    #[derive(Serialize, Clone)]
    pub struct Text<'a> {
        pub delta: &'a str,
    }

    #[derive(Serialize, Clone)]
    pub struct ToolCall {
        pub id: String,
        pub name: String,
        pub input: serde_json::Value,
    }

    #[derive(Serialize, Clone)]
    pub struct Finish {
        pub result: serde_json::Value,
        pub total_tokens: usize,
        /// 真实输入 token（vendor 上报 usage 时才有；OpenAI 兼容流式
        /// 需厂商支持，不支持时为 None）。
        pub prompt_tokens: Option<usize>,
        /// 真实输出 token（同上）。
        pub completion_tokens: Option<usize>,
    }

    impl Finish {
        /// 从回合末次 usage 构造（无 usage 时三个计数均为缺省）。
        pub fn from_usage(result: serde_json::Value, usage: Option<&TokenUsage>) -> Self {
            Self {
                result,
                total_tokens: usage.map(|u| u.total_tokens).unwrap_or(0),
                prompt_tokens: usage.map(|u| u.prompt_tokens),
                completion_tokens: usage.map(|u| u.completion_tokens),
            }
        }
    }

    #[derive(Serialize, Clone)]
    pub struct Error {
        pub message: String,
    }

    impl Error {
        pub fn new(message: impl Into<String>) -> Self {
            Self {
                message: message.into(),
            }
        }
    }
}

/// 将前端历史消息转换为 referee [`Message`]。
pub fn to_referee_messages(history: &[HistoryMessage]) -> Vec<Message> {
    history
        .iter()
        .map(|msg| Message {
            role: match msg.role.as_str() {
                "assistant" => Role::Assistant,
                _ => Role::User,
            },
            content: msg.content.clone().into(),
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
            usage: None,
        })
        .collect()
}

/// 启动一轮流式会话：回放历史 + 发起 `chat_stream`。
///
/// 返回 [`ChatHandle`]——调用方应立即注册到取消表，随后调用
/// [`consume_stream`] 消费。失败时返回用户可读的错误消息。
pub fn start_chat_session(
    runtime: &FluenRuntime,
    session_id: SessionId,
    history: Vec<Message>,
    message: String,
    system_prompt: String,
    thinking_enabled: bool,
) -> Result<ChatHandle, String> {
    // 回放前端传入的历史（前端是对话的事实源，每轮重建会话）
    if !history.is_empty() {
        runtime
            .restore_session_history(session_id, history)
            .map_err(|e| format!("会话历史恢复失败: {e}"))?;
    }

    let payload = ChatPayload {
        message: Message::user(message),
        options: ChatOptions {
            system_prompt: Some(system_prompt),
            thinking: ThinkingConfig {
                enabled: thinking_enabled,
                effort: None,
            },
            ..ChatOptions::default()
        },
        peer_depth: 0,
    };

    runtime
        .chat_stream(session_id, payload)
        .map_err(|e| format!("会话启动失败: {e}"))
}

/// 消费流式回复并 emit 到前端。
///
/// 阻塞至回合结束（完成 / 错误 / 取消）。`finish` 恰好 emit 一次
/// （流正常结束时）；错误时 emit `error` 且不再 emit `finish`；
/// 用户取消（流静默结束）不 emit 任何终止事件。
pub async fn consume_stream(handle: ChatHandle, window: &Window, events: &ChatEvents) {
    let reply = match handle.wait().await {
        Some(reply) => reply,
        None => return, // 回合已结束（如取消），无需处理
    };

    use referee_ai::engine::EngineReply;
    let stream = match reply {
        EngineReply::Streaming(stream) => stream,
        EngineReply::Error(err) => {
            emit_error(window, events, err.to_string());
            return;
        }
        EngineReply::Busy { .. } => {
            emit_error(window, events, "会话忙碌，请等待当前回合结束");
            return;
        }
        EngineReply::Cancelled => return,
        EngineReply::Timeout => {
            emit_error(window, events, "会话超时");
            return;
        }
        EngineReply::Success(resp) => {
            // chat_stream 正常不返回 Success；稳健处理
            let _ = window.emit(
                events.finish,
                payload::Finish::from_usage(
                    serde_json::Value::String(
                        resp.message.content.as_text().unwrap_or_default().to_string(),
                    ),
                    resp.usage.as_ref(),
                ),
            );
            return;
        }
    };

    let mut stream = stream;
    let mut acc = ToolCallAccumulator::default();
    let mut full_text = String::new();
    let mut last_usage: Option<TokenUsage> = None;
    let mut errored = false;

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(StreamChunk::Delta {
                content,
                reasoning_content,
                tool_calls,
                ..
            }) => {
                // 空 delta（如工具调用轮次的角色帧 content:""）不 emit，
                // 避免前端创建空文本消息、隔断活动时间线分组
                if let Some(text) = content.filter(|t| !t.is_empty()) {
                    full_text.push_str(&text);
                    let _ = window.emit(events.text, payload::Text { delta: &text });
                }
                if let Some(thought) = reasoning_content.filter(|t| !t.is_empty()) {
                    let _ = window.emit(events.thought, payload::Thought { delta: &thought });
                }
                if !tool_calls.is_empty() {
                    acc.merge(tool_calls);
                    acc.flush_ready(window, events);
                }
            }
            Ok(StreamChunk::Finish { usage, .. }) => {
                // 回合边界：tool_calls 的 index 每回合从 0 重计，累积器跨回合复用
                // 会把后续回合的调用并入已发送条目而被静默吞掉（委派/读板等
                // 第二回合之后的工具行从时间线消失）。先冲刷本回合残余再重置。
                acc.flush_all(window, events);
                acc = ToolCallAccumulator::default();
                last_usage = usage;
            }
            Err(e) => {
                tracing::error!(
                    target = "fluen_chat_bridge",
                    error = %e,
                    "聊天流式错误（LLM/上游）"
                );
                emit_error(window, events, e.to_string());
                errored = true;
                break;
            }
        }
    }

    if errored {
        return;
    }
    // 用户取消：无 usage 且无文本增量时静默结束
    if last_usage.is_none() && full_text.is_empty() && acc.pending_is_empty() {
        return;
    }

    acc.flush_all(window, events);
    let _ = window.emit(
        events.finish,
        payload::Finish::from_usage(
            serde_json::Value::String(full_text),
            last_usage.as_ref(),
        ),
    );
}

/// emit 错误事件（emit 失败时静默——窗口可能已关闭）。
fn emit_error(window: &Window, events: &ChatEvents, message: impl Into<String>) {
    let _ = window.emit(events.error, payload::Error::new(message.into()));
}

/// 工具调用增量累积器。
///
/// `ToolCallDelta` 按 `index` 分片到达（id / name 首片携带，arguments 逐片拼接）；
/// 累积至 arguments 可解析为完整 JSON 时 emit 一次 `tool-call` 事件。
#[derive(Default)]
struct ToolCallAccumulator {
    calls: HashMap<u32, PendingCall>,
}

struct PendingCall {
    id: String,
    name: String,
    arguments: String,
    sent: bool,
}

impl ToolCallAccumulator {
    fn merge(&mut self, deltas: Vec<referee_ai::provider::ToolCallDelta>) {
        for delta in deltas {
            let entry = self.calls.entry(delta.index).or_insert_with(|| PendingCall {
                id: String::new(),
                name: String::new(),
                arguments: String::new(),
                sent: false,
            });
            if let Some(id) = delta.id {
                if !id.is_empty() {
                    entry.id = id;
                }
            }
            if let Some(func) = delta.function {
                if let Some(name) = func.name {
                    if !name.is_empty() {
                        entry.name = name;
                    }
                }
                if let Some(args) = func.arguments {
                    entry.arguments.push_str(&args);
                }
            }
        }
    }

    /// 取出未发送的工具调用（按 `index` 升序）并标记已发送。
    ///
    /// `complete_only = true` 时仅取参数可解析为完整 JSON 的条目
    /// （流式增量阶段，避免把半截参数当字符串发出）；`false` 为流结束
    /// 兜底（参数不完整按原始字符串发）。返回后调用即视为已处理。
    fn take_unsent(&mut self, complete_only: bool) -> Vec<payload::ToolCall> {
        let mut indexes: Vec<u32> = self.calls.keys().copied().collect();
        indexes.sort_unstable();
        let mut out = Vec::new();
        for idx in indexes {
            let Some(call) = self.calls.get_mut(&idx) else { continue };
            if call.sent {
                continue;
            }
            if complete_only && call.arguments.is_empty() {
                continue;
            }
            let parsed = serde_json::from_str::<serde_json::Value>(&call.arguments);
            if complete_only && parsed.is_err() {
                continue;
            }
            call.sent = true;
            out.push(payload::ToolCall {
                id: call.id.clone(),
                name: call.name.clone(),
                input: parsed.unwrap_or(serde_json::Value::String(call.arguments.clone())),
            });
        }
        out
    }

    /// emit 参数已完整的工具调用（JSON 解析成功即视为完整）。
    ///
    /// 按模型声明的 `index` 升序 emit——并行调用时保证时间线顺序稳定
    /// （HashMap 迭代无序）。
    fn flush_ready(&mut self, window: &Window, events: &ChatEvents) {
        for call in self.take_unsent(true) {
            tracing::debug!(
                target = "fluen_chat_bridge",
                tool_name = %call.name,
                tool_call_id = %call.id,
                "模型发起工具调用"
            );
            let _ = window.emit(events.tool_call, call);
        }
    }

    /// 流结束时冲刷全部未发送的工具调用（兜底，参数不完整时按原始字符串发）。
    fn flush_all(&mut self, window: &Window, events: &ChatEvents) {
        for call in self.take_unsent(false) {
            let _ = window.emit(events.tool_call, call);
        }
    }

    fn pending_is_empty(&self) -> bool {
        self.calls.values().all(|c| c.sent)
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_maps_roles() {
        let history = vec![
            HistoryMessage {
                role: "user".into(),
                content: "你好".into(),
            },
            HistoryMessage {
                role: "assistant".into(),
                content: "你好，有什么可以帮你？".into(),
            },
            HistoryMessage {
                role: "unknown".into(),
                content: "按 user 处理".into(),
            },
        ];
        let messages = to_referee_messages(&history);
        assert_eq!(messages.len(), 3);
        assert!(matches!(messages[0].role, Role::User));
        assert!(matches!(messages[1].role, Role::Assistant));
        assert!(matches!(messages[2].role, Role::User));
    }

    #[test]
    fn empty_history_maps_to_empty() {
        assert!(to_referee_messages(&[]).is_empty());
    }

    use referee_ai::provider::{ToolCallDelta, ToolCallFunctionDelta};

    /// 构造一条工具调用增量分片。
    fn delta(index: u32, id: &str, name: &str, args: &str) -> ToolCallDelta {
        ToolCallDelta {
            index,
            id: Some(id.into()),
            function: Some(ToolCallFunctionDelta {
                name: Some(name.into()),
                arguments: Some(args.into()),
            }),
        }
    }

    #[test]
    fn turn_reset_allows_index_reuse_across_turns() {
        // 回合 1：index 0/1 两个工具调用，全部发出
        let mut acc = ToolCallAccumulator::default();
        acc.merge(vec![
            delta(0, "c1", "paper_outline", r#"{"title":"大纲"}"#),
            delta(1, "c2", "paper_section", r#"{"title":"一、引言"}"#),
        ]);
        let sent = acc.take_unsent(true);
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].name, "paper_outline");
        assert_eq!(sent[1].name, "paper_section");

        // 回合边界（Finish 分片处理）：冲刷残余并重置累积器
        acc = ToolCallAccumulator::default();

        // 回合 2：index 从 0 重计——delegate_agent 必须正常发出
        acc.merge(vec![delta(
            0,
            "c3",
            "delegate_agent",
            r#"{"agent_id":"essay_writing","task":"扩写研究现状"}"#,
        )]);
        let sent = acc.take_unsent(true);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].name, "delegate_agent");
        assert_eq!(sent[0].input["agent_id"], "essay_writing");
    }

    #[test]
    fn missing_turn_reset_swallows_next_turn_calls() {
        // 回归锚点：不重置时（旧行为），第二回合同 index 的调用
        // 并入已发送条目被静默吞掉——委派面板消失的根因
        let mut acc = ToolCallAccumulator::default();
        acc.merge(vec![delta(0, "c1", "paper_outline", r#"{"title":"大纲"}"#)]);
        assert_eq!(acc.take_unsent(true).len(), 1);

        acc.merge(vec![delta(0, "c2", "delegate_agent", r#"{"task":"扩写"}"#)]);
        assert!(acc.take_unsent(true).is_empty(), "未重置时第二回合调用应被吞掉");
        // 且参数被污染：原条目参数已不再是合法 JSON
        assert!(!acc.calls[&0].sent || acc.calls[&0].arguments.contains("扩写"));
    }
}
