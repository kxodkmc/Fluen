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
//!
//! ## 会话模型
//!
//! 与 Motis 一致：前端是对话的事实源（每轮传入完整 history）。每轮：
//! 新建 `SessionId` → `restore_session_history` 恢复上下文 → `chat_stream`
//! 启动流式回合 → [`chat_bridge`] 消费流并 emit 事件。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use referee_ai::session::SessionId;
use tauri::{State, Window};
use uuid::Uuid;

use crate::chat_bridge::{self, start_chat_session, to_referee_messages, ChatEvents, HistoryMessage};
use crate::llm_config::storage::ConfigStorage;
use crate::motis_chat::approval::MotisApprover;
use crate::motis_chat::commands::MotisChatState;

use super::error::AiAssistantError;
use super::prompt;
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

/// 学术助手事件集（`ai-assistant:*` 前缀）。
const EVENTS: ChatEvents = ChatEvents {
    thought: EVENT_THOUGHT,
    text: EVENT_TEXT,
    tool_call: EVENT_TOOL_CALL,
    finish: EVENT_FINISH,
    error: EVENT_ERROR,
};

/// 学术助手全局状态：维护活跃回合句柄映射。
///
/// 审批决策通道由 [`MotisChatState`] 持有（两智能体共享，
/// approval_id 全局唯一），本状态仅管理回合取消。
pub struct AiAssistantState {
    /// 活跃的回合句柄映射（key: run_id）。
    handles: Mutex<HashMap<String, referee_ai::engine::ChatHandle>>,
}

impl AiAssistantState {
    /// 创建新的状态实例。
    pub fn new() -> Self {
        Self {
            handles: Mutex::new(HashMap::new()),
        }
    }

    /// 注册回合句柄。
    fn register(&self, run_id: String, handle: referee_ai::engine::ChatHandle) {
        let mut map = self.handles.lock().unwrap();
        map.insert(run_id, handle);
    }

    /// 注销回合句柄。
    fn unregister(&self, run_id: &str) {
        let mut map = self.handles.lock().unwrap();
        map.remove(run_id);
    }

    /// 取消当前活跃的会话。
    ///
    /// 取消第一个找到的活跃回合并移除。返回是否成功取消。
    fn cancel_active(&self) -> bool {
        let mut map = self.handles.lock().unwrap();
        if let Some(key) = map.keys().next().cloned() {
            if let Some(handle) = map.remove(&key) {
                return handle.cancel();
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
///
/// # 流程
///
/// 1. 加载 LlmConfig（学术助手直接用全局激活项）
/// 2. 构建 referee 运行时（论文写作工具 + 审批包装）
/// 3. 回放历史 + 启动流式回合
/// 4. [`chat_bridge`] 消费流并 emit 到前端
/// 5. 完成后清理回合句柄
#[tauri::command]
pub async fn ai_assistant_send(
    message: String,
    history: Vec<HistoryMessage>,
    project_path: Option<String>,
    window: Window,
    llm_storage: State<'_, ConfigStorage>,
    assistant_state: State<'_, AiAssistantState>,
    motis_chat_state: State<'_, MotisChatState>,
) -> Result<(), AiAssistantError> {
    // 1. 加载 LLM 全局配置
    let llm_config = llm_storage
        .load()
        .map_err(|e| AiAssistantError::LlmConfig(e.to_string()))?;

    // 2. 构建运行时（学术写作工具集 + 审批包装）
    //    审批决策通道与 Motis 共享（approval_id 全局唯一）
    let approver = Arc::new(MotisApprover::new(
        window.clone(),
        motis_chat_state.approvals_handle(),
        EVENT_APPROVAL_REQUEST,
    ));
    let (thinking_enabled, runtime) =
        build_runtime(&llm_config, project_path.as_deref(), approver)?;

    // 3. 回放历史 + 启动流式回合
    let session_id = SessionId::new_v4();
    let system_prompt = prompt::build_system_prompt();
    let handle = start_chat_session(
        &runtime,
        session_id,
        to_referee_messages(&history),
        message,
        system_prompt,
        thinking_enabled,
    )
    .map_err(AiAssistantError::Runtime)?;

    // 4. 注册句柄并消费流
    let run_id = Uuid::new_v4().to_string();
    assistant_state.register(run_id.clone(), handle.clone());
    chat_bridge::consume_stream(handle, &window, &EVENTS).await;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_names_use_ai_assistant_prefix() {
        assert!(EVENT_THOUGHT.starts_with("ai-assistant:"));
        assert!(EVENT_TOOL_CALL.starts_with("ai-assistant:"));
        assert!(EVENT_APPROVAL_REQUEST.starts_with("ai-assistant:"));
        assert!(EVENT_FINISH.starts_with("ai-assistant:"));
        assert!(EVENT_ERROR.starts_with("ai-assistant:"));
    }
}
