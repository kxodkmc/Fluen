//! referee 运行时构建——学术助手专属装配。
//!
//! 与 Motis 宠物助手的运行时不同，学术助手：
//!
//! 1. **模型来源**：直接使用 LLM 全局激活项（不依赖宠物助手配置）
//! 2. **提示词系统**：学术写作 profile（fluen-markup 规范 + 撰写流程），
//!    由 [`super::prompt::build_system_prompt`] 组装后经
//!    `ChatOptions::system_prompt` 传入
//! 3. **工具集**：只装配与论文写作相关的工具——
//!    - **不直接装配** referee 内置文件工具（其接受任意绝对路径、
//!      无项目相对路径门面；改为组合 referee 原语并经路径安全层暴露）
//!    - `paper_outline` / `paper_section`：读取论文大纲 / 指定章节
//!      （只读，文件读取经 referee `read`）
//!    - `literature_search`：检索文献知识库（只读，混合检索 top4，
//!      知识库存在时装配）
//!    - `manuscript`：写入论文正文（格式校验 + 章节同步，需审批；
//!      正文唯一写入通道）
//!    - `project_read`：读取项目内通用文件（只读，字符窗口续读）
//!    - `project_write` / `project_edit`：写入/编辑项目内非正文文件
//!      （referee 原子写与唯一匹配编辑，需审批；正文 main.md 写保护）
//! 4. **审批机制**：写工具经 `ApprovalGuard`（agent_runtime::approval）包装，
//!    调用前弹窗征求用户确认——包装发生在共享装配层
//!    [`agent_tools::assemble`](crate::agent_tools::assemble)

use std::sync::Arc;

use referee_ai::tool::ToolRegistry;

use crate::agent_runtime::approval::{approval_executor, Approver};
use crate::agent_runtime::{FluenRuntime, FluenRuntimeBuilder};
use crate::llm_chat;
use crate::llm_config::model::LlmConfig;

use super::error::AiAssistantError;

/// 构建学术助手运行时。
///
/// # 参数
///
/// - `llm`: LLM 全局配置（直接使用全局激活项，无智能体级覆盖）
/// - `project_path`: 当前打开的论文项目路径（可选）。存在时装配论文写作
///   工具；未打开项目时仅提供纯对话能力（可输出草稿，不落盘）
/// - `approver`: 工具审批器（写操作弹窗确认，包装 `manuscript` /
///   `project_write` / `project_edit`）
///
/// # 返回
///
/// 思考模式是否启用（由模型能力决定，供 `ChatOptions` 使用）
/// 与 [`FluenRuntime`]。
pub fn build_runtime(
    llm: &LlmConfig,
    project_path: Option<&str>,
    approver: Arc<dyn Approver>,
    read_tracker: Arc<crate::agent_tools::project::read_state::ReadTracker>,
) -> Result<(bool, FluenRuntime), AiAssistantError> {
    // 1. 解析 provider 与 model（学术助手直接用全局激活项）
    let (provider, model_id) =
        llm_chat::resolve_provider_model(None, None, llm).map_err(map_llm_chat_error)?;

    // 模型支持思考时启用思考模式（与 Motis 一致，由模型能力驱动）
    let thinking_enabled = provider.model_supports_thinking(&model_id);

    // 2. 构造 referee LLMProvider
    let llm_provider =
        llm_chat::build_llm_provider(provider, &model_id).map_err(map_llm_chat_error)?;

    // 3. 有打开的项目时，装配论文写作工具
    let mut builder = FluenRuntimeBuilder::new(llm_provider);
    if let Some(project_path) = project_path {
        let registry = build_tool_registry(project_path, llm, approver, read_tracker)?;
        // 审批等待最长 5 分钟，执行器超时须大于该上限（见 approval_executor）
        builder = builder.with_tools(
            registry,
            approval_executor(crate::agent_runtime::approval::APPROVAL_EXECUTOR_TIMEOUT),
        );
    }

    Ok((thinking_enabled, builder.build()))
}

/// 装配论文写作工具集（只读带读取记账；写操作写前必读门 + ApprovalGuard 包装）。
fn build_tool_registry(
    project_path: &str,
    llm: &LlmConfig,
    approver: Arc<dyn Approver>,
    read_tracker: Arc<crate::agent_tools::project::read_state::ReadTracker>,
) -> Result<ToolRegistry, AiAssistantError> {
    let registry = ToolRegistry::with_defaults();
    let reg_err = |e: referee_ai::tool::RegistryError| {
        AiAssistantError::Config(format!("工具注册失败: {e}"))
    };

    // 只读：论文大纲与章节读取
    crate::agent_tools::assemble::register_paper_readers(&registry, project_path)
        .map_err(reg_err)?;

    // 只读：文献知识库搜索（知识库存在时装配；缺失/打开失败时降级跳过）
    crate::agent_tools::assemble::register_literature_search(&registry, project_path, llm)
        .map_err(reg_err)?;

    // 写操作：论文正文写入（格式校验 + 章节同步，读门 + ApprovalGuard 包装）
    crate::agent_tools::assemble::register_manuscript(
        &registry,
        project_path,
        approver.clone(),
        read_tracker.clone(),
    )
    .map_err(reg_err)?;

    // 读写：项目内文件三件套（写/编辑经读门与审批；正文 main.md 写保护）
    crate::agent_tools::assemble::register_project_files(
        &registry,
        project_path,
        approver,
        read_tracker,
    )
    .map_err(reg_err)?;

    Ok(registry)
}

/// 将 [`LlmChatError`](crate::llm_chat::LlmChatError) 映射为 [`AiAssistantError`]。
fn map_llm_chat_error(e: crate::llm_chat::LlmChatError) -> AiAssistantError {
    match &e {
        crate::llm_chat::LlmChatError::Config(msg) => {
            tracing::error!("学术助手 LLM 配置错误: {msg}")
        }
        crate::llm_chat::LlmChatError::NoProvider => {
            tracing::error!("学术助手未配置可用的 LLM 提供商")
        }
    }
    match e {
        crate::llm_chat::LlmChatError::Config(msg) => AiAssistantError::Config(msg),
        crate::llm_chat::LlmChatError::NoProvider => AiAssistantError::NoProvider,
    }
}
