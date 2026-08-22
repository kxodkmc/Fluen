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
//! ## 执行流程
//!
//! 1. Motis 的 LLM 决定调用 `delegate_agent`，传入 `agent_id` 与 `task`
//! 2. 工具构建子智能体运行时（专属 prompt + 工具集）
//! 3. 子智能体执行 `task`（内部可能多轮工具调用）
//! 4. `wait=true` 时同步返回子智能体的最终回复
//! 5. Motis 拿到结果后汇总回复用户
//!
//! ## 与 confluent SubAgentTool 的差异
//!
//! 本工具不依赖 confluent 的 `AgentRuntime` / `Kernel` 体系，
//! 而是直接在 `FluenRuntime` 上启动新的 `SessionId` 并执行一轮 chat，
//! 结果以 `ToolOutput` 返回给 Motis 的引擎（工具结果自动注入 Motis 上下文）。

use std::sync::Arc;

use async_trait::async_trait;
use referee_ai::engine::{ChatHandle, EngineReply};
use referee_ai::provider::ThinkingConfig;
use referee_ai::session::{ChatOptions, ChatPayload, SessionId};
use referee_ai::tool::{Tool, ToolCategory, ToolContext, ToolError, ToolOutput};
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::agent_runtime::approval::Approver;
use crate::llm_config::model::LlmConfig;

use super::agents::{self, AgentId};

/// 工具名称。
pub const DELEGATE_AGENT_TOOL_NAME: &str = "delegate_agent";

/// 工具描述。
const DESCRIPTION: &str = "将任务派发给子智能体执行。Motis（总督角色）自身不负责编写、计算等具体工作，而是通过此工具将任务委派给合适的子智能体（如学术撰写助手、知识库构建助手、数据分析助手），子智能体独立执行后返回结果。";

/// 工具执行结果上报器——把工具返回结果透传给调用方（如前端事件）。
#[derive(Clone)]
pub struct ToolReporter {
    emit: Arc<dyn Fn(&str, &str, &serde_json::Value) + Send + Sync>,
}

impl ToolReporter {
    /// 构造带回调的上报器。
    pub fn new<F>(emit: F) -> Self
    where
        F: Fn(&str, &str, &serde_json::Value) + Send + Sync + 'static,
    {
        Self {
            emit: Arc::new(emit),
        }
    }

    /// 上报一次工具结果（`tool_call_id` / `tool_name` / `result`）。
    pub fn report(&self, tool_call_id: &str, tool_name: &str, result: &serde_json::Value) {
        (self.emit)(tool_call_id, tool_name, result);
    }
}

/// 子智能体委派工具。
///
/// 每次调用动态构建目标子智能体的运行时（独立的 LLM 会话 + 专属工具集），
/// 执行一轮非流式 chat 并返回结果。
pub struct DelegateAgentTool {
    /// 输入参数 JSON Schema。
    parameters: Value,
    /// LLM 全局配置（构建子智能体运行时时使用）。
    llm: LlmConfig,
    /// 当前论文项目路径。
    project_path: String,
    /// 工具审批器（传递给子智能体的写操作工具）。
    approver: Arc<dyn Approver>,
    /// 已启用的子智能体 ID 列表（空列表 = 全部可用）。
    enabled_agents: Vec<String>,
    /// 工具执行结果上报器（透传委派结果给上层/前端）。
    reporter: ToolReporter,
    /// 活跃的子智能体句柄映射（供取消使用）。
    handles: Arc<Mutex<Vec<(SessionId, ChatHandle)>>>,
}

