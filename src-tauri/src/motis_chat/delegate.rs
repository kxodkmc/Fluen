//! # 子智能体委派工具
//!
//! 让 Motis（总督角色）通过 **function calling** 调度子智能体执行任务。
//!
//! Motis 自身不负责编写、计算等具体工作，而是通过 `delegate_agent` 工具
//! 将任务派发给注册的子智能体（学术撰写助手 / 知识库构建助手 / 数据分析助手），
//! 子智能体独立运行后返回结果，由 Motis 汇总。
//!
//! ## 工具接口
//!
//! | 参数 | 类型 | 说明 |
//! |------|------|------|
//! | `agent_id` | string (enum) | 目标子智能体 ID |
//! | `task` | string | 派发给子智能体的任务描述 |
//! | `wait` | boolean (默认 true) | 是否同步等待子智能体完成 |
//!
//! ## 执行流程（referee 内核协议）
//!
//! 1. Motis 的 LLM 决定调用 `delegate_agent`，传入 `agent_id` 与 `task`
//! 2. 委派登记入 [`DelegationTracker`](super::federation::DelegationTracker)
//!    （事件路由 + 取消传播），以 [`DelegationGuard`]（RAII）保证任意
//!    退出路径（回信 / 超时 / 外层切断）都会注销并中断残留子会话
//! 3. 每次委派铸造**全新会话** `SessionId::new_v4()`（任务隔离），组装
//!    [`SessionMessage::Chat`]（`peer_depth + 1` 透传，深度门控由引擎兜底）
//! 4. 经 [`Kernel::invoke`](referee_core::Kernel::invoke) 同步 RPC 到目标
//!    子代理运行时（[`super::federation`] 注册的扩展），论文写作场景
//!    超时放宽到 10 分钟（见 [`super::timeouts`] 分层）
//! 5. 结果按 referee 对等工具语义落库：异步派发或超过阈值的大结果写入
//!    带 ACL 的工件板（按调用者分板），仅回传 `artifact_id`；
//!    Motis 可经 `list_my_board` / `read_artifact` 取回原文
//!
//! ## 可观测性
//!
//! 委派生命周期（started / finished）与子智能体内部工具调用
//! （`agent-tool-call` / `agent-tool-result`，经联邦注入的观测装饰器）
//! 均经 [`AgentReporter`](super::agent_reporter::AgentReporter) 上报前端。
//!
//! 子代理运行时的构建与复用见 [`super::federation`]；本工具只做
//! 协议组合与结果整形，不管理任何生命周期。

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use referee_ai::provider::{Message, ThinkingConfig};
use referee_ai::session::{ChatOptions, ChatPayload, SessionId, SessionMessage, SessionReply};
use referee_ai::tool::{Tool, ToolCategory, ToolContext, ToolError, ToolOutput};
use serde_json::{json, Value};

use super::agents::{self, AgentId};
use super::agent_reporter::AgentReporter;
use super::federation::{DelegationInfo, Federation};
use super::timeouts::DELEGATE_RPC_TIMEOUT_MS;

/// 工具名称。
pub const DELEGATE_AGENT_TOOL_NAME: &str = "delegate_agent";

/// 工具描述。
const DESCRIPTION: &str = "将任务派发给子智能体执行。Motis（总督角色）自身不负责编写、计算等具体工作，而是通过此工具将任务委派给合适的子智能体（如学术撰写助手、知识库构建助手、数据分析助手），子智能体独立执行后返回结果。";

/// 大结果落库阈值（字节）——与 referee 对等工具一致。
const LARGE_RESULT_THRESHOLD: usize = 4096;

/// 任务预览长度（登记表与日志展示用）。
const TASK_PREVIEW_CHARS: usize = 80;

/// 子智能体委派工具——referee 内核协议的薄组合。
///
/// 目标子代理运行时由 [`Federation`](super::federation::Federation)
/// 长寿命持有；本工具每次委派开新会话并同步等待回信。
pub struct DelegateAgentTool {
    /// 输入参数 JSON Schema。
    parameters: Value,
    /// 已启用的子智能体 ID 列表（空列表 = 全部可用）。
    enabled_agents: Vec<String>,
    /// 子智能体事件上报器（委派生命周期 + 结果透传）。
    reporter: Arc<AgentReporter>,
    /// 所属联邦（内核 + 已注册的子代理运行时）。
    federation: Arc<Federation>,
}

