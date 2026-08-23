//! 工具观测装饰器——把工具执行的开始/结束上报给外部接收器。
//!
//! referee 的工具系统没有执行期钩子（`ExecutorConfig` 亦无可注入回调），
//! 可观测性通过**装饰器**实现：[`ObservedTool`] 包装任意工具，
//! `execute` 前后调用 [`ToolEventSink`] 上报——对引擎与 LLM 完全透明
//! （声明信息全部转发被包装工具），与 [`ApprovalGuard`](super::approval::ApprovalGuard)
//! 同构。
//!
//! ## 注册表级包装
//!
//! [`observe_registry`] 以「重建注册表」方式对既有工具集整体包装：
//! 业务装配层（`agent_tools::assemble` 等）与工具实现**零改动**。
//!
//! ## 示例
//!
//! ```no_run
//! use std::sync::Arc;
//! use fluen_lib::agent_runtime::observability::observe_registry;
//! use referee_ai::tool::ToolRegistry;
//! # fn wrap(registry: &ToolRegistry, sink: Arc<dyn fluen_lib::agent_runtime::observability::ToolEventSink>) -> ToolRegistry {
//! observe_registry(registry, sink)
//! # }
//! ```

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use referee_ai::tool::{Tool, ToolCategory, ToolContext, ToolError, ToolOutput, ToolRegistry};
use serde_json::Value;

/// 工具执行事件接收器——业务侧实现（如转发为 Tauri 事件）。
///
/// 回调在工具执行的异步上下文中同步调用，实现方须快速返回、不得阻塞。
pub trait ToolEventSink: Send + Sync {
    /// 工具开始执行（审批等待之前，等待时长计入耗时）。
    fn on_tool_start(&self, ctx: &ToolContext, name: &str, input: &Value);

    /// 工具执行结束（成功或失败，含被装饰层之下抛出的错误）。
    fn on_tool_end(&self, ctx: &ToolContext, name: &str, outcome: ToolOutcome<'_>, duration_ms: u64);
}

/// 工具结束事件的结果载荷。
#[derive(Debug)]
pub enum ToolOutcome<'a> {
    /// 执行成功（`content` 为工具输出）。
    Ok(&'a ToolOutput),
    /// 执行失败（含审批拒绝、超时前被装饰层返回的错误等）。
    Err(&'a ToolError),
}

impl ToolOutcome<'_> {
    /// 是否成功。
    pub fn is_ok(&self) -> bool {
        matches!(self, ToolOutcome::Ok(_))
    }
}

/// 观测装饰器——包装工具，执行前后向 [`ToolEventSink`] 上报。
///
/// 对引擎透明：声明信息全部转发被包装工具，仅 `execute` 前后打点。
pub struct ObservedTool {
    inner: Arc<dyn Tool>,
    sink: Arc<dyn ToolEventSink>,
}

impl ObservedTool {
    /// 包装单个工具。
    pub fn new(inner: Arc<dyn Tool>, sink: Arc<dyn ToolEventSink>) -> Arc<dyn Tool> {
        Arc::new(Self { inner, sink })
    }
}

#[async_trait]
impl Tool for ObservedTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn input_schema(&self) -> Value {
        self.inner.input_schema()
    }

    fn category(&self) -> ToolCategory {
        self.inner.category()
    }

    fn default_wait(&self) -> bool {
        self.inner.default_wait()
    }

    fn depth_limited(&self) -> bool {
        self.inner.depth_limited()
    }

    async fn execute(&self, ctx: ToolContext, args: Value) -> Result<ToolOutput, ToolError> {
        let started = Instant::now();
        self.sink.on_tool_start(&ctx, self.inner.name(), &args);
        let result = self.inner.execute(ctx.clone(), args).await;
        let outcome = match &result {
            Ok(output) => ToolOutcome::Ok(output),
            Err(e) => ToolOutcome::Err(e),
        };
        self.sink
            .on_tool_end(&ctx, self.inner.name(), outcome, started.elapsed().as_millis() as u64);
        result
    }
}

