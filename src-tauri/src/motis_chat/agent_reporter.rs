//! # 子智能体事件上报器——委派过程可观测性
//!
//! [`AgentReporter`] 把子智能体委派的**生命周期**（发起/结束）、子智能体
//! **LLM 输出增量**（思考/文本）、**内部工具调用**（开始/结束）以及
//! **Motis 主会话工具的执行结果** 以 Tauri 事件透传给前端：
//!
//! | 方法 | 会话 | 事件 | 触发点 |
//! |------|------|------|--------|
//! | [`delegation_started`](AgentReporter::delegation_started) | — | `motis:agent-started` | 委派工具发起 RPC 前 |
//! | [`on_thinking_delta`](EngineObserver::on_thinking_delta) | 子会话 | `motis:agent-thought` | 引擎回合内 LLM 思考增量 |
//! | [`on_text_delta`](EngineObserver::on_text_delta) | 子会话 | `motis:agent-text` | 引擎回合内 LLM 文本增量 |
//! | [`on_tool_start`](ToolEventSink::on_tool_start) | 子会话 | `motis:agent-tool-call` | 子代理工具开始执行 |
//! | [`on_tool_end`](ToolEventSink::on_tool_end) | 子会话 | `motis:agent-tool-result` | 子代理工具执行结束 |
//! | [`on_tool_finished`](EngineObserver::on_tool_finished) | 子会话 | `motis:agent-tool-result` | 执行器折叠失败的兜底上报（超时/panic 等） |
//! | [`delegation_finished`](AgentReporter::delegation_finished) | — | `motis:agent-finished` | 委派回信（成功/失败/超时） |
//! | [`on_tool_end`](ToolEventSink::on_tool_end) | 主会话 | `motis:tool-result` | Motis 自身工具执行结束（含审批通过后的写入成败） |
//!
//! ## 事件路由
//!
//! 会话按 [`DelegationTracker`] 区分：委派子会话（tracker 命中）的事件
//! 关联父级 `delegate_agent` 工具调用 ID，展示于子智能体运行面板；
//! 主会话（tracker 未命中）的工具结果按自身调用 ID 直接回填时间线。
//!
//! 上报器同时实现 [`ToolEventSink`]（工具观测装饰器注入点）与
//! [`EngineObserver`]（referee 引擎事件钩子），构建 Motis 运行时与
//! 子代理联邦时注入（见 `runtime::build_motis_tool_registry` / `federation`）。
//!
//! ## 非阻塞约束
//!
//! [`EngineObserver`] 回调在引擎热循环内同步执行，实现只做事件组装与
//! emit 转发（轻量 IPC），不做任何重活或阻塞等待。

use std::sync::Arc;

use referee_ai::session::SessionId;
use referee_ai::tool::{ToolContext, ToolOutcome as ExecutorOutcome};
use referee_ai::EngineObserver;
use serde_json::{json, Value};

use crate::agent_runtime::observability::{ToolEventSink, ToolOutcome};

use super::events::{
    AgentDeltaPayload, AgentFinishedPayload, AgentStartedPayload, AgentToolCallPayload,
    AgentToolResultPayload, ToolResultPayload, EVENT_AGENT_FINISHED, EVENT_AGENT_STARTED,
    EVENT_AGENT_TEXT, EVENT_AGENT_THOUGHT, EVENT_AGENT_TOOL_CALL, EVENT_AGENT_TOOL_RESULT,
    EVENT_PROJECT_UPDATED, EVENT_TOOL_RESULT,
};
use super::federation::DelegationTracker;

/// 子代理工具结果事件的最大字符数（超长截断，防止事件膨胀；
/// 完整结果仍由委派最终结果与工件板承载）。
const TOOL_RESULT_EVENT_MAX_CHARS: usize = 2000;

/// 子智能体事件上报器。
#[derive(Clone)]
pub struct AgentReporter {
    /// 事件发送回调 `(event_name, payload)`——由 commands 层注入（window.emit）。
    emit: Arc<dyn Fn(&str, &Value) + Send + Sync>,
    /// 进行中委派登记表（子会话 → 父委派信息，事件路由用）。
    tracker: Arc<DelegationTracker>,
}

