//! Motis 运行时构建——基于 referee [`FluenRuntime`]。
//!
//! 依据 [`MascotConfig`] + [`MascotData`] + [`LlmConfig`] 装配：
//!
//! 1. 解析 provider/model（优先 Motis 配置，回退 LLM 全局激活项）
//!    并构造 referee [`LLMProvider`](referee_ai::provider::LLMProvider)
//! 2. 组装系统提示词（[`super::prompt::build_system_prompt`]，由 commands 层传入会话）
//! 3. 装配 Motis 总督角色工具集：
//!    - **基础工具**：`paper_outline` / `paper_section`（论文大纲与章节读取，只读）、
//!      `project_read`（项目内文件读取，只读）、`project_write` / `project_edit`
//!      （项目内文件写入/编辑，ApprovalGuard 包装）
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

use referee_ai::provider::ToolDeclaration;
use referee_ai::tool::ToolRegistry;

use crate::agent_runtime::approval::{approval_executor, Approver};
use crate::agent_runtime::{FluenRuntime, FluenRuntimeBuilder};
use crate::llm_chat;
use crate::llm_config::model::LlmConfig;
use crate::mascot::model::MascotConfig;

use super::agent_reporter::AgentReporter;
use super::artifact_store::ProjectArtifactStore;
use super::delegate::DelegateAgentTool;
use super::error::MotisChatError;
use super::federation::FederationPool;
use super::timeouts::{
    motis_engine_config, LLM_HTTP_TIMEOUT, MOTIS_AWAITING_TIMEOUT, MOTIS_TOOL_TIMEOUT,
};
use crate::mcp_host::socstat::SocstatMcpHost;

/// Motis 运行时装配产物——回合所需的一切上下文。
///
/// 除运行时本体外，携带工具声明清单与模型规格，
/// 供 commands 层构建上下文用量报告（[`super::context_usage`]）。
pub struct MotisRuntimeBundle {
    /// 思考模式是否启用（由模型能力决定，供 `ChatOptions` 使用）。
    pub thinking_enabled: bool,
    /// referee 运行时。
    pub runtime: FluenRuntime,
    /// 项目成果板（工具不可用时为 `None`，供系统提示词注入成果清单）。
    pub board_store: Option<Arc<ProjectArtifactStore>>,
    /// 主会话工具声明清单（工具不可用时为空）。
    pub tool_declarations: Vec<ToolDeclaration>,
    /// 模型上下文窗口（token）。
    pub context_window: usize,
    /// 模型最大输出预留（token）。
    pub max_output_tokens: usize,
}

/// 构建 Motis 运行时。
///
/// # 参数
///
/// - `mascot`: Motis 宠物助手配置（能力开关、provider/model 覆盖、人格）
/// - `llm`: LLM 全局配置（提供商与模型列表）
/// - `project_path`: 当前打开的论文项目路径（可选）。存在且
///   `function_calling_enabled` 时装配项目级工具
/// - `approver`: 工具审批器（写操作弹窗确认，包装 `project_write` /
///   `project_edit`）
/// - `federation_pool`: 子智能体联邦槽（确保委派目标就绪并注入内核）
/// - `socstat`: socstat MCP 统计服务器宿主（注入 `data_analyst`
///   工具集；`None` 或连接失败时数据分析通道降级为无统计工具）
///
/// # 返回
///
/// [`MotisRuntimeBundle`]——思考模式开关、[`FluenRuntime`]、项目成果板
/// （工具不可用时为 `None`）、工具声明清单与模型规格。
pub async fn build_runtime(
    mascot: &MascotConfig,
    llm: &LlmConfig,
    project_path: Option<&str>,
    approver: Arc<dyn Approver>,
    read_tracker: Arc<crate::agent_tools::project::read_state::ReadTracker>,
    reporter: Arc<AgentReporter>,
    federation_pool: &FederationPool,
    socstat: Option<SocstatMcpHost>,
) -> Result<MotisRuntimeBundle, MotisChatError> {
    // 1. 解析 provider 与 model（优先 Motis 配置，回退全局激活项）
    let (provider, model_id) = llm_chat::resolve_provider_model(
        mascot.provider_id.as_deref(),
        mascot.model_id.as_deref(),
        llm,
    )
    .map_err(map_llm_chat_error)?;

    // 模型支持思考时启用思考模式（如 DeepSeek / 智谱深度思考）。
    let thinking_enabled = provider.model_supports_thinking(&model_id);

    // 模型规格（上下文窗口 / 最大输出）——与 provider 构造同源解析，
    // 供上下文用量报告作分母
    let (context_window, max_output_tokens) = llm_chat::resolve_model_spec(provider, &model_id);

    // 2. 构造 referee LLMProvider（HTTP 兜底超时大于引擎单轮超时）
    let llm_provider = llm_chat::build_llm_provider_with_timeout(
        provider,
        &model_id,
        LLM_HTTP_TIMEOUT,
    )
    .map_err(map_llm_chat_error)?;

    // 3. 装配 Motis 总督角色工具集（引擎单轮 LLM 超时放宽至 5 分钟）
    let mut builder = FluenRuntimeBuilder::new(llm_provider)
        .with_config(motis_engine_config(MOTIS_AWAITING_TIMEOUT));
    let mut board_store = None;
    let mut tool_declarations = Vec::new();
    if mascot.function_calling_enabled {
        if let Some(project_path) = project_path {
            // 确保子智能体联邦就绪（按指纹复用/重建），并把内核注入执行器，
            // 使 delegate_agent 的 `ctx.kernel` 对等 RPC 可用
            let federation = federation_pool
                .get_or_build(
                    llm,
                    project_path,
                    approver.clone(),
                    read_tracker.clone(),
                    &mascot.enabled_agents,
                    Some(reporter.clone()),
                    socstat,
                )
                .await
                .map_err(|e| MotisChatError::Runtime(e.to_string()))?;
            let registry = build_motis_tool_registry(
                mascot,
                project_path,
                approver,
                read_tracker,
                reporter,
                &federation,
            )?;
            // 声明快照须在 registry 移入 builder 前导出（观测包装不改变声明内容）
            tool_declarations = registry.declarations();
            // 执行器超时须大于委派 RPC 超时（600s），否则会吞掉 RPC 的
            // 明确超时错误并导致子会话追踪泄漏（见 timeouts 分层）
            builder = builder.with_tools(
                registry,
                approval_executor(MOTIS_TOOL_TIMEOUT).with_kernel(federation.kernel().clone()),
            );
            board_store = Some(federation.artifact_store());
        }
    }

    Ok(MotisRuntimeBundle {
        thinking_enabled,
        runtime: builder.build(),
        board_store,
        tool_declarations,
        context_window,
        max_output_tokens,
    })
}

