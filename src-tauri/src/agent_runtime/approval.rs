//! 工具审批装饰器——在工具执行前插入用户确认等待。
//!
//! referee 的工具系统没有 `pre_tool_use` 钩子，审批通过**装饰器**实现：
//! [`ApprovalGuard`] 包装需审批的工具，`execute` 前调用 [`Approver`]
//! 挂起等待用户决策——批准放行，拒绝以错误反馈给 LLM。
//!
//! ## 设计
//!
//! - [`Approver`] 由业务侧实现（如弹窗征求前端确认）；「哪些调用需要审批」
//!   的策略由实现内部判断（如只读操作直接放行），装饰器本身无条件转发
//! - 装饰器透明转发 `name` / `description` / `input_schema` / `category` /
//!   `default_wait`，对引擎与 LLM 完全不可见
//! - 超时治理由 [`ToolExecutor`](referee_ai::tool::ToolExecutor) 统一负责：
//!   装配含审批工具的运行时应将 `ExecutorConfig::tool_timeout` 调至大于
//!   审批等待上限（如审批 5 分钟 → 超时 6 分钟）
//!
//! ## 示例
//!
//! ```rust,ignore
//! // 本模块为 crate 私有（mod agent_runtime），doctest 无法链接此路径，
//! // 示例仅作概念展示、不参与编译
//! use std::sync::Arc;
//! use fluen_lib::agent_runtime::approval::ApprovalGuard;
//!
//! # fn make(
//! #     tool: Arc<dyn referee_ai::tool::Tool>,
//! #     approver: Arc<dyn fluen_lib::agent_runtime::approval::Approver>,
//! # ) -> Arc<dyn referee_ai::tool::Tool> {
//! // 包装写工具：execute 前等待用户批准
//! ApprovalGuard::new(tool, approver)
//! # }
//! ```

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use referee_ai::tool::{Tool, ToolCategory, ToolContext, ToolError, ToolOutput};
use serde_json::Value;

/// 审批决策器——业务侧实现（弹窗等待、策略判断）。
///
/// 实现内部判断该调用是否需要审批（如只读操作直接 `Ok(())` 放行）；
/// 拒绝时返回 `Err`（错误信息会反馈给 LLM，可据此调整后续行为）。
#[async_trait]
pub trait Approver: Send + Sync {
    /// 审批一次工具调用。
    ///
    /// - `Ok(())`：放行，工具继续执行
    /// - `Err`：拒绝（错误信息反馈给 LLM）
    async fn approve(&self, tool_name: &str, input: &Value) -> Result<(), ToolError>;
}

/// 审批装饰器——包装工具，执行前征求用户批准。
///
/// 对引擎透明：声明信息全部转发被包装工具，仅 `execute` 前插入审批等待。
pub struct ApprovalGuard {
    inner: Arc<dyn Tool>,
    approver: Arc<dyn Approver>,
}

impl ApprovalGuard {
    /// 构造装饰器。
    pub fn new(inner: Arc<dyn Tool>, approver: Arc<dyn Approver>) -> Self {
        Self { inner, approver }
    }
}

/// 含审批工具的默认执行器超时（审批等待 300s + 执行余量）。
///
/// 适用于无委派场景（学术助手、子智能体等）；Motis 总督因需容纳
/// 委派 RPC（600s）取更大值，见 `motis_chat::timeouts`。
pub const APPROVAL_EXECUTOR_TIMEOUT: Duration = Duration::from_secs(360);

/// 构造适用于含审批工具的执行器。
///
/// `tool_timeout` 须大于审批等待上限（业务侧 [`Approver`] 实现约定
/// 最长 5 分钟）；无特殊要求时用 [`APPROVAL_EXECUTOR_TIMEOUT`]，
/// 调用方按场景取值（如 Motis 总督须大于委派 RPC 超时）。
pub fn approval_executor(tool_timeout: Duration) -> referee_ai::tool::ToolExecutor {
    use referee_ai::tool::{ExecutorConfig, ToolExecutor};

    let config = ExecutorConfig {
        tool_timeout,
        ..ExecutorConfig::default()
    };
    ToolExecutor::new(config)
}

#[async_trait]
impl Tool for ApprovalGuard {
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
        self.approver.approve(self.inner.name(), &args).await?;
        self.inner.execute(ctx, args).await
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 记录执行次数的桩工具。
    struct CountingTool {
        executions: AtomicUsize,
    }

    #[async_trait]
    impl Tool for CountingTool {
        fn name(&self) -> &str {
            "counting"
        }
        fn description(&self) -> &str {
            "counting tool"
        }
        fn input_schema(&self) -> Value {
            json!({ "type": "object" })
        }
        async fn execute(&self, _ctx: ToolContext, _args: Value) -> Result<ToolOutput, ToolError> {
            self.executions.fetch_add(1, Ordering::SeqCst);
            Ok(ToolOutput::text("done"))
        }
    }

    /// 可配置决策的桩审批器。
    struct StubApprover {
        approved: bool,
        calls: AtomicUsize,
    }

    #[async_trait]
    impl Approver for StubApprover {
        async fn approve(&self, _tool_name: &str, _input: &Value) -> Result<(), ToolError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.approved {
                Ok(())
            } else {
                Err(ToolError::Execution("用户拒绝了操作".into()))
            }
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
    async fn approved_call_executes_inner_tool() {
        let tool = Arc::new(CountingTool {
            executions: AtomicUsize::new(0),
        });
        let approver = Arc::new(StubApprover {
            approved: true,
            calls: AtomicUsize::new(0),
        });

        let guard = ApprovalGuard::new(tool.clone() as Arc<dyn Tool>, approver.clone());
        let output = guard.execute(ctx(), json!({})).await.unwrap();
        assert_eq!(output.content, "done");
        assert_eq!(tool.executions.load(Ordering::SeqCst), 1);
        assert_eq!(approver.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn rejected_call_skips_inner_tool() {
        let tool = Arc::new(CountingTool {
            executions: AtomicUsize::new(0),
        });
        let approver = Arc::new(StubApprover {
            approved: false,
            calls: AtomicUsize::new(0),
        });

        let guard = ApprovalGuard::new(tool.clone() as Arc<dyn Tool>, approver);
        let err = guard.execute(ctx(), json!({})).await.unwrap_err();
        assert!(err.to_string().contains("用户拒绝"));
        assert_eq!(tool.executions.load(Ordering::SeqCst), 0, "拒绝后不得执行内部工具");
    }

    #[test]
    fn declaration_is_transparent() {
        let tool = Arc::new(CountingTool {
            executions: AtomicUsize::new(0),
        });
        let approver = Arc::new(StubApprover {
            approved: true,
            calls: AtomicUsize::new(0),
        });

        let guard = ApprovalGuard::new(tool as Arc<dyn Tool>, approver);
        assert_eq!(guard.name(), "counting");
        assert_eq!(guard.description(), "counting tool");
        let decl = guard.to_declaration();
        assert_eq!(decl.name, "counting");
    }
}