/// 对既有工具集整体观测包装——重建注册表，逐工具套 [`ObservedTool`]。
///
/// 业务装配层与工具实现零改动；同名重注册在相同配置下必然成功，
/// 失败视为装配期编程错误。
pub fn observe_registry(registry: &ToolRegistry, sink: Arc<dyn ToolEventSink>) -> ToolRegistry {
    let observed = ToolRegistry::with_defaults();
    for tool in registry.all() {
        observed
            .register(ObservedTool::new(tool, sink.clone()))
            .expect("observe_registry: re-registering existing tools cannot conflict");
    }
    observed
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    /// 桩工具：固定输出。
    struct StubTool {
        fail: bool,
    }

    #[async_trait]
    impl Tool for StubTool {
        fn name(&self) -> &str {
            "stub"
        }
        fn description(&self) -> &str {
            "stub tool"
        }
        fn input_schema(&self) -> Value {
            json!({ "type": "object" })
        }
        async fn execute(&self, _ctx: ToolContext, _args: Value) -> Result<ToolOutput, ToolError> {
            if self.fail {
                Err(ToolError::Execution("boom".into()))
            } else {
                Ok(ToolOutput::text("done"))
            }
        }
    }

    /// 记录事件的桩接收器。
    struct RecordingSink {
        events: Mutex<Vec<(&'static str, String, bool)>>,
    }

    impl ToolEventSink for RecordingSink {
        fn on_tool_start(&self, _ctx: &ToolContext, name: &str, _input: &Value) {
            self.events
                .lock()
                .unwrap()
                .push(("start", name.to_string(), true));
        }

        fn on_tool_end(&self, _ctx: &ToolContext, name: &str, outcome: ToolOutcome<'_>, _duration_ms: u64) {
            self.events
                .lock()
                .unwrap()
                .push(("end", name.to_string(), outcome.is_ok()));
        }
    }

    fn ctx() -> ToolContext {
        ToolContext {
            tool_call_id: "test-call".into(),
            session_id: uuid::Uuid::new_v4(),
            turn_id: 0,
            kernel: None,
            store: None,
            wait: true,
            peer_depth: 0,
        }
    }

    #[tokio::test]
    async fn success_reports_start_and_end() {
        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        });
        let tool = ObservedTool::new(Arc::new(StubTool { fail: false }), sink.clone());
        let out = tool.execute(ctx(), json!({})).await.unwrap();
        assert_eq!(out.content, "done");

        let events = sink.events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], ("start", "stub".to_string(), true));
        assert_eq!(events[1], ("end", "stub".to_string(), true));
    }

    #[tokio::test]
    async fn failure_reports_end_with_error() {
        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        });
        let tool = ObservedTool::new(Arc::new(StubTool { fail: true }), sink.clone());
        assert!(tool.execute(ctx(), json!({})).await.is_err());

        let events = sink.events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1], ("end", "stub".to_string(), false));
    }

    #[test]
    fn declaration_is_transparent() {
        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        });
        let tool = ObservedTool::new(Arc::new(StubTool { fail: false }), sink);
        assert_eq!(tool.name(), "stub");
        assert_eq!(tool.description(), "stub tool");
    }

    #[test]
    fn observe_registry_preserves_tools() {
        let registry = ToolRegistry::with_defaults();
        registry
            .register(Arc::new(StubTool { fail: false }))
            .unwrap();
        registry
            .register(Arc::new(StubTool { fail: true }))
            .unwrap_err(); // 同名注册应失败，确保只有一个 stub

        let sink = Arc::new(RecordingSink {
            events: Mutex::new(Vec::new()),
        });
        let observed = observe_registry(&registry, sink);
        assert_eq!(observed.len(), registry.len());
        assert!(observed.get("stub").is_some());
    }
}
