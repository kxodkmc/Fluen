//! Tauri commands——学术助手的前端调用接口。
//!
//! 通过 `@tauri-apps/api` 的 `invoke` 函数调用：
//!
//! ```typescript
//! // 发送写作请求（流式接收事件）
//! await invoke('ai_assistant_send', {
//!   message: '帮我写引言',
//!   history: [...],
//!   projectPath: '...'
//! });
//!
//! // 取消当前会话
//! await invoke('ai_assistant_cancel');
//! ```
//!
//! ## 事件流
//!
//! `ai_assistant_send` 启动后，前端通过 `listen` 监听以下事件：
//!
//! | 事件 | payload | 说明 |
//! |------|---------|------|
//! | `ai-assistant:thought` | `{ delta: string }` | 思考增量 |
//! | `ai-assistant:text` | `{ delta: string }` | 文本增量 |
//! | `ai-assistant:tool-call` | `{ id, name, input }` | 工具调用 |
//! | `ai-assistant:approval-request` | `{ id, tool_name, input }` | 写操作审批请求 |
//! | `ai-assistant:finish` | `{ result, total_tokens }` | 完成 |
//! | `ai-assistant:error` | `{ message: string }` | 错误 |
//!
//! ## 审批决策
//!
//! 审批请求的决策回传复用 `motis_chat_resolve_approval` 命令
//! （审批通道与 Motis 共享同一共享存储，approval_id 全局唯一）。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use confluent::agent_runtime::AgentEvent;
use futures::StreamExt;
use serde::Deserialize;
use tauri::{Emitter, State, Window};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::llm_config::storage::ConfigStorage;
use crate::motis_chat::approval::ToolApprovalExtension;
use crate::motis_chat::commands::HistoryMessage;
use crate::motis_chat::events::{
    ErrorPayload, FinishPayload, TextPayload, ThoughtPayload, ToolCallPayload,
};

use super::error::AiAssistantError;
use super::runtime::build_runtime;

/// 思考增量事件。
pub const EVENT_THOUGHT: &str = "ai-assistant:thought";
/// 文本增量事件。
pub const EVENT_TEXT: &str = "ai-assistant:text";
/// 工具调用事件。
pub const EVENT_TOOL_CALL: &str = "ai-assistant:tool-call";
/// 工具审批请求事件（写操作需用户确认）。
pub const EVENT_APPROVAL_REQUEST: &str = "ai-assistant:approval-request";
/// 完成事件。
pub const EVENT_FINISH: &str = "ai-assistant:finish";
/// 错误事件。
pub const EVENT_ERROR: &str = "ai-assistant:error";

/// 学术助手全局状态：维护活跃会话的取消令牌映射。
pub struct AiAssistantState {
    /// 活跃的取消令牌映射（key: run_id）。
    tokens: Mutex<HashMap<String, CancellationToken>>,
}

impl AiAssistantState {
    /// 创建新的状态实例。
    pub fn new() -> Self {
        Self {
            tokens: Mutex::new(HashMap::new()),
        }
    }

    /// 注册取消令牌。
    fn register(&self, run_id: String, token: CancellationToken) {
        let mut map = self.tokens.lock().unwrap();
        map.insert(run_id, token);
    }

    /// 注销取消令牌。
    fn unregister(&self, run_id: &str) {
        let mut map = self.tokens.lock().unwrap();
        map.remove(run_id);
    }

    /// 取消当前活跃的会话。
    fn cancel_active(&self) -> bool {
        let mut map = self.tokens.lock().unwrap();
        if let Some(key) = map.keys().next().cloned() {
            if let Some(token) = map.remove(&key) {
                token.cancel();
                return true;
            }
        }
        false
    }
}

impl Default for AiAssistantState {
    fn default() -> Self {
        Self::new()
    }
}

