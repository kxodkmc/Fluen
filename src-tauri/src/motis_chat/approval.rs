//! 工具审批扩展——将写操作确认桥接到前端弹窗。
//!
//! confluent 的 [`ConfluentRuntimeBuilder`] 未透传原生 `ApprovalHandler`，
//! 因此通过 [`RuntimeExtension::pre_tool_use`] 实现等效行为：声明需确认的
//! 工具在调用前推送审批请求到前端并挂起等待，用户点击「应用 / 拒绝」后由
//! `motis_chat_resolve_approval` 命令回传决策。
//!
//! 行为与 confluent 原生审批一致：
//! - 批准 → 放行，工具正常执行
//! - 拒绝 → 返回 `Err`，运行时转成 `ToolError::Unauthorized(原因)` 反馈给 LLM
//! - 运行取消 / 审批超时（5 分钟）→ 挂起被中断
//!
//! ## 审批范围
//!
//! | 工具 | 是否弹窗 |
//! |------|----------|
//! | `project_file`（write / edit / append） | ✅ |
//! | `project_file`（read） | ❌ 只读免审批 |
//! | `manuscript`（论文正文写入） | ✅ |
//! | `fs_execute_command` | ✅ 执行任意命令，必须确认 |
//! | `fs_write_file` | 已被 motis 装配时过滤（绕过确认的写工具） |
//! | 其余工具（fs_read_file / fs_search / MCP 等） | ❌ 暂不弹窗 |
//!
//! ## 事件名
//!
//! 审批请求事件名在构造时指定（如 `motis:approval-request` /
//! `ai-assistant:approval-request`），由各智能体前端分别监听。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use confluent::agent_runtime::{ExecutionContext, RuntimeError, RuntimeExtension};
use serde_json::Value;
use tauri::{Emitter, Window};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::agent_tools::file::PROJECT_FILE_TOOL_NAME;

use super::events::ApprovalRequestPayload;

/// 除 `project_file` 外仍需审批的工具（高风险/写类，按名称匹配）。
const ALWAYS_APPROVE_TOOLS: &[&str] = &["fs_execute_command", "manuscript"];

/// 审批请求超时（5 分钟无响应按拒绝处理，防止前端不响应导致永久挂起）。
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(300);

/// 审批结果（用户决策）。
#[derive(Debug, Clone)]
pub struct ApprovalOutcome {
    /// 是否批准（true = 应用）。
    pub approved: bool,
    /// 拒绝原因（可选，透出给 LLM）。
    pub reason: Option<String>,
}

/// 待审批请求的共享存储：`approval_id → 决策发送端`。
///
/// 由 [`ToolApprovalExtension`] 写入（发起审批时），
/// 由 `motis_chat_resolve_approval` 命令读取（用户决策时）。
pub type ApprovalMap = Mutex<HashMap<String, oneshot::Sender<ApprovalOutcome>>>;

/// 工具审批扩展（注册为 `RuntimeExtension`）。
pub struct ToolApprovalExtension {
    /// 目标窗口（用于 emit 审批请求事件）。
    window: Window,
    /// 审批请求事件名（各智能体前端分别监听）。
    event_name: String,
    /// 待审批请求共享存储（与 `MotisChatState` 共享同一实例）。
    approvals: Arc<ApprovalMap>,
}

impl ToolApprovalExtension {
    /// 构造扩展。
    pub fn new(window: Window, approvals: Arc<ApprovalMap>, event_name: &str) -> Self {
        Self {
            window,
            event_name: event_name.to_string(),
            approvals,
        }
    }
}

/// 判断工具调用是否需要审批。
fn needs_approval(tool_name: &str, input: &Value) -> bool {
    if tool_name == PROJECT_FILE_TOOL_NAME {
        // project_file：写操作（非 read）需要审批
        !matches!(input.get("action").and_then(|v| v.as_str()), Some("read"))
    } else {
        // 其它高风险工具按名称匹配（fs_execute_command 等）
        ALWAYS_APPROVE_TOOLS.contains(&tool_name)
    }
}

#[async_trait]
impl RuntimeExtension for ToolApprovalExtension {
    /// 工具调用前：需要审批的工具弹窗征求用户确认。
    async fn pre_tool_use(
        &self,
        tool_name: &str,
        input: &Value,
        ctx: &ExecutionContext,
    ) -> Result<(), RuntimeError> {
        if !needs_approval(tool_name, input) {
            return Ok(());
        }

        // 1. 生成审批请求 ID 并注册决策通道
        let id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.approvals
            .lock()
            .expect("approval map poisoned")
            .insert(id.clone(), tx);

        // 2. 推送审批请求到前端
        if let Err(e) = self.window.emit(
            &self.event_name,
            ApprovalRequestPayload {
                id: id.clone(),
                tool_name: tool_name.to_string(),
                input: input.clone(),
            },
        ) {
            self.approvals.lock().expect("approval map poisoned").remove(&id);
            return Err(RuntimeError::Fatal(format!("审批通知发送失败: {}", e)));
        }

        // 3. 挂起等待用户决策（或运行取消 / 超时）
        let cancel_token = ctx.cancel_token.clone();
        tokio::select! {
            outcome = rx => match outcome {
                Ok(ApprovalOutcome { approved: true, .. }) => Ok(()),
                Ok(ApprovalOutcome { approved: false, reason }) => Err(RuntimeError::Fatal(format!(
                    "用户拒绝了「{}」操作，未执行：{}",
                    tool_name,
                    reason.unwrap_or_else(|| "未提供原因".into())
                ))),
                Err(_) => Err(RuntimeError::Fatal("审批通道意外关闭".into())),
            },
            _ = cancel_token.cancelled() => {
                self.approvals.lock().expect("approval map poisoned").remove(&id);
                Err(RuntimeError::Cancelled)
            }
            _ = tokio::time::sleep(APPROVAL_TIMEOUT) => {
                self.approvals.lock().expect("approval map poisoned").remove(&id);
                Err(RuntimeError::Fatal("审批请求超时（用户未响应），已按拒绝处理".into()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn project_file_read_is_exempt() {
        assert!(!needs_approval(
            PROJECT_FILE_TOOL_NAME,
            &json!({ "action": "read", "path": "a.txt" })
        ));
    }

    #[test]
    fn project_file_writes_require_approval() {
        for action in ["write", "edit", "append"] {
            assert!(needs_approval(
                PROJECT_FILE_TOOL_NAME,
                &json!({ "action": action, "path": "a.txt" })
            ));
        }
        // action 缺失时按需审批处理（默认安全）
        assert!(needs_approval(PROJECT_FILE_TOOL_NAME, &json!({ "path": "a.txt" })));
    }

    #[test]
    fn execute_command_requires_approval() {
        assert!(needs_approval(
            "fs_execute_command",
            &json!({ "command": "rm", "args": ["-rf", "/"] })
        ));
    }

    #[test]
    fn manuscript_requires_approval() {
        // 论文正文写入工具：整个工具都是写操作
        assert!(needs_approval("manuscript", &json!({ "action": "update", "content": "..." })));
    }

    #[test]
    fn read_only_builtin_tools_are_exempt() {
        assert!(!needs_approval("fs_read_file", &json!({ "path": "a.txt" })));
        assert!(!needs_approval("fs_search", &json!({ "query": "x" })));
    }
}
