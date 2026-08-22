//! referee 运行时构建——学术助手专属装配。
//!
//! 与 Motis 宠物助手的运行时不同，学术助手：
//!
//! 1. **模型来源**：直接使用 LLM 全局激活项（不依赖宠物助手配置）
//! 2. **提示词系统**：学术写作 profile（fluen-markup 规范 + 撰写流程），
//!    由 [`super::prompt::build_system_prompt`] 组装后经
//!    `ChatOptions::system_prompt` 传入
//! 3. **工具集**：只装配与论文写作相关的工具——
//!    - **不装配** referee 内置工具（其只读工具接受任意绝对路径、
//!      无项目根约束；项目内读取由 `project_file` 的 read 提供
//!      并受路径安全校验保护）
//!    - `paper_content`：读取论文全文 / 大纲 / 章节（只读）
//!    - `literature_search`：检索文献知识库（只读，混合检索 top4，
//!      知识库存在时装配）
//!    - `manuscript`：写入论文正文（格式校验 + 章节同步，需审批）
//!    - `project_file`：读写项目内文件（路径限制在项目根内，写操作需审批）
//! 4. **审批机制**：写工具经 [`ApprovalGuard`] 包装，调用前弹窗征求用户确认

use std::sync::Arc;

use referee_ai::tool::ToolRegistry;

use crate::agent_runtime::approval::{approval_executor, ApprovalGuard, Approver};
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
///   `project_file`）
///
/// # 返回
///
/// 思考模式是否启用（由模型能力决定，供 `ChatOptions` 使用）
/// 与 [`FluenRuntime`]。
pub fn build_runtime(
    llm: &LlmConfig,
    project_path: Option<&str>,
    approver: Arc<dyn Approver>,
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
        let registry = build_tool_registry(project_path, llm, approver)?;
        // 审批等待最长 5 分钟，执行器超时取 6 分钟（见 approval_executor）
        builder = builder.with_tools(registry, approval_executor());
    }

    Ok((thinking_enabled, builder.build()))
}

/// 装配论文写作工具集（只读直装，写操作 ApprovalGuard 包装）。
fn build_tool_registry(
    project_path: &str,
    llm: &LlmConfig,
    approver: Arc<dyn Approver>,
) -> Result<ToolRegistry, AiAssistantError> {
    let registry = ToolRegistry::with_defaults();

    // 只读：论文内容读取（全文 / 大纲 / 章节）
    registry
        .register(Arc::new(crate::agent_tools::paper::PaperContentTool::new(
            project_path.to_string(),
        )))
        .map_err(|e| AiAssistantError::Config(format!("工具注册失败: {e}")))?;

    // 只读：文献知识库搜索（知识库存在时装配；注入 embedding 以启用语义
    // 检索，未配置 embedding 时由 fluen-knowledge 自动降级为关键词检索）
    register_literature_search(&registry, project_path, llm)?;

    // 写操作：论文正文写入（格式校验 + 章节同步，ApprovalGuard 包装）
    registry
        .register(Arc::new(ApprovalGuard::new(
            Arc::new(crate::agent_tools::manuscript::ManuscriptEditTool::new(
                project_path.to_string(),
            )),
            approver.clone(),
        )))
        .map_err(|e| AiAssistantError::Config(format!("工具注册失败: {e}")))?;

    // 读写：项目内文件（路径限制在项目根内，写操作经审批）
    registry
        .register(Arc::new(ApprovalGuard::new(
            Arc::new(crate::agent_tools::file::ProjectFileTool::new(
                project_path.to_string(),
            )),
            approver,
        )))
        .map_err(|e| AiAssistantError::Config(format!("工具注册失败: {e}")))?;

    Ok(registry)
}

/// 知识库存在时注册 `literature_search` 工具。
///
/// 打开失败（索引损坏等）仅告警降级，不阻断助手启动。
fn register_literature_search(
    registry: &ToolRegistry,
    project_path: &str,
    llm: &LlmConfig,
) -> Result<(), AiAssistantError> {
    let references_dir = std::path::PathBuf::from(project_path).join("references");
    if !references_dir.join("wiki").join("index.db").is_file() {
        return Ok(());
    }

    let kb = match fluen_knowledge::async_kb::AsyncKnowledgeBase::open(&references_dir) {
        Ok(kb) => kb,
        Err(e) => {
            tracing::warn!(
                references_dir = %references_dir.display(),
                "文献知识库打开失败，跳过 literature_search 工具: {e}"
            );
            return Ok(());
        }
    };

    // 注入 embedding router 启用语义检索（未配置时降级关键词检索）
    let kb = match crate::builtin_providers::embedding::build_embedding_router(llm) {
        Some(router) => kb.with_embedding_provider(Arc::new(router)),
        None => kb,
    };

    registry
        .register(Arc::new(crate::agent_tools::literature::LiteratureSearchTool::new(
            kb,
        )))
        .map_err(|e| AiAssistantError::Config(format!("工具注册失败: {e}")))?;

    Ok(())
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