impl DelegateAgentTool {
    /// 构造委派工具。
    ///
    /// `enabled_agents` 为空列表时全部子智能体可用；
    /// 非空时仅允许列表中的 ID 被调度。
    pub fn new(
        enabled_agents: Vec<String>,
        reporter: Arc<AgentReporter>,
        federation: Arc<Federation>,
    ) -> Self {
        // 根据启用列表过滤 JSON Schema 中的 enum
        let available_ids: Vec<&str> = if enabled_agents.is_empty() {
            AgentId::all().iter().map(|a| a.as_str()).collect()
        } else {
            AgentId::all()
                .iter()
                .filter(|a| enabled_agents.contains(&a.as_str().to_string()))
                .map(|a| a.as_str())
                .collect()
        };

        let parameters = json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "enum": available_ids,
                    "description": "目标子智能体 ID：academic_writer=学术撰写助手；knowledge_builder=知识库构建助手；data_analyst=数据分析助手"
                },
                "task": {
                    "type": "string",
                    "description": "派发给子智能体的任务描述（应清晰、完整，包含必要的上下文与约束）"
                },
                "wait": {
                    "type": "boolean",
                    "default": true,
                    "description": "结果返回形式（默认 true）：true 回传全文；false 回传成果板引用（artifact_id），不回传正文"
                }
            },
            "required": ["agent_id", "task"]
        });

        Self {
            parameters,
            enabled_agents,
            reporter,
            federation,
        }
    }
}

/// 委派子会话守卫——Drop 时注销登记并中断残留子会话（幂等）。
///
/// 覆盖全部退出路径：正常回信、RPC 超时、以及外层执行器超时导致的
/// future 被 drop（此时 `execute` 后续代码不再执行，唯有 Drop 兜底）。
struct DelegationGuard {
    federation: Arc<Federation>,
    session_id: uuid::Uuid,
}

impl DelegationGuard {
    fn new(federation: Arc<Federation>, session_id: uuid::Uuid) -> Self {
        Self {
            federation,
            session_id,
        }
    }
}

impl Drop for DelegationGuard {
    fn drop(&mut self) {
        self.federation.end_child(&self.session_id);
    }
}

