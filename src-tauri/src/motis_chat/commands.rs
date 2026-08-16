//! Tauri commands——Motis 聊天的前端调用接口。
//!
//! 通过 `@tauri-apps/api` 的 `invoke` 函数调用：
//!
//! ```typescript
//! // 发送消息（流式接收事件）
//! await invoke('motis_chat_send', {
//!   message: '你好',
//!   history: [{ role: 'user', content: '之前的问题' }, ...]
//! });
//!
//! // 取消当前会话
//! await invoke('motis_chat_cancel');
//! ```
//!
//! ## 事件流
//!
//! `motis_chat_send` 启动后，前端通过 `listen` 监听以下事件：
//!
//! | 事件 | payload | 说明 |
//! |------|---------|------|
//! | `motis:thought` | `{ delta: string }` | 思考增量 |
//! | `motis:text` | `{ delta: string }` | 文本增量 |
//! | `motis:tool-call` | `{ id, name, input }` | 工具调用 |
//! | `motis:finish` | `{ result, total_tokens }` | 完成 |
//! | `motis:error` | `{ message: string }` | 错误 |

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use confluent::agent_runtime::AgentEvent;
use futures::StreamExt;
use serde::Deserialize;
use tauri::{Emitter, State, Window};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::llm_config::storage::ConfigStorage;
use crate::mascot::storage::{MascotConfigStorage, MascotDataStorage};

use super::approval::{ApprovalMap, ApprovalOutcome, ToolApprovalExtension};
use super::error::MotisChatError;
use super::events::{
    ErrorPayload, FinishPayload, TextPayload, ThoughtPayload, ToolCallPayload, EVENT_ERROR,
    EVENT_FINISH, EVENT_TEXT, EVENT_THOUGHT, EVENT_TOOL_CALL,
};
use super::runtime::build_runtime;

/// 历史消息（由前端传入）。
#[derive(Debug, Clone, Deserialize)]
pub struct HistoryMessage {
    /// 消息角色（`"user"` 或 `"assistant"`）。
    pub role: String,
    /// 消息内容。
    pub content: String,
}

/// Motis 聊天全局状态：维护活跃的取消令牌映射与待审批请求映射。
///
/// 通过 `Mutex<HashMap<run_id, CancellationToken>>` 管理当前活跃的会话，
/// 供 `motis_chat_cancel` 取消当前运行；`approvals` 为工具审批请求的
/// 决策通道（审批 handler 写入，`motis_chat_resolve_approval` 读取）。
pub struct MotisChatState {
    /// 活跃的取消令牌映射（key: run_id）。
    tokens: Mutex<HashMap<String, CancellationToken>>,
    /// 待审批请求的决策通道映射（key: approval_id）。
    approvals: Arc<ApprovalMap>,
}

