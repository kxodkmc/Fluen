//! # 知识库构建事件上报器——构建过程对话流可观测性
//!
//! [`KbReporter`] 把构建管线的 **LLM 输出增量**（思考/文本）与**工具调用**
//! （开始/结束）以 Tauri 事件透传给前端（Kb Agent 对话面板）：
//!
//! | 方法 | 事件 | 触发点 |
//! |------|------|--------|
//! | [`on_thinking_delta`](EngineObserver::on_thinking_delta) | `kbchat:thought` | 引擎回合内 LLM 思考增量 |
//! | [`on_text_delta`](EngineObserver::on_text_delta) | `kbchat:text` | 引擎回合内 LLM 文本增量 |
//! | [`on_tool_start`](ToolEventSink::on_tool_start) | `kbchat:tool-call` | 工具开始执行 |
//! | [`on_tool_end`](ToolEventSink::on_tool_end) | `kbchat:tool-result` | 工具执行结束 |
//! | [`on_tool_finished`](EngineObserver::on_tool_finished) | `kbchat:tool-result` | 执行器折叠失败的兜底上报（超时/panic 等） |
//!
//! ## 事件路由
//!
//! 事件按**当前上下文**（`task_id` + `ref_id`）关联：上下文由 runner 在执行
//! 任务前经 [`set_context`](KbReporter::set_context) 设置、结束后清除。
//! 上下文为 `None` 时事件静默丢弃——会话池跨任务复用 runtime 时，
//! 观察者常驻但无任务上下文，行为等价 no-op。
//!
//! 上报器同时实现 [`ToolEventSink`]（工具观测装饰器注入点，经
//! `observe_registry` 包装）与 [`EngineObserver`]（referee 引擎事件钩子，
//! 经 `FluenRuntimeBuilder::with_observer` 注入）。引擎注入观察者后，
//! 非流式 `chat()` 路径在厂商支持流式时自动改走「内部流式收敛」，
//! 增量经 `on_*_delta` 钩子流出（厂商不支持流式时保持直调，零回归）。
//!
//! ## 非阻塞约束
//!
//! 观察者回调在引擎热循环内同步执行，实现只做事件组装与 emit 转发
//! （轻量 IPC），不做任何重活或阻塞等待。

use std::sync::{Arc, Mutex};

use referee_ai::tool::{ToolContext, ToolOutcome as ExecutorOutcome};
use referee_ai::EngineObserver;
use serde_json::{json, Value};

use crate::agent_runtime::observability::{ToolEventSink, ToolOutcome};

use super::events::{
    KbChatDeltaPayload, KbChatToolCallPayload, KbChatToolResultPayload, EVENT_KBCHAT_TEXT,
    EVENT_KBCHAT_THOUGHT, EVENT_KBCHAT_TOOL_CALL, EVENT_KBCHAT_TOOL_RESULT,
};

/// 工具结果事件的最大字符数（超长截断，防止事件膨胀；
/// 完整结果仍由条目捕获与 checkpoint 承载）。
const TOOL_RESULT_EVENT_MAX_CHARS: usize = 2000;

/// 知识库构建事件上报器。
///
/// `Clone` 语义为共享（内部全 `Arc`）：session 池、runtime 观察者与
/// runner 持有同一实例，runner 更新上下文后观察者即刻感知。
#[derive(Clone)]
pub struct KbReporter {
    /// 事件发送回调 `(event_name, payload)`——由 runner 层注入（window.emit）。
    emit: Arc<dyn Fn(&str, &Value) + Send + Sync>,
    /// 当前事件上下文 `(task_id, ref_id)`；`None` 时静默丢弃事件。
    ctx: Arc<Mutex<Option<(String, String)>>>,
}

impl KbReporter {
    /// 构造上报器。
    ///
    /// `emit` 接收 `(事件名, payload)`，payload 已完成序列化前的组装。
    pub fn new<F>(emit: F) -> Self
    where
        F: Fn(&str, &Value) + Send + Sync + 'static,
    {
        Self {
            emit: Arc::new(emit),
            ctx: Arc::new(Mutex::new(None)),
        }
    }

    /// 设置事件上下文（runner 执行任务前调用）。
    pub fn set_context(&self, task_id: &str, ref_id: &str) {
        *self.ctx.lock().expect("kb reporter ctx poisoned") =
            Some((task_id.to_string(), ref_id.to_string()));
    }

    /// 清除事件上下文（任务结束（成功/失败/取消）后调用）。
    pub fn clear_context(&self) {
        *self.ctx.lock().expect("kb reporter ctx poisoned") = None;
    }

    /// 在上下文存在时以 `(task_id, ref_id)` 执行回调，否则静默跳过。
    fn with_ctx(&self, f: impl FnOnce(&str, &str)) {
        let guard = self.ctx.lock().expect("kb reporter ctx poisoned");
        if let Some((task_id, ref_id)) = guard.as_ref() {
            f(task_id, ref_id);
        }
    }