impl AgentReporter {
    /// 构造上报器。
    ///
    /// `emit` 接收 `(事件名, payload)`，payload 已完成序列化前的组装。
    pub fn new<F>(emit: F, tracker: Arc<DelegationTracker>) -> Self
    where
        F: Fn(&str, &Value) + Send + Sync + 'static,
    {
        Self {
            emit: Arc::new(emit),
            tracker,
        }
    }

    fn emit_event(&self, event: &str, payload: &Value) {
        (self.emit)(event, payload);
    }

    /// `manuscript` 写正文成功后通知前端刷新项目内容
    /// （编辑器 / 预览 / 大纲自动跟随 `mainMd` 变化）。
    fn notify_project_update(&self, name: &str, ok: bool) {
        if ok && name == crate::agent_tools::manuscript::MANUSCRIPT_TOOL_NAME {
            self.emit_event(EVENT_PROJECT_UPDATED, &serde_json::json!({}));
        }
    }

    /// 上报委派发起。
    pub fn delegation_started(
        &self,
        tool_call_id: &str,
        agent_id: &str,
        task: &str,
        timeout_ms: u64,
    ) {
        let payload = AgentStartedPayload {
            tool_call_id: tool_call_id.to_string(),
            agent_id: agent_id.to_string(),
            task: task.to_string(),
            timeout_ms,
        };
        self.emit_event(EVENT_AGENT_STARTED, &json!(payload));
    }

    /// 上报委派结束（成功带 token 用量，失败带原因，均带总耗时）。
    pub fn delegation_finished(
        &self,
        tool_call_id: &str,
        agent_id: &str,
        ok: bool,
        duration_ms: u64,
        tokens_used: Option<usize>,
        error: Option<String>,
    ) {
        let payload = AgentFinishedPayload {
            tool_call_id: tool_call_id.to_string(),
            agent_id: agent_id.to_string(),
            ok,
            duration_ms,
            tokens_used,
            error,
        };
        self.emit_event(EVENT_AGENT_FINISHED, &json!(payload));
    }
}

impl ToolEventSink for AgentReporter {
    fn on_tool_start(&self, ctx: &ToolContext, name: &str, input: &Value) {
        // 非委派会话（Motis 主会话）无需 start 事件——前端已有流式 tool-call
        let Some(info) = self.tracker.lookup(&ctx.session_id) else {
            return;
        };
        let payload = AgentToolCallPayload {
            tool_call_id: info.parent_tool_call_id.clone(),
            agent_id: info.agent_id.as_str().to_string(),
            id: ctx.tool_call_id.clone(),
            name: name.to_string(),
            input: input.clone(),
        };
        self.emit_event(EVENT_AGENT_TOOL_CALL, &json!(payload));
    }

    fn on_tool_end(
        &self,
        ctx: &ToolContext,
        name: &str,
        outcome: ToolOutcome<'_>,
        duration_ms: u64,
    ) {
        // 委派子会话：发 agent-tool-result（关联父委派，展示于子智能体运行面板）
        if let Some(info) = self.tracker.lookup(&ctx.session_id) {
            let (ok, result) = match outcome {
                ToolOutcome::Ok(output) => {
                    (true, json!({ "content": truncate(output.content.as_str()) }))
                }
                ToolOutcome::Err(e) => (false, json!({ "error": truncate(&e.to_string()) })),
            };
            self.notify_project_update(name, ok);
            let payload = AgentToolResultPayload {
                tool_call_id: info.parent_tool_call_id.clone(),
                agent_id: info.agent_id.as_str().to_string(),
                id: ctx.tool_call_id.clone(),
                name: name.to_string(),
                ok,
                duration_ms,
                result,
            };
            self.emit_event(EVENT_AGENT_TOOL_RESULT, &json!(payload));
            return;
        }

        // Motis 主会话：发 motis:tool-result 回填时间线（含审批通过后的写入成败）
        let result = match outcome {
            ToolOutcome::Ok(output) => serde_json::from_str(&output.content)
                .unwrap_or_else(|_| serde_json::Value::String(truncate(output.content.as_str()))),
            ToolOutcome::Err(e) => json!({ "error": truncate(&e.to_string()) }),
        };
        self.notify_project_update(name, result.get("error").is_none());
        let payload = ToolResultPayload {
            tool_call_id: ctx.tool_call_id.clone(),
            name: name.to_string(),
            result,
        };
        self.emit_event(EVENT_TOOL_RESULT, &json!(payload));
    }
}