/// 装配 Motis 总督角色的工具集。
///
/// Motis 仅装配只读工具与委派工具：
/// - `paper_outline` / `paper_section`：论文大纲与章节读取（只读）
/// - `project_read`：项目内文件读取（只读）
/// - `project_write` / `project_edit`：项目内文件写入/编辑（ApprovalGuard 包装）
/// - `list_my_board` / `read_artifact`：项目成果板读取（成果以 artifact_id
///   落库后，总督可跨轮取回原文；板随项目持久化）
/// - `delegate_agent`：子智能体委派工具（内核协议）
fn build_motis_tool_registry(
    mascot: &MascotConfig,
    project_path: &str,
    approver: Arc<dyn Approver>,
    read_tracker: Arc<crate::agent_tools::project::read_state::ReadTracker>,
    reporter: Arc<AgentReporter>,
    federation: &Arc<super::federation::Federation>,
) -> Result<ToolRegistry, MotisChatError> {
    use referee_agent::tool::{ArtifactReader, ListMyBoard};

    let registry = ToolRegistry::with_defaults();
    let reg_err = |e: referee_ai::tool::RegistryError| {
        MotisChatError::Config(format!("工具注册失败: {e}"))
    };

    // 只读：论文大纲与章节读取
    crate::agent_tools::assemble::register_paper_readers(
        &registry,
        project_path,
        read_tracker.clone(),
    )
    .map_err(reg_err)?;

    // 读写：项目内文件三件套（只读带读取记账；写/编辑经读门 + ApprovalGuard 包装）
    crate::agent_tools::assemble::register_project_files(
        &registry,
        project_path,
        approver.clone(),
        read_tracker,
    )
    .map_err(reg_err)?;

    // 成果板读取：委派大结果以 artifact_id 返回时，总督可自主取回原文
    let store = federation.artifact_store();
    registry
        .register(Arc::new(ListMyBoard::new(store.clone())))
        .map_err(reg_err)?;
    registry
        .register(Arc::new(ArtifactReader::new(store)))
        .map_err(reg_err)?;

    // 子智能体委派工具（总督核心能力；目标运行时由联邦长寿命持有）
    registry
        .register(Arc::new(DelegateAgentTool::new(
            mascot.enabled_agents.clone(),
            reporter.clone(),
            federation.clone(),
        )))
        .map_err(reg_err)?;

    tracing::debug!(
        function_calling = mascot.function_calling_enabled,
        agents_enabled = ?mascot.enabled_agents,
        "Motis 总督工具集装配完成"
    );

    // 整体观测包装：主会话工具执行结果（含审批通过后的写入成败）
    // 经 reporter 以 `motis:tool-result` 回填前端时间线
    Ok(crate::agent_runtime::observability::observe_registry(
        &registry,
        reporter as Arc<dyn crate::agent_runtime::observability::ToolEventSink>,
    ))
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
