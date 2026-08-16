//! confluent 运行时构建——学术助手专属装配。
//!
//! 与 Motis 宠物助手的运行时不同，学术助手：
//!
//! 1. **模型来源**：直接使用 LLM 全局激活项（不依赖宠物助手配置）
//! 2. **提示词系统**：学术写作 profile（fluen-markup 规范 + 撰写流程）
//! 3. **工具集**：只装配与论文写作相关的工具——
//!    - **不装配** confluent 内置 ToolKit（其只读工具 fs_read_file / fs_search
//!      接受任意绝对路径、无项目根约束；项目内读取由 `project_file` 的 read
//!      提供并受路径安全校验保护）
//!    - `paper_content`：读取论文全文 / 大纲 / 章节
//!    - `literature_search`：检索文献知识库（混合检索，最多 top4，知识库存在时装配）
//!    - `manuscript`：写入论文正文（格式校验 + 章节同步，需审批）
//!    - `project_file`：读写项目内文件（路径限制在项目根内，写操作需审批）
//! 4. **审批扩展**：写操作调用前弹窗征求用户确认

use std::sync::Arc;

use confluent::llmkit::ThinkingMode;
use confluent::{ConfluentRuntime, ConfluentRuntimeBuilder};

use crate::agent_tools::file::ProjectFileToolProvider;
use crate::agent_tools::manuscript::ManuscriptEditToolProvider;
use crate::agent_tools::paper::PaperContentToolProvider;
use crate::llm_chat;
use crate::llm_config::model::LlmConfig;

use super::error::AiAssistantError;
use super::prompt::{
    ACADEMIC_ASSISTANT_PROFILE_ID, AcademicContextInjector, academic_registry,
};

/// 构建学术助手运行时。
///
/// # 参数
///
/// - `llm`: LLM 全局配置（提供商与模型列表）
/// - `project_path`: 当前打开的论文项目路径（可选）。存在时装配论文写作工具；
///   未打开项目时仅提供纯对话能力（可输出草稿，不落盘）
/// - `approval_extension`: 工具审批扩展（写操作需用户确认）
pub async fn build_runtime(
    llm: &LlmConfig,
    project_path: Option<&str>,
    approval_extension: Arc<dyn confluent::agent_runtime::RuntimeExtension>,
) -> Result<ConfluentRuntime, AiAssistantError> {
    // 1. 解析 provider 与 model（学术助手直接用全局激活项）
    let (provider, model_id) = llm_chat::resolve_provider_model(None, None, llm)
        .map_err(map_llm_chat_error)?;

    // 2. 构造 ChatClient
    let chat_client = llm_chat::build_chat_client(provider).map_err(map_llm_chat_error)?;

    // 3. 构造 builder——装配学术写作提示词系统
    let registry = academic_registry();
    let context_injector = Arc::new(AcademicContextInjector::new());

    // 模型支持思考时启用思考模式
    let supports_thinking = provider.model_supports_thinking(&model_id);

    let mut builder = ConfluentRuntimeBuilder::new()
        .with_agent_id(ACADEMIC_ASSISTANT_PROFILE_ID.to_string())
        .with_model(model_id)
        .with_chat_client(Arc::new(chat_client))
        .with_prompt_registry(registry)
        .with_prompt_profile(ACADEMIC_ASSISTANT_PROFILE_ID)
        .with_extension(context_injector as Arc<dyn confluent::agent_runtime::RuntimeExtension>);

    if supports_thinking {
        builder = builder.with_thinking(ThinkingMode::Enabled);
    }

    // 4. 有打开的项目时，装配论文写作工具（不装配内置 ToolKit——其只读工具
    //    无项目根路径约束；项目内读取由 project_file 的 read 提供并受安全校验）
    if let Some(project_path) = project_path {
        // 论文内容读取 + 正文写入 + 项目内文件读写
        builder = builder.with_tool_provider(Arc::new(PaperContentToolProvider::new(
            project_path.to_string(),
        )));
        builder = builder.with_tool_provider(Arc::new(ManuscriptEditToolProvider::new(
            project_path.to_string(),
        )));
        builder = builder.with_tool_provider(Arc::new(ProjectFileToolProvider::new(
            project_path.to_string(),
        )));

        // 文献知识库搜索（知识库存在时装配；注入 embedding 以启用语义检索，
        // 未配置 embedding 时由 fluen-knowledge 自动降级为关键词检索）
        let references_dir = std::path::PathBuf::from(project_path).join("references");
        if references_dir.join("wiki").join("index.db").is_file() {
            match fluen_knowledge::async_kb::AsyncKnowledgeBase::open(&references_dir) {
                Ok(kb) => {
                    let kb =
                        match crate::builtin_providers::embedding::build_embedding_router(llm) {
                            Some(router) => kb.with_embedding_provider(Arc::new(router)),
                            None => kb,
                        };
                    builder = builder.with_tool_provider(Arc::new(
                        crate::agent_tools::literature::LiteratureSearchToolProvider::new(kb),
                    ));
                }
                Err(e) => {
                    tracing::warn!(
                        references_dir = %references_dir.display(),
                        "文献知识库打开失败，跳过 literature_search 工具: {e}"
                    );
                }
            }
        }
    }

    // 5. 注入工具审批扩展（写操作调用前弹窗确认）
    builder = builder.with_extension(approval_extension);

    // 6. 构建运行时
    let runtime = builder.build().await?;
    Ok(runtime)
}

/// 将 [`LlmChatError`](crate::llm_chat::LlmChatError) 映射为 [`AiAssistantError`]。
fn map_llm_chat_error(e: crate::llm_chat::LlmChatError) -> AiAssistantError {
    match e {
        crate::llm_chat::LlmChatError::Config(msg) => AiAssistantError::Config(msg),
        crate::llm_chat::LlmChatError::NoProvider => AiAssistantError::NoProvider,
    }
}