impl DelegateAgentTool {
    /// 构造委派工具。
    ///
    /// `enabled_agents` 为空列表时全部子智能体可用；
    /// 非空时仅允许列表中的 ID 被调度。
    pub fn new(
        llm: LlmConfig,
        project_path: String,
        approver: Arc<dyn Approver>,
        enabled_agents: Vec<String>,
        reporter: ToolReporter,
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
                    "description": "是否同步等待子智能体完成（默认 true）。false 时异步派发，结果在下一回合注入"
                }
            },
            "required": ["agent_id", "task"]
        });

        Self {
            parameters,
            llm,
            project_path,
            approver,
            enabled_agents,
            reporter,
            handles: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// 返回可用的子智能体列表（供 Motis 提示词注入）。
    ///
    /// 根据 `enabled_agents` 过滤：空列表 = 全部可用。
    pub fn available_agents_description(&self) -> String {
        agents::enabled_agents_description(&self.enabled_agents)
    }

    /// 取消所有活跃的子智能体会话。
    pub async fn cancel_all(&self) {
        let mut handles = self.handles.lock().await;
        for (_, handle) in handles.drain(..) {
            handle.cancel();
        }
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

    /// 委派是内部调用（子智能体在引擎内运行，不占用外部 IO 槽位）。
    fn category(&self) -> ToolCategory {
        ToolCategory::Local
    }

    /// 委派是同步等待型工具（默认 wait=true）。
    fn default_wait(&self) -> bool {
        true
    }

    /// 子智能体可嵌套调用（但 referee 引擎有深度限制兜底）。
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

        let agent_id = AgentId::from_str(agent_id_str).ok_or_else(|| {
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

        let wait = args
            .get("wait")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        tracing::info!(
            agent_id = %agent_id,
            task_preview = %task.chars().take(80).collect::<String>(),
            wait,
            "Motis 委派任务给子智能体"
        );

        // 2. 构建子智能体运行时
        let (thinking_enabled, runtime) = agents::build_agent_runtime(
            &agent_id,
            &self.llm,
            &self.project_path,
            self.approver.clone(),
        )
        .map_err(|e| ToolError::Execution(format!("构建子智能体运行时失败: {e}")))?;

        // 3. 构建系统提示词
        let system_prompt = agents::agent_system_prompt(&agent_id)
            .ok_or_else(|| ToolError::Execution("无法获取子智能体提示词".into()))?;

        // 4. 启动子智能体会话
        let session_id = SessionId::new_v4();
        let payload = ChatPayload {
            message: referee_ai::provider::Message::user(task.to_string()),
            options: ChatOptions {
                system_prompt: Some(system_prompt),
                thinking: ThinkingConfig {
                    enabled: thinking_enabled,
                    effort: None,
                },
                ..ChatOptions::default()
            },
            peer_depth: ctx.peer_depth + 1,
        };

        let handle = if wait {
            // 同步等待：使用非流式 chat
            runtime.chat(session_id, payload)
        } else {
            // 异步派发：使用流式（但工具层面不消费流，结果在下一回合注入）
            runtime.chat_stream(session_id, payload)
        }
        .map_err(|e| ToolError::Execution(format!("子智能体会话启动失败: {e}")))?;

        // 5. 注册句柄（供取消）
        {
            let mut handles = self.handles.lock().await;
            handles.push((session_id, handle.clone()));
        }

        if !wait {
            // 异步派发：立即返回占位结果
            return Ok(ToolOutput::text(
                "任务已异步派发给子智能体，结果将在下一回合自动注入上下文。"
            ));
        }

        // 6. 同步等待子智能体完成
        let reply = match handle.wait().await {
            Some(reply) => reply,
            None => {
                return Err(ToolError::Execution("子智能体会话被取消".into()));
            }
        };

        // 7. 清理句柄
        {
            let mut handles = self.handles.lock().await;
            handles.retain(|(sid, _)| *sid != session_id);
        }

        match reply {
            EngineReply::Success(resp) => {
                let content = resp.message.content.as_text().unwrap_or_default().to_string();
                let tokens = resp.usage.as_ref().map(|u| u.total_tokens).unwrap_or(0);
                let payload = json!({
                    "agent": agent_id.as_str(),
                    "result": content,
                    "tokens_used": tokens,
                });
                // 透传委派结果给上层/前端（按工具调用 ID 关联）
                self.reporter.report(&ctx.tool_call_id, agent_id.as_str(), &payload);
                Ok(ToolOutput::from_json(&payload))
            }
            EngineReply::Error(err) => {
                self.reporter.report(
                    &ctx.tool_call_id,
                    agent_id.as_str(),
                    &json!({ "error": format!("子智能体执行出错: {err}") }),
                );
                Err(ToolError::Execution(format!("子智能体执行出错: {err}")))
            }
            EngineReply::Cancelled => {
                Err(ToolError::Execution("子智能体会话被取消".into()))
            }
            EngineReply::Timeout => {
                Err(ToolError::Execution("子智能体会话超时".into()))
            }
            EngineReply::Busy { .. } => {
                Err(ToolError::Execution("子智能体会话忙碌".into()))
            }
            EngineReply::Streaming(_) => {
                // 非流式 chat 不应返回 Streaming，稳健处理
                Ok(ToolOutput::text("子智能体已完成（流式结果已收敛）"))
            }
        }
    }
}