/// 发送写作请求并流式接收回复。
#[tauri::command]
pub async fn ai_assistant_send(
    message: String,
    history: Vec<HistoryMessage>,
    project_path: Option<String>,
    window: Window,
    llm_storage: State<'_, ConfigStorage>,
    assistant_state: State<'_, AiAssistantState>,
    motis_chat_state: State<'_, crate::motis_chat::MotisChatState>,
) -> Result<(), AiAssistantError> {
    // 1. 加载 LLM 全局配置
    let llm_config = llm_storage
        .load()
        .map_err(|e| AiAssistantError::LlmConfig(e.to_string()))?;

    // 2. 构建运行时（学术写作提示词 + 论文工具 + 审批扩展）
    //    审批决策通道与 Motis 共享（approval_id 全局唯一）
    let approval_extension = Arc::new(ToolApprovalExtension::new(
        window.clone(),
        motis_chat_state.approvals_handle(),
        EVENT_APPROVAL_REQUEST,
    ));
    let runtime = build_runtime(&llm_config, project_path.as_deref(), approval_extension).await?;

    // 3. 构造输入（历史 + 当前消息）
    let input = build_input(&message, &history);

    // 4. 启动流式执行
    let (stream, cancel_token) = runtime.run_stream(input);

    // 5. 注册取消令牌
    let run_id = Uuid::new_v4().to_string();
    assistant_state.register(run_id.clone(), cancel_token);

    // 6. 消费事件流并 emit 到前端
    let mut stream = stream;
    while let Some(result) = stream.next().await {
        match result {
            Ok(event) => {
                if emit_agent_event(&window, event).is_err() {
                    // emit 失败（窗口可能已关闭），终止流
                    break;
                }
            }
            Err(e) => {
                let _ = window.emit(
                    EVENT_ERROR,
                    ErrorPayload {
                        message: e.to_string(),
                    },
                );
                break;
            }
        }
    }

    // 7. 清理取消令牌
    assistant_state.unregister(&run_id);

    Ok(())
}

/// 取消当前活跃的学术助手会话。
#[tauri::command]
pub fn ai_assistant_cancel(
    assistant_state: State<'_, AiAssistantState>,
) -> Result<(), AiAssistantError> {
    assistant_state.cancel_active();
    Ok(())
}

/// 构造 confluent 输入。
///
/// 将历史记录与当前消息格式化为单条用户消息。
fn build_input(message: &str, history: &[HistoryMessage]) -> serde_json::Value {
    let mut parts: Vec<String> = Vec::new();

    for msg in history {
        let role = match msg.role.as_str() {
            "user" => "用户",
            "assistant" => "助手",
            other => other,
        };
        parts.push(format!("{}: {}", role, msg.content));
    }

    parts.push(format!("用户: {}", message));

    serde_json::Value::String(parts.join("\n"))
}

/// 将 [`AgentEvent`] 转换为 Tauri 事件并 emit 到前端。
fn emit_agent_event(window: &Window, event: AgentEvent) -> Result<(), tauri::Error> {
    match event {
        AgentEvent::ThoughtDelta(delta) => {
            window.emit(EVENT_THOUGHT, ThoughtPayload { delta })?;
        }
        AgentEvent::TextDelta(delta) => {
            window.emit(EVENT_TEXT, TextPayload { delta })?;
        }
        AgentEvent::ToolCallRequest(tool_call) => {
            window.emit(
                EVENT_TOOL_CALL,
                ToolCallPayload {
                    id: tool_call.id,
                    name: tool_call.name,
                    input: tool_call.input,
                },
            )?;
        }
        AgentEvent::Finish { result, usage } => {
            window.emit(
                EVENT_FINISH,
                FinishPayload {
                    result,
                    total_tokens: usage.total_tokens,
                },
            )?;
        }
        AgentEvent::Yield => {
            // 主动让出，无需通知前端
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_input_joins_history_and_message() {
        let history = vec![
            HistoryMessage {
                role: "user".into(),
                content: "先写引言".into(),
            },
            HistoryMessage {
                role: "assistant".into(),
                content: "好的".into(),
            },
        ];
        let input = build_input("现在写方法", &history);
        let text = input.as_str().unwrap();
        assert!(text.contains("用户: 先写引言"));
        assert!(text.contains("助手: 好的"));
        assert!(text.contains("用户: 现在写方法"));
    }

    #[test]
    fn event_names_use_ai_assistant_prefix() {
        assert!(EVENT_THOUGHT.starts_with("ai-assistant:"));
        assert!(EVENT_APPROVAL_REQUEST.starts_with("ai-assistant:"));
        assert!(EVENT_ERROR.starts_with("ai-assistant:"));
    }
}