#[async_trait]
impl Tool for DelegateAgentTool {
    fn name(&self) -> &str {
        DELEGATE_AGENT_TOOL_NAME
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn input_schema(&self) -> Value {
        self.parameters.clone()
    }

    /// 委派是内部调用（子代理在引擎内运行，不占用外部 IO 槽位）。
    fn category(&self) -> ToolCategory {
        ToolCategory::Local
    }

    /// 委派是同步等待型工具（默认 wait=true）。
    fn default_wait(&self) -> bool {
        true
    }

    /// 子代理可嵌套调用（声明过滤 + 引擎 max_subagent_depth 兜底）。
    fn depth_limited(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        ctx: ToolContext,
        args: Value,
    ) -> Result<ToolOutput, ToolError> {
        // 1. 解析参数
        let agent_id_str = args
            .get("agent_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments("缺少 agent_id 参数".into()))?;

        let agent_id =
            AgentId::from_str(agent_id_str).ok_or_else(|| {
                ToolError::InvalidArguments(format!("未知 agent_id: {agent_id_str}"))
            })?;

        // 检查子智能体是否已启用
        if !self.enabled_agents.is_empty()
            && !self.enabled_agents.contains(&agent_id.as_str().to_string())
        {
            return Err(ToolError::Execution(format!(
                "子智能体 `{agent_id}` 已被禁用，无法委派任务"
            )));
        }

        let task = args
            .get("task")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| ToolError::InvalidArguments("缺少 task 参数".into()))?;

        let wait = args.get("wait").and_then(|v| v.as_bool()).unwrap_or(true);

        tracing::info!(
            agent_id = %agent_id,
            task_preview = %task.chars().take(TASK_PREVIEW_CHARS).collect::<String>(),
            wait,
            "Motis 委派任务给子智能体"
        );

        // 2. 定位已注册的目标子代理
        let registered = self.federation.agent(&agent_id).ok_or_else(|| {
            ToolError::Execution(format!("子智能体 `{agent_id}` 未在联邦中注册"))
        })?;

        // 3. 组装委派消息：全新会话（任务隔离）+ 深度 +1 透传
        let session_id = SessionId::new_v4();
        let system_prompt = agents::agent_system_prompt(&agent_id)
            .ok_or_else(|| ToolError::Execution("无法获取子智能体提示词".into()))?;
        let payload = ChatPayload {
            message: Message::user(task.to_string()),
            options: ChatOptions {
                system_prompt: Some(system_prompt),
                thinking: ThinkingConfig {
                    enabled: registered.thinking_enabled(),
                    effort: None,
                },
                ..ChatOptions::default()
            },
            peer_depth: ctx.peer_depth + 1,
        };
        let msg = SessionMessage::Chat {
            session_id,
            payload,
        };

        // 4. 登记委派（事件路由 + 取消传播）并上报发起；
        //    RAII 守卫保证任意退出路径都注销登记并中断残留子会话
        let started = Instant::now();
        self.federation.tracker().track(
            session_id,
            DelegationInfo {
                agent_id: agent_id.clone(),
                parent_tool_call_id: ctx.tool_call_id.clone(),
                task_preview: task.chars().take(TASK_PREVIEW_CHARS).collect(),
                started_at: started,
            },
        );
        let _guard = DelegationGuard::new(self.federation.clone(), session_id);
        self.reporter.delegation_started(
            &ctx.tool_call_id,
            agent_id.as_str(),
            task,
            DELEGATE_RPC_TIMEOUT_MS,
        );

        // 上报委派失败并返回错误（闭包统一 finished 事件与耗时）。
        let fail = |error: String| -> ToolError {
            self.reporter.delegation_finished(
                &ctx.tool_call_id,
                agent_id.as_str(),
                false,
                started.elapsed().as_millis() as u64,
                None,
                Some(error.clone()),
            );
            ToolError::Execution(error)
        };

        let invoke_result = match ctx.kernel.as_ref() {
            Some(kernel) => {
                kernel
                    .invoke(registered.runtime_id(), msg.to_envelope(), DELEGATE_RPC_TIMEOUT_MS)
                    .await
            }
            None => Err(referee_core::KernelError::TargetUnreachable),
        };
        let resp_env = invoke_result.map_err(|e| {
            if matches!(e, referee_core::KernelError::Timeout) {
                fail(format!(
                    "委派超时（{} 分钟）：子智能体任务耗时超出预算，可尝试拆分任务后重试",
                    DELEGATE_RPC_TIMEOUT_MS / 60_000
                ))
            } else if matches!(e, referee_core::KernelError::TargetUnreachable) && ctx.kernel.is_none() {
                fail("对等 RPC 未启用：Motis 执行器未注入内核".into())
            } else {
                fail(format!("委派 RPC 失败: {e}"))
            }
        })?;

        // 5. 解析回信
        let reply = SessionReply::from_envelope(&resp_env)
            .map_err(|e| fail(format!("解析子代理回信失败: {e}")))?;

        let elapsed_ms = started.elapsed().as_millis() as u64;
        match reply {
            SessionReply::Success { message, usage, .. } => {
                let content = message.content.as_text().unwrap_or("").to_string();
                let tokens = usage.as_ref().map(|u| u.total_tokens).unwrap_or(0);

                self.reporter.delegation_finished(
                    &ctx.tool_call_id,
                    agent_id.as_str(),
                    true,
                    elapsed_ms,
                    Some(tokens),
                    None,
                );

                // 6. 成果落库：异步派发或超阈值大结果写入调用者（父会话）的工件板，
                //    仅回传 artifact_id（ACL 防止其他会话读取）。
                //    最终结果由外层观测装饰器统一以 motis:tool-result 回填前端
                if !ctx.wait || content.len() > LARGE_RESULT_THRESHOLD {
                    let payload = self.store_artifact(&ctx, session_id, agent_id.as_str(), task, &content).await?;
                    return Ok(ToolOutput::from_json(&payload));
                }

                let payload = json!({
                    "agent": agent_id.as_str(),
                    "result": content,
                    "tokens_used": tokens,
                });
                Ok(ToolOutput::from_json(&payload))
            }
            SessionReply::Busy { .. } => Err(fail("子智能体会话忙碌，请稍后重试".into())),
            SessionReply::Cancelled => Err(fail("子智能体会话被取消".into())),
            SessionReply::Error { message } => Err(fail(format!("子智能体执行出错: {message}"))),
            SessionReply::Unhandled { reason } => {
                Err(fail(format!("子智能体无法处理该消息: {reason}")))
            }
        }
    }
}

impl DelegateAgentTool {
    /// 将委派结果写入调用者的工件板，返回 `artifact_id` 引用载荷。
    async fn store_artifact(
        &self,
        ctx: &ToolContext,
        child_session: uuid::Uuid,
        agent_id: &str,
        task: &str,
        content: &str,
    ) -> Result<Value, ToolError> {
        use referee_agent::artifact::{Artifact, ArtifactStore, StoreError};

        let store = self.federation.artifact_store();
        let board = store
            .ensure_board(ctx.session_id)
            .await
            .map_err(|e| ToolError::Execution(format!("创建成果板失败: {e}")))?;
        let artifact = Artifact::new(
            board,
            child_session,
            format!("{DELEGATE_AGENT_TOOL_NAME}:{agent_id}"),
            task.to_string(),
            "text/plain",
            content.as_bytes().to_vec(),
        );
        match store.store(artifact).await {
            Ok(artifact_id) => Ok(json!({
                "agent": agent_id,
                "result_ref": { "artifact_id": artifact_id },
            })),
            Err(StoreError::CapacityExceeded) => Err(ToolError::Execution(
                "成果板已满（容量上限），无法落库委派结果".into(),
            )),
            Err(e) => Err(ToolError::Execution(format!("成果落库失败: {e}"))),
        }
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::approval::Approver;
    use crate::llm_config::model::LlmConfig;

    use super::super::agent_reporter::AgentReporter;
    use super::super::federation::{DelegationTracker, Federation};

    struct NoopApprover;

    #[async_trait::async_trait]
    impl Approver for NoopApprover {
        async fn approve(&self, _tool_name: &str, _input: &Value) -> Result<(), ToolError> {
            Ok(())
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

    /// 空事件上报器（不发任何 Tauri 事件）。
    fn noop_reporter() -> Arc<AgentReporter> {
        Arc::new(AgentReporter::new(
            |_, _| {},
            std::sync::Arc::new(DelegationTracker::new()),
        ))
    }

    /// 构建零子代理的联邦：启用清单过滤后为空，不触及任何 LLM provider。
    async fn empty_federation() -> Arc<Federation> {
        let dir = std::env::temp_dir().join(format!("fluen_delegate_{}", uuid::Uuid::new_v4()));
        Federation::build(
            &LlmConfig::default(),
            &dir.to_string_lossy(),
            Arc::new(NoopApprover),
            &["__none__".to_string()],
            0,
            std::sync::Arc::new(DelegationTracker::new()),
            None,
        )
        .await
        .unwrap()
        .into()
    }

    async fn execute_with(args: Value) -> ToolError {
        let tool = DelegateAgentTool::new(Vec::new(), noop_reporter(), empty_federation().await);
        tool.execute(ctx(), args).await.unwrap_err()
    }

    async fn execute_enabled_with(enabled: Vec<String>, args: Value) -> ToolError {
        let tool = DelegateAgentTool::new(enabled, noop_reporter(), empty_federation().await);
        tool.execute(ctx(), args).await.unwrap_err()
    }

    #[tokio::test]
    async fn missing_task_rejected_before_kernel_access() {
        let err = execute_with(json!({ "agent_id": "academic_writer" })).await;
        assert!(matches!(err, ToolError::InvalidArguments(_)), "{err:?}");
    }

    #[tokio::test]
    async fn missing_agent_id_rejected() {
        let err = execute_with(json!({ "task": "x" })).await;
        assert!(matches!(err, ToolError::InvalidArguments(_)), "{err:?}");
    }

    #[tokio::test]
    async fn unknown_agent_rejected() {
        let err = execute_with(json!({ "agent_id": "nope", "task": "x" })).await;
        assert!(matches!(err, ToolError::InvalidArguments(_)), "{err:?}");
        assert!(err.to_string().contains("未知 agent_id"));
    }

    #[tokio::test]
    async fn disabled_agent_rejected_before_rpc() {
        let err = execute_enabled_with(
            vec!["data_analyst".to_string()],
            json!({ "agent_id": "academic_writer", "task": "x" }),
        )
        .await;
        // 启用清单检查先于联邦查找
        assert!(err.to_string().contains("已被禁用"), "{err:?}");
    }

    #[tokio::test]
    async fn unregistered_agent_reports_federation_miss_without_kernel() {
        // 联邦中无任何子代理：应在访问内核前报「未在联邦中注册」
        let err = execute_with(json!({ "agent_id": "academic_writer", "task": "x" })).await;
        assert!(err.to_string().contains("未在联邦中注册"), "{err:?}");
    }

    #[test]
    fn schema_filters_by_enabled_agents() {
        let fed = tokio::runtime::Runtime::new().unwrap().block_on(empty_federation());
        let all = DelegateAgentTool::new(Vec::new(), noop_reporter(), fed.clone());
        let schema = all.input_schema();
        let enum_vals = schema["properties"]["agent_id"]["enum"].as_array().unwrap();
        assert_eq!(enum_vals.len(), 3);

        let restricted = DelegateAgentTool::new(
            vec!["data_analyst".to_string()],
            noop_reporter(),
            fed,
        );
        let schema = restricted.input_schema();
        let enum_vals = schema["properties"]["agent_id"]["enum"].as_array().unwrap();
        assert_eq!(enum_vals.len(), 1);
        assert_eq!(enum_vals[0], "data_analyst");
    }
}