impl MotisChatState {
    /// 创建新的聊天状态实例。
    pub fn new() -> Self {
        Self {
            tokens: Mutex::new(HashMap::new()),
            approvals: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 返回审批通道的共享句柄（供 [`MotisApprovalHandler`] 写入）。
    pub fn approvals_handle(&self) -> Arc<ApprovalMap> {
        self.approvals.clone()
    }

    /// 回传审批决策：移除并发送决策到等待中的审批请求。
    ///
    /// 请求不存在（已处理 / 已取消 / 从未发起）时返回 [`MotisChatError::ApprovalNotFound`]。
    pub fn resolve_approval(
        &self,
        approval_id: &str,
        approved: bool,
        reason: Option<String>,
    ) -> Result<(), MotisChatError> {
        let mut map = self
            .approvals
            .lock()
            .expect("approval map poisoned during resolve");
        let tx = map
            .remove(approval_id)
            .ok_or_else(|| MotisChatError::ApprovalNotFound(approval_id.to_string()))?;
        // receiver 已关闭（请求已取消/超时）时静默忽略
        let _ = tx.send(ApprovalOutcome { approved, reason });
        Ok(())
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
    ///
    /// 取消第一个找到的活跃令牌并移除。返回是否成功取消。
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

impl Default for MotisChatState {
    fn default() -> Self {
        Self::new()
    }
}

/// 发送消息并流式接收回复。
///
/// # 流程
///
/// 1. 加载 MascotConfig + MascotData + LlmConfig
/// 2. 构建 confluent 运行时（装配提示词系统 + MCP / Skills / Toolkit 适配器）
/// 3. 构造输入（历史 + 当前消息）
/// 4. `run_stream` 启动流式执行
/// 5. 逐事件 `window.emit` 推送到前端
/// 6. 完成后清理取消令牌
#[tauri::command]
pub async fn motis_chat_send(
    message: String,
    history: Vec<HistoryMessage>,
    project_path: Option<String>,
    window: Window,
    mascot_storage: State<'_, MascotConfigStorage>,
    mascot_data_storage: State<'_, MascotDataStorage>,
    llm_storage: State<'_, ConfigStorage>,
    chat_state: State<'_, MotisChatState>,
) -> Result<(), MotisChatError> {
    // 1. 加载配置
    let mascot_config = mascot_storage
        .get()
        .map_err(|e| MotisChatError::Config(e.to_string()))?;
    let mascot_data = mascot_data_storage
        .get()
        .map_err(|e| MotisChatError::Config(e.to_string()))?;
    let llm_config = llm_storage
        .load()
        .map_err(|e| MotisChatError::LlmConfig(e.to_string()))?;

    // 2. 构建运行时（含提示词系统 + 论文内容工具 + 工具审批扩展）
    let approval_extension = Arc::new(ToolApprovalExtension::new(
        window.clone(),
        chat_state.approvals_handle(),
        super::events::EVENT_APPROVAL_REQUEST,
    ));
    let runtime = build_runtime(
        &mascot_config,
        &mascot_data,
        &llm_config,
        project_path.as_deref(),
        approval_extension,
    )
    .await?;

    // 3. 构造输入（历史 + 当前消息，人格由系统提示词处理）
    let input = build_input(&message, &history);

    // 4. 启动流式执行
    let (stream, cancel_token) = runtime.run_stream(input);

    // 5. 注册取消令牌
    let run_id = Uuid::new_v4().to_string();
    chat_state.register(run_id.clone(), cancel_token);

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
    chat_state.unregister(&run_id);

    Ok(())
}

/// 取消当前活跃的 Motis 会话。
#[tauri::command]
pub fn motis_chat_cancel(chat_state: State<'_, MotisChatState>) -> Result<(), MotisChatError> {
    chat_state.cancel_active();
    Ok(())
}

/// 回传工具审批决策（前端确认弹窗按钮触发）。
///
/// 审批请求由 `motis:approval-request` 事件推送；用户点击「应用」/「拒绝」后
/// 调用本命令，决策通过 oneshot 通道送达挂起的工具调用。
#[tauri::command]
pub fn motis_chat_resolve_approval(
    approval_id: String,
    approved: bool,
    reason: Option<String>,
    chat_state: State<'_, MotisChatState>,
) -> Result<(), MotisChatError> {
    chat_state.resolve_approval(&approval_id, approved, reason)
}

/// 构造 confluent 输入。
///
/// 将历史记录与当前消息格式化为单条用户消息，
/// 作为 [`ConfluentRuntime::run_stream`] 的输入。
///
/// 人格、表达模式等上下文由 [`MotisContextInjector`] 通过 `dynamic_state`
/// 注入，经 [`PromptExtension`] 组装为 system prompt，不再在此拼接。
fn build_input(message: &str, history: &[HistoryMessage]) -> serde_json::Value {
    let mut parts: Vec<String> = Vec::new();

    // 历史记录
    for msg in history {
        let role = match msg.role.as_str() {
            "user" => "用户",
            "assistant" => "助手",
            other => other,
        };
        parts.push(format!("{}: {}", role, msg.content));
    }

    // 当前消息
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