/// 按字符数截断（避免字节截断破坏 UTF-8），超长时追加省略标记。
fn truncate(text: &str) -> String {
    if text.chars().count() <= TOOL_RESULT_EVENT_MAX_CHARS {
        return text.to_string();
    }
    let cut: String = text.chars().take(TOOL_RESULT_EVENT_MAX_CHARS).collect();
    format!("{cut}\n…(已截断)")
}

impl AgentReporter {
    /// 组装并发出一条输出增量事件（思考 / 文本共用）。
    ///
    /// 仅委派子会话（tracker 命中）产生事件；主会话的增量由
    /// Motis 自身的流式通道承载，此处跳过。
    fn emit_delta(&self, event: &str, session_id: SessionId, delta: &str) {
        let Some(info) = self.tracker.lookup(&session_id) else {
            return;
        };
        let payload = AgentDeltaPayload {
            tool_call_id: info.parent_tool_call_id,
            agent_id: info.agent_id.as_str().to_string(),
            delta: delta.to_string(),
        };
        self.emit_event(event, &json!(payload));
    }
}

impl EngineObserver for AgentReporter {
    fn on_thinking_delta(&self, session_id: SessionId, delta: &str) {
        self.emit_delta(EVENT_AGENT_THOUGHT, session_id, delta);
    }

    fn on_text_delta(&self, session_id: SessionId, delta: &str) {
        self.emit_delta(EVENT_AGENT_TEXT, session_id, delta);
    }

    /// 执行器折叠失败的兜底上报——装饰器观测不到的错误路径
    /// （超时 / panic / 未注册 / 许可不可用 / 批次收敛），前端据此把
    /// 卡在「运行中」的活动条目落为失败态。
    ///
    /// `Ok` 结果由 [`ObservedTool`](crate::agent_runtime::observability::ObservedTool)
    /// 装饰器以完整输入/输出上报，此处跳过避免同一调用双报。
    /// 引擎钩子不携带工具名与输出内容（仅分类），`name` 留空、
    /// 前端按内部调用 ID 关联既有条目。
    fn on_tool_finished(
        &self,
        session_id: SessionId,
        tool_call_id: &str,
        outcome: ExecutorOutcome,
        duration_ms: u64,
    ) {
        if outcome == ExecutorOutcome::Ok {
            return;
        }
        let Some(info) = self.tracker.lookup(&session_id) else {
            return;
        };
        let payload = AgentToolResultPayload {
            tool_call_id: info.parent_tool_call_id,
            agent_id: info.agent_id.as_str().to_string(),
            id: tool_call_id.to_string(),
            name: String::new(),
            ok: false,
            duration_ms,
            result: json!({ "error": executor_outcome_error(outcome) }),
        };
        self.emit_event(EVENT_AGENT_TOOL_RESULT, &json!(payload));
    }
}

