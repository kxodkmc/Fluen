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
//! | `motis:context-usage` | `ContextUsageReport` | 本轮上下文分类估算（发送前） |
//! | `motis:finish` | `{ result, total_tokens, prompt_tokens, completion_tokens }` | 完成 |
//! | `motis:error` | `{ message: string }` | 错误 |
//!
//! ## 会话模型
//!
//! 前端是对话的事实源（每轮传入完整 history）。每轮：
//! 新建 `SessionId` → `restore_session_history` 恢复上下文 → `chat_stream`
//! 启动流式回合 → [`chat_bridge`] 消费流并 emit 事件。
//! 会话与运行时随回合结束丢弃（引擎内部自动回收）。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tauri::{Emitter, State, Window};
use uuid::Uuid;

use crate::chat_bridge::{
    self, start_chat_session, to_referee_messages, ChatEvents, HistoryMessage,
};
use crate::llm_config::storage::ConfigStorage;
use crate::mascot::storage::{MascotConfigStorage, MascotDataStorage};
use referee_ai::session::SessionId;

use super::agent_reporter::AgentReporter;
use super::approval::{ApprovalMap, ApprovalOutcome, MotisApprover};
use super::context_usage;
use crate::agent_tools::project::read_state::ReadTracker;
use super::error::MotisChatError;
use super::events::{EVENT_APPROVAL_REQUEST, EVENT_CONTEXT_USAGE};
use super::federation::FederationPool;
use super::prompt;
use super::runtime::build_runtime;

/// motis 事件集（`motis:*` 前缀）。
const EVENTS: ChatEvents = ChatEvents {
    thought: super::events::EVENT_THOUGHT,
    text: super::events::EVENT_TEXT,
    tool_call: super::events::EVENT_TOOL_CALL,
    finish: super::events::EVENT_FINISH,
    error: super::events::EVENT_ERROR,
};

/// Motis 聊天全局状态：活跃回合句柄与待审批请求映射。
///
/// `handles` 供 `motis_chat_cancel` 中断当前回合；
/// `approvals` 为工具审批请求的决策通道（审批器写入，
/// `motis_chat_resolve_approval` 读取）。
pub struct MotisChatState {
    /// 活跃的回合句柄映射（key: run_id）。
    handles: Mutex<HashMap<String, referee_ai::engine::ChatHandle>>,
    /// 待审批请求的决策通道映射（key: approval_id）。
    approvals: Arc<ApprovalMap>,
    /// 项目文件读取跟踪器（写前必读门记账，全智能体共享）。
    read_tracker: Arc<ReadTracker>,
}

impl MotisChatState {
    /// 创建新的聊天状态实例。
    pub fn new() -> Self {
        Self {
            handles: Mutex::new(HashMap::new()),
            approvals: Arc::new(Mutex::new(HashMap::new())),
            read_tracker: ReadTracker::new_arc(),
        }
    }

    /// 返回审批通道的共享句柄（供 [`MotisApprover`] 写入）。
    pub fn approvals_handle(&self) -> Arc<ApprovalMap> {
        self.approvals.clone()
    }

    /// 返回读取跟踪器的共享句柄（供装配层接线写前必读门）。
    pub fn read_tracker_handle(&self) -> Arc<ReadTracker> {
        self.read_tracker.clone()
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

impl Default for MotisChatState {
    fn default() -> Self {
        Self::new()
    }
}

/// 发送消息并流式接收回复。
///
/// # 流程
///
/// 1. 加载 MascotConfig + LlmConfig（MascotData 在提示词组装时读取）
/// 2. 构建 referee 运行时（项目级工具 + 审批包装）
/// 3. 回放历史 + 启动流式回合
/// 4. [`chat_bridge`] 消费流并 emit 到前端
/// 5. 完成后清理回合句柄
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
    federation_pool: State<'_, FederationPool>,
    socstat_mcp: State<'_, crate::mcp_host::socstat::SocstatMcpHost>,
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

    // 2. 构建运行时（论文内容工具 + 文件工具审批包装）
    let approver = Arc::new(MotisApprover::new(
        window.clone(),
        chat_state.approvals_handle(),
        EVENT_APPROVAL_REQUEST,
        project_path.clone(),
    ));
    // 子智能体事件上报器：委派生命周期与子代理内部工具调用经统一
    // emit 回调分发为 Tauri 事件（事件名由上报器侧指定）
    let agent_reporter = Arc::new(AgentReporter::new(
        {
            let window = window.clone();
            move |event, payload| {
                let _ = window.emit(event, payload.clone());
            }
        },
        federation_pool.tracker(),
    ));
    let bundle = build_runtime(
        &mascot_config,
        &llm_config,
        project_path.as_deref(),
        approver,
        chat_state.read_tracker_handle(),
        agent_reporter,
        &federation_pool,
        Some(socstat_mcp.inner().clone()),
    )
    .await?;

    // 3. 回放历史 + 启动流式回合
    let session_id = SessionId::new_v4();
    // 构建已启用子智能体描述（供提示词注入）。
    // 工具与委派仅在「工具调用开关 + 已打开论文项目」时可用。
    let function_calling_available =
        mascot_config.function_calling_enabled && project_path.is_some();
    let agents_desc = if function_calling_available {
        crate::motis_chat::agents::enabled_agents_description(&mascot_config.enabled_agents)
    } else {
        String::new()
    };
    // 成果板清单注入提示词：委派仅回传 artifact_id 且历史只回放纯文本，
    // 跨轮后 ID 必然丢失——注入真实 ID 让 Motis 随时能用 read_artifact 取回
    let board_section = bundle
        .board_store
        .as_ref()
        .map(|store| prompt::board_section(&store.snapshot()))
        .unwrap_or_default();
    let prompt_sections = prompt::build_system_prompt_parts(
        &mascot_config,
        &mascot_data,
        &agents_desc,
        &board_section,
        function_calling_available,
    );
    let system_prompt = prompt::join_sections(&prompt_sections);

    // 本轮请求上下文用量分类估算——发送前推送，驱动前端「上下文容量」面板
    let usage_report = context_usage::build_report(
        &prompt_sections,
        &bundle.tool_declarations,
        &history,
        &message,
        bundle.context_window,
        bundle.max_output_tokens,
    );
    let _ = window.emit(EVENT_CONTEXT_USAGE, &usage_report);

    let handle = start_chat_session(
        &bundle.runtime,
        session_id,
        to_referee_messages(&history),
        message,
        system_prompt,
        bundle.thinking_enabled,
    )
    .map_err(MotisChatError::Runtime)?;

    // 4. 注册句柄并消费流
    let run_id = Uuid::new_v4().to_string();
    chat_state.register(run_id.clone(), handle.clone());
    chat_bridge::consume_stream(handle, &window, &EVENTS).await;
    chat_state.unregister(&run_id);

    // 兜底清理：父回合被取消/出错时，仍在运行的委派子会话会被中断
    federation_pool.interrupt_children().await;

    Ok(())
}

/// 取消当前活跃的 Motis 会话（含进行中的委派子会话）。
#[tauri::command]
pub async fn motis_chat_cancel(
    chat_state: State<'_, MotisChatState>,
    federation_pool: State<'_, FederationPool>,
) -> Result<(), MotisChatError> {
    chat_state.cancel_active();
    // 父回合取消后，已派发但未完成的子会话不会随调用方 future 中止，需显式中断
    federation_pool.interrupt_children().await;
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
