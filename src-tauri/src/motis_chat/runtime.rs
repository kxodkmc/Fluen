//! Motis 运行时构建——基于 referee [`FluenRuntime`]。
//!
//! 依据 [`MascotConfig`] + [`MascotData`] + [`LlmConfig`] 装配：
//!
//! 1. 解析 provider/model（优先 Motis 配置，回退 LLM 全局激活项）
//!    并构造 referee [`LLMProvider`](referee_ai::provider::LLMProvider)
//! 2. 组装系统提示词（[`super::prompt::build_system_prompt`]，由 commands 层传入会话）
//! 3. 装配 Motis 总督角色工具集：
//!    - **基础工具**（只读）：`paper_content`（论文内容读取）、`project_file`（项目内文件读写）
//!    - **子智能体委派**：`delegate_agent`（派发任务给子智能体并汇总结果）
//!    写操作经 [`ApprovalGuard`](crate::agent_runtime::approval::ApprovalGuard) 包装
//!
//! ## 总督角色工具策略
//!
//! Motis 自身仅装配**只读工具**与**委派工具**——不直接装配
//! `manuscript`（论文正文写入）等具体执行工具，
//! 这些由子智能体在各自运行时中独立装配。
//!
//! ## 配置控制
//!
//! - `function_calling_enabled`：控制是否装配所有工具（含基础工具与委派工具）
//! - `enabled_agents`：控制哪些子智能体可通过 `delegate_agent` 调度
//!   （空列表时全部可用，设置后仅允许已启用的智能体）

use std::sync::Arc;

use referee_ai::tool::ToolRegistry;

use crate::agent_runtime::approval::{approval_executor, ApprovalGuard, Approver};
use crate::agent_runtime::{FluenRuntime, FluenRuntimeBuilder};
use crate::llm_chat;
use crate::llm_config::model::LlmConfig;
use crate::mascot::model::MascotConfig;

use super::delegate::DelegateAgentTool;
use super::error::MotisChatError;

/// 构建 Motis 运行时。
///
/// # 参数
///
/// - `mascot`: Motis 宠物助手配置（能力开关、provider/model 覆盖、人格）
/// - `llm`: LLM 全局配置（提供商与模型列表）
/// - `project_path`: 当前打开的论文项目路径（可选）。存在且
///   `function_calling_enabled` 时装配项目级工具
/// - `approver`: 工具审批器（写操作弹窗确认，包装 `project_file`）
///
/// # 返回
///
/// 思考模式是否启用（由模型能力决定，供 `ChatOptions` 使用）
/// 与 [`FluenRuntime`]。
pub fn build_runtime(
    mascot: &MascotConfig,
    llm: &LlmConfig,
    project_path: Option<&str>,
    approver: Arc<dyn Approver>,
    reporter: crate::motis_chat::delegate::ToolReporter,
) -> Result<(bool, FluenRuntime), MotisChatError> {
    // 1. 解析 provider 与 model（优先 Motis 配置，回退全局激活项）
    let (provider, model_id) = llm_chat::resolve_provider_model(
        mascot.provider_id.as_deref(),
        mascot.model_id.as_deref(),
        llm,
    )
    .map_err(map_llm_chat_error)?;

    // 模型支持思考时启用思考模式（如 DeepSeek / 智谱深度思考）。
    let thinking_enabled = provider.model_supports_thinking(&model_id);

    // 2. 构造 referee LLMProvider
    let llm_provider = llm_chat::build_llm_provider(provider, &model_id)
        .map_err(map_llm_chat_error)?;

    // 3. 装配 Motis 总督角色工具集
    let mut builder = FluenRuntimeBuilder::new(llm_provider);
    if mascot.function_calling_enabled {
        if let Some(project_path) = project_path {
            let registry = build_motis_tool_registry(mascot, llm, project_path, approver, reporter)?;
            // 审批等待最长 5 分钟，执行器超时取 6 分钟（见 approval_executor）
            builder = builder.with_tools(registry, approval_executor());
        }
    }

    Ok((thinking_enabled, builder.build()))
}

/// 装配 Motis 总督角色的工具集。
///
/// Motis 仅装配只读工具与委派工具：
/// - `paper_content`：论文内容读取（只读）
/// - `project_file`：项目内文件读写（写操作经 ApprovalGuard 包装）
/// - `delegate_agent`：子智能体委派工具
fn build_motis_tool_registry(
    mascot: &MascotConfig,
    llm: &LlmConfig,
    project_path: &str,
    approver: Arc<dyn Approver>,
    reporter: crate::motis_chat::delegate::ToolReporter,
) -> Result<ToolRegistry, MotisChatError> {
    let registry = ToolRegistry::with_defaults();

    // 只读：论文内容读取（全文 / 大纲 / 章节）
    registry
        .register(Arc::new(crate::agent_tools::paper::PaperContentTool::new(
            project_path.to_string(),
        )))
        .map_err(|e| MotisChatError::Config(format!("工具注册失败: {e}")))?;

    // 读写：项目内文件（ApprovalGuard 包装写操作）
    registry
        .register(Arc::new(ApprovalGuard::new(
            Arc::new(crate::agent_tools::file::ProjectFileTool::new(
                project_path.to_string(),
            )),
            approver.clone(),
        )))
        .map_err(|e| MotisChatError::Config(format!("工具注册失败: {e}")))?;

    // 子智能体委派工具（总督核心能力，根据 enabled_agents 过滤）
    registry
        .register(Arc::new(DelegateAgentTool::new(
            llm.clone(),
            project_path.to_string(),
            approver,
            mascot.enabled_agents.clone(),
            reporter,
        )))
        .map_err(|e| MotisChatError::Config(format!("工具注册失败: {e}")))?;

    tracing::debug!(
        function_calling = mascot.function_calling_enabled,
        agents_enabled = ?mascot.enabled_agents,
        "Motis 总督工具集装配完成"
    );

    Ok(registry)
}

/// 将 [`LlmChatError`](crate::llm_chat::LlmChatError) 映射为 [`MotisChatError`]。
fn map_llm_chat_error(e: crate::llm_chat::LlmChatError) -> MotisChatError {
    match &e {
        crate::llm_chat::LlmChatError::Config(msg) => {
            tracing::error!("Motis LLM 配置错误: {msg}")
        }
        crate::llm_chat::LlmChatError::NoProvider => {
            tracing::error!("Motis 未配置可用的 LLM 提供商")
        }
    }
    match e {
        crate::llm_chat::LlmChatError::Config(msg) => MotisChatError::Config(msg),
        crate::llm_chat::LlmChatError::NoProvider => MotisChatError::NoProvider,
    }
}