/// 执行器折叠结果的人读文案。
fn executor_outcome_error(outcome: ExecutorOutcome) -> &'static str {
    match outcome {
        ExecutorOutcome::Timeout => "工具执行超时（执行器层折叠）",
        ExecutorOutcome::Panic => "工具执行异常崩溃",
        ExecutorOutcome::NotFound => "工具未注册",
        ExecutorOutcome::PermitUnavailable => "并发许可不可用",
        ExecutorOutcome::BatchDeadline => "等待类批次超出总时限，被引擎收敛",
        ExecutorOutcome::Ok => "",
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use referee_ai::tool::{ToolError, ToolOutput};
    use std::sync::Mutex;

    use super::super::agents::AgentId;
    use super::super::federation::DelegationInfo;

    fn ctx_with_session(session: uuid::Uuid) -> ToolContext {
        ToolContext {
            tool_call_id: "inner-call".into(),
            session_id: session,
            turn_id: 0,
            kernel: None,
            store: None,
            wait: true,
            peer_depth: 1,
        }
    }

    /// 构造记录事件的上报器 + 登记了委派的 tracker。
    fn reporter_with_delegation() -> (AgentReporter, Arc<Mutex<Vec<(String, Value)>>>, uuid::Uuid) {
        let events: Arc<Mutex<Vec<(String, Value)>>> = Arc::new(Mutex::new(Vec::new()));
        let tracker = Arc::new(DelegationTracker::new());
        let session = uuid::Uuid::new_v4();
        tracker.track(
            session,
            DelegationInfo {
                agent_id: AgentId::EssayWriting,
                parent_tool_call_id: "parent-call".into(),
                task_preview: "撰写引言".into(),
                started_at: std::time::Instant::now(),
            },
        );
        let reporter = {
            let events = events.clone();
            AgentReporter::new(
                move |name, payload| {
                    events
                        .lock()
                        .unwrap()
                        .push((name.to_string(), payload.clone()))
                },
                tracker,
            )
        };
        (reporter, events, session)
    }

    #[test]
    fn tool_events_route_to_parent_delegation() {
        let (reporter, events, session) = reporter_with_delegation();

        reporter.on_tool_start(
            &ctx_with_session(session),
            "project_read",
            &json!({"path": "a.md"}),
        );
        reporter.on_tool_end(
            &ctx_with_session(session),
            "project_read",
            ToolOutcome::Ok(&ToolOutput::text("内容")),
            12,
        );

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, "motis:agent-tool-call");
        assert_eq!(events[0].1["tool_call_id"], "parent-call");
        assert_eq!(events[0].1["agent_id"], "essay_writing");
        assert_eq!(events[0].1["id"], "inner-call");

        assert_eq!(events[1].0, "motis:agent-tool-result");
        assert_eq!(events[1].1["ok"], true);
        assert_eq!(events[1].1["duration_ms"], 12);
    }

    #[test]
    fn unknown_session_start_is_skipped_but_end_emits_tool_result() {
        let (reporter, events, _session) = reporter_with_delegation();
        // 主会话（tracker 未命中）：start 不发（前端已有流式 tool-call），
        // end 以 motis:tool-result 回填时间线
        reporter.on_tool_start(&ctx_with_session(uuid::Uuid::new_v4()), "x", &json!({}));
        assert!(events.lock().unwrap().is_empty());

        reporter.on_tool_end(
            &ctx_with_session(uuid::Uuid::new_v4()),
            "project_write",
            ToolOutcome::Ok(&ToolOutput::from_json(&json!({"path": "a.md", "ok": true}))),
            33,
        );
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "motis:tool-result");
        assert_eq!(events[0].1["tool_call_id"], "inner-call");
        assert_eq!(events[0].1["name"], "project_write");
        assert_eq!(events[0].1["result"]["ok"], true);
    }

    #[test]
    fn main_session_error_tool_result_is_reported() {
        let (reporter, events, _session) = reporter_with_delegation();
        reporter.on_tool_end(
            &ctx_with_session(uuid::Uuid::new_v4()),
            "manuscript",
            ToolOutcome::Err(&ToolError::Execution("fluen-markup 校验失败，拒绝保存".into())),
            50,
        );
        let events = events.lock().unwrap();
        assert_eq!(events[0].0, "motis:tool-result");
        assert!(events[0].1["result"]["error"]
            .as_str()
            .unwrap()
            .contains("拒绝保存"));
        // 写入失败不触发项目刷新通知
        assert!(events.iter().all(|(name, _)| name != "motis:project-updated"));
    }

    #[test]
    fn successful_manuscript_write_notifies_project_update() {
        let (reporter, events, session) = reporter_with_delegation();
        // 子会话路径：子智能体写正文成功 → agent-tool-result + project-updated
        reporter.on_tool_end(
            &ctx_with_session(session),
            "manuscript",
            ToolOutcome::Ok(&ToolOutput::from_json(&json!({"saved": true}))),
            10,
        );
        // 主会话路径：同样通知
        reporter.on_tool_end(
            &ctx_with_session(uuid::Uuid::new_v4()),
            "manuscript",
            ToolOutcome::Ok(&ToolOutput::from_json(&json!({"saved": true}))),
            10,
        );
        // 只读工具不通知
        reporter.on_tool_end(
            &ctx_with_session(uuid::Uuid::new_v4()),
            "paper_outline",
            ToolOutcome::Ok(&ToolOutput::text("大纲")),
            5,
        );

        let events = events.lock().unwrap();
        let updates = events
            .iter()
            .filter(|(name, _)| name == "motis:project-updated")
            .count();
        assert_eq!(updates, 2, "主/子会话的 manuscript 成功各通知一次");
    }

    #[test]
    fn delegation_lifecycle_payloads() {
        let (reporter, events, _session) = reporter_with_delegation();
        reporter.delegation_started("parent-call", "essay_writing", "写引言", 600_000);
        reporter.delegation_finished(
            "parent-call",
            "essay_writing",
            false,
            1200,
            None,
            Some("超时".into()),
        );

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, "motis:agent-started");
        assert_eq!(events[0].1["timeout_ms"], 600_000);
        assert_eq!(events[1].0, "motis:agent-finished");
        assert_eq!(events[1].1["ok"], false);
        assert_eq!(events[1].1["error"], "超时");
    }

    #[test]
    fn long_tool_result_is_truncated() {
        let (reporter, events, session) = reporter_with_delegation();
        let long = "长".repeat(TOOL_RESULT_EVENT_MAX_CHARS + 100);
        reporter.on_tool_end(
            &ctx_with_session(session),
            "t",
            ToolOutcome::Err(&ToolError::Execution(long.clone())),
            1,
        );
        let events = events.lock().unwrap();
        let text = events[0].1["result"]["error"].as_str().unwrap();
        assert!(text.contains("已截断"), "超长错误信息应被截断");
        assert!(text.chars().count() < long.chars().count() + 32);
    }

    #[test]
    fn engine_deltas_route_to_parent_delegation() {
        let (reporter, events, session) = reporter_with_delegation();

        reporter.on_thinking_delta(session, "思考增量");
        reporter.on_text_delta(session, "写作增量");

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, "motis:agent-thought");
        assert_eq!(events[0].1["tool_call_id"], "parent-call");
        assert_eq!(events[0].1["agent_id"], "essay_writing");
        assert_eq!(events[0].1["delta"], "思考增量");
        assert_eq!(events[1].0, "motis:agent-text");
        assert_eq!(events[1].1["delta"], "写作增量");
    }

    #[test]
    fn unknown_session_engine_events_are_skipped() {
        let (reporter, events, _session) = reporter_with_delegation();
        // 主会话 / 已注销的子会话：引擎事件不外发
        reporter.on_thinking_delta(uuid::Uuid::new_v4(), "x");
        reporter.on_text_delta(uuid::Uuid::new_v4(), "y");
        reporter.on_tool_finished(
            uuid::Uuid::new_v4(),
            "c",
            ExecutorOutcome::Timeout,
            10,
        );
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn executor_folded_failure_falls_back_to_tool_result() {
        let (reporter, events, session) = reporter_with_delegation();

        reporter.on_tool_finished(session, "inner-call", ExecutorOutcome::Timeout, 800);

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "motis:agent-tool-result");
        assert_eq!(events[0].1["tool_call_id"], "parent-call");
        assert_eq!(events[0].1["id"], "inner-call");
        assert_eq!(events[0].1["ok"], false);
        assert_eq!(events[0].1["duration_ms"], 800);
        assert!(events[0].1["result"]["error"].as_str().unwrap().contains("超时"));
    }

    #[test]
    fn executor_ok_outcome_is_not_double_reported() {
        let (reporter, events, session) = reporter_with_delegation();
        // Ok 由观测装饰器以完整内容上报，observer 兜底跳过
        reporter.on_tool_finished(session, "inner-call", ExecutorOutcome::Ok, 5);
        assert!(events.lock().unwrap().is_empty());
    }
}