    fn emit_event(&self, event: &str, payload: &Value) {
        (self.emit)(event, payload);
    }

    /// 组装并发出一条输出增量事件（思考 / 文本共用）。
    fn emit_delta(&self, event: &str, delta: &str) {
        // 空增量（工具调用轮次的角色帧 content:""）不发，避免前端产生空消息
        if delta.is_empty() {
            return;
        }
        self.with_ctx(|task_id, ref_id| {
            let payload = KbChatDeltaPayload {
                task_id: task_id.to_string(),
                ref_id: ref_id.to_string(),
                delta: delta.to_string(),
            };
            self.emit_event(event, &json!(payload));
        });
    }
}

impl ToolEventSink for KbReporter {
    fn on_tool_start(&self, ctx: &ToolContext, name: &str, input: &Value) {
        self.with_ctx(|task_id, ref_id| {
            let payload = KbChatToolCallPayload {
                task_id: task_id.to_string(),
                ref_id: ref_id.to_string(),
                id: ctx.tool_call_id.clone(),
                name: name.to_string(),
                input: input.clone(),
            };
            self.emit_event(EVENT_KBCHAT_TOOL_CALL, &json!(payload));
        });
    }

    fn on_tool_end(
        &self,
        ctx: &ToolContext,
        name: &str,
        outcome: ToolOutcome<'_>,
        duration_ms: u64,
    ) {
        let (ok, result) = match outcome {
            ToolOutcome::Ok(output) => (
                true,
                json!({ "content": truncate(output.content.as_str()) }),
            ),
            ToolOutcome::Err(e) => (false, json!({ "error": truncate(&e.to_string()) })),
        };
        self.with_ctx(|task_id, ref_id| {
            let payload = KbChatToolResultPayload {
                task_id: task_id.to_string(),
                ref_id: ref_id.to_string(),
                tool_call_id: ctx.tool_call_id.clone(),
                name: name.to_string(),
                ok,
                duration_ms,
                result,
            };
            self.emit_event(EVENT_KBCHAT_TOOL_RESULT, &json!(payload));
        });
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

impl EngineObserver for KbReporter {
    fn on_thinking_delta(&self, _session_id: referee_ai::session::SessionId, delta: &str) {
        self.emit_delta(EVENT_KBCHAT_THOUGHT, delta);
    }

    fn on_text_delta(&self, _session_id: referee_ai::session::SessionId, delta: &str) {
        self.emit_delta(EVENT_KBCHAT_TEXT, delta);
    }

    /// 执行器折叠失败的兜底上报——装饰器观测不到的错误路径
    /// （超时 / panic / 未注册 / 许可不可用 / 批次收敛），前端据此把
    /// 卡在「运行中」的活动条目落为失败态。
    ///
    /// `Ok`（正常完成）与 `Failed`（工具主动报错）结果均由
    /// [`ObservedTool`](crate::agent_runtime::observability::ObservedTool)
    /// 装饰器以完整输入/输出上报，此处跳过避免同一调用双报。
    /// 引擎钩子不携带工具名与输出内容（仅分类），`name` 留空、
    /// 前端按调用 ID 关联既有条目。
    fn on_tool_finished(
        &self,
        _session_id: referee_ai::session::SessionId,
        tool_call_id: &str,
        outcome: ExecutorOutcome,
        duration_ms: u64,
    ) {
        if outcome == ExecutorOutcome::Ok {
            return;
        }
        self.with_ctx(|task_id, ref_id| {
            let payload = KbChatToolResultPayload {
                task_id: task_id.to_string(),
                ref_id: ref_id.to_string(),
                tool_call_id: tool_call_id.to_string(),
                name: String::new(),
                ok: false,
                duration_ms,
                result: json!({ "error": executor_outcome_error(outcome) }),
            };
            self.emit_event(EVENT_KBCHAT_TOOL_RESULT, &json!(payload));
        });
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
        ExecutorOutcome::Failed => "工具主动报错（错误文本已随结果回传模型纠错）",
        ExecutorOutcome::Ok => "",
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use referee_ai::session::SessionId;
    use referee_ai::tool::{ToolError, ToolOutput};
    use std::sync::Mutex;

    fn ctx() -> ToolContext {
        ToolContext {
            tool_call_id: "inner-call".into(),
            session_id: SessionId::new_v4(),
            turn_id: 0,
            kernel: None,
            store: None,
            wait: true,
            peer_depth: 0,
        }
    }

    /// 构造记录事件的上报器。
    fn recording_reporter() -> (KbReporter, Arc<Mutex<Vec<(String, Value)>>>) {
        let events: Arc<Mutex<Vec<(String, Value)>>> = Arc::new(Mutex::new(Vec::new()));
        let reporter = {
            let events = events.clone();
            KbReporter::new(move |name, payload| {
                events
                    .lock()
                    .unwrap()
                    .push((name.to_string(), payload.clone()))
            })
        };
        (reporter, events)
    }

    #[test]
    fn no_context_drops_all_events() {
        let (reporter, events) = recording_reporter();
        // 上下文未设置：全部事件静默丢弃（会话池跨任务复用时的 no-op 语义）
        reporter.on_thinking_delta(SessionId::new_v4(), "思考");
        reporter.on_text_delta(SessionId::new_v4(), "文本");
        reporter.on_tool_start(&ctx(), "knowledge_query", &json!({}));
        reporter.on_tool_end(
            &ctx(),
            "knowledge_query",
            ToolOutcome::Ok(&ToolOutput::text("结果")),
            5,
        );
        reporter.on_tool_finished(SessionId::new_v4(), "c", ExecutorOutcome::Timeout, 10);
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn context_set_events_route_with_task_and_ref() {
        let (reporter, events) = recording_reporter();
        reporter.set_context("task-1", "ref-abc");

        reporter.on_thinking_delta(SessionId::new_v4(), "思考增量");
        reporter.on_text_delta(SessionId::new_v4(), "文本增量");
        reporter.on_tool_start(&ctx(), "submit_plan", &json!({"x": 1}));
        reporter.on_tool_end(
            &ctx(),
            "submit_plan",
            ToolOutcome::Ok(&ToolOutput::text("计划已提交")),
            12,
        );

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].0, "kbchat:thought");
        assert_eq!(events[0].1["task_id"], "task-1");
        assert_eq!(events[0].1["ref_id"], "ref-abc");
        assert_eq!(events[0].1["delta"], "思考增量");
        assert_eq!(events[1].0, "kbchat:text");
        assert_eq!(events[2].0, "kbchat:tool-call");
        assert_eq!(events[2].1["name"], "submit_plan");
        assert_eq!(events[2].1["id"], "inner-call");
        assert_eq!(events[3].0, "kbchat:tool-result");
        assert_eq!(events[3].1["ok"], true);
        assert_eq!(events[3].1["duration_ms"], 12);
        assert_eq!(events[3].1["result"]["content"], "计划已提交");
    }

    #[test]
    fn context_cleared_events_stop() {
        let (reporter, events) = recording_reporter();
        reporter.set_context("task-1", "ref-abc");
        reporter.clear_context();
        reporter.on_text_delta(SessionId::new_v4(), "迟到增量");
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn empty_delta_is_skipped() {
        let (reporter, events) = recording_reporter();
        reporter.set_context("task-1", "ref-abc");
        reporter.on_text_delta(SessionId::new_v4(), "");
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn error_tool_result_reports_error_object() {
        let (reporter, events) = recording_reporter();
        reporter.set_context("task-1", "ref-abc");
        reporter.on_tool_end(
            &ctx(),
            "knowledge_create_entry",
            ToolOutcome::Err(&ToolError::Execution("校验失败".into())),
            50,
        );
        let events = events.lock().unwrap();
        assert_eq!(events[0].0, "kbchat:tool-result");
        assert_eq!(events[0].1["ok"], false);
        assert!(events[0].1["result"]["error"].as_str().unwrap().contains("校验失败"));
    }

    #[test]
    fn long_tool_result_is_truncated() {
        let (reporter, events) = recording_reporter();
        reporter.set_context("task-1", "ref-abc");
        let long = "长".repeat(TOOL_RESULT_EVENT_MAX_CHARS + 100);
        reporter.on_tool_end(
            &ctx(),
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
    fn executor_folded_failure_falls_back_to_tool_result() {
        let (reporter, events) = recording_reporter();
        reporter.set_context("task-1", "ref-abc");

        reporter.on_tool_finished(SessionId::new_v4(), "inner-call", ExecutorOutcome::Timeout, 800);

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "kbchat:tool-result");
        assert_eq!(events[0].1["tool_call_id"], "inner-call");
        assert_eq!(events[0].1["ok"], false);
        assert_eq!(events[0].1["duration_ms"], 800);
        assert!(events[0].1["result"]["error"].as_str().unwrap().contains("超时"));
    }

    #[test]
    fn executor_ok_outcome_is_not_double_reported() {
        let (reporter, events) = recording_reporter();
        reporter.set_context("task-1", "ref-abc");
        // Ok 由观测装饰器以完整内容上报，observer 兜底跳过
        reporter.on_tool_finished(SessionId::new_v4(), "inner-call", ExecutorOutcome::Ok, 5);
        assert!(events.lock().unwrap().is_empty());
    }
}
