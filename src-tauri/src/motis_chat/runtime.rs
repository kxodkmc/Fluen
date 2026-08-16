//! confluent 运行时构建。
//!
//! 依据 [`MascotConfig`] + [`MascotData`] + [`LlmConfig`] 构造 [`ConfluentRuntime`]：
//!
//! 1. 解析 provider/model（优先 Motis 配置，回退 LLM 全局激活项）
//! 2. 构造 [`ChatClient`]（LLM 连接层 [`crate::llm_chat`] 提供）
//! 3. 装配模块化提示词系统（PromptRegistry + MotisContextInjector + PromptExtension）
//! 4. 按能力开关装配 MCP / Skills / Toolkit 适配器
//!
//! ## 能力装配策略
//!
//! | 开关 | 装配内容 |
//! |------|----------|
//! | 提示词系统 | 始终装配——Motis prompt profile + 上下文注入器 + PromptExtension |
//! | `mcp_enabled` | 从配置目录加载 `.mcp.json`，装配 MCP 适配器 |
//! | `skills_enabled` | 从配置目录加载 `skills/`，装配 Skills 适配器 |
//! | `function_calling_enabled` | 装配 confluent 内置 ToolKit |
//!
//! **不在本模块硬编码应用操作工具**，所有能力扩展通过 confluent 适配器装配。

use std::sync::Arc;

use confluent::llmkit::ThinkingMode;
use confluent::{ConfluentRuntime, ConfluentRuntimeBuilder, ToolKit};

use crate::llm_chat;
use crate::llm_config::model::LlmConfig;
use crate::mascot::model::{MascotConfig, MascotData};
use crate::platform::fluen_config_dir;

use super::error::MotisChatError;
use super::prompt::{MotisContextInjector, MOTIS_PROFILE_ID, motis_registry};

/// MCP 配置文件名（位于配置目录下）。
const MCP_CONFIG_FILE: &str = ".mcp.json";

/// Skills 目录名（位于配置目录下）。
const SKILLS_DIR_NAME: &str = "skills";

/// 构建 confluent 运行时。
///
/// # 参数
///
/// - `mascot`: Motis 宠物助手配置（能力开关、provider/model 覆盖、人格）
/// - `data`: Motis 宠物运行时数据（心情、好感度）
/// - `llm`: LLM 全局配置（提供商与模型列表）
/// - `project_path`: 当前打开的论文项目路径（可选）。存在时装配
///   [`crate::agent_tools::paper::PaperContentTool`]，供智能体读取论文内容。
///
/// # 流程
///
/// 1. 解析 provider/model（优先 Motis，回退 LLM 全局激活项）
/// 2. 构造 ChatClient
/// 3. 装配模块化提示词系统（PromptRegistry + MotisContextInjector + PromptExtension）
/// 4. 按能力开关装配 MCP / Skills / Toolkit 适配器
/// 5. 注入工具审批扩展（写操作需用户确认）
/// 6. 构建并返回 ConfluentRuntime
pub async fn build_runtime(
    mascot: &MascotConfig,
    data: &MascotData,
    llm: &LlmConfig,
    project_path: Option<&str>,
    approval_extension: Arc<dyn confluent::agent_runtime::RuntimeExtension>,
) -> Result<ConfluentRuntime, MotisChatError> {
    // 1. 解析 provider 与 model
    let (provider, model_id) = llm_chat::resolve_provider_model(
        mascot.provider_id.as_deref(),
        mascot.model_id.as_deref(),
        llm,
    )
    .map_err(map_llm_chat_error)?;

    // 2. 构造 ChatClient
    let chat_client = llm_chat::build_chat_client(provider).map_err(map_llm_chat_error)?;

    // 3. 构造 builder——装配提示词系统
    let registry = motis_registry();
    let context_injector = Arc::new(MotisContextInjector::new(mascot, data));

    // 模型支持思考时启用思考模式（如 DeepSeek / 智谱深度思考）。
    // 由模型能力标志驱动，未来如需用户级开关可在此扩展。
    let supports_thinking = provider.model_supports_thinking(&model_id);

    let mut builder = ConfluentRuntimeBuilder::new()
        .with_agent_id("motis".to_string())
        .with_model(model_id)
        .with_chat_client(Arc::new(chat_client))
        // 装配模块化提示词：注册 PromptRegistry → 构造 DefaultComposer → 包装为 PromptExtension
        .with_prompt_registry(registry)
        // 设置当前 profile id（ProfileInjector 在 pre_run 写入 dynamic_state）
        .with_prompt_profile(MOTIS_PROFILE_ID)
        // 注入 Motis 上下文变量（agent_name / personality_style / mood / affinity 等）
        .with_extension(context_injector as Arc<dyn confluent::agent_runtime::RuntimeExtension>);

    if supports_thinking {
        builder = builder.with_thinking(ThinkingMode::Enabled);
    }

    // 4. 按能力开关装配适配器
    if mascot.mcp_enabled {
        builder = assemble_mcp(builder).await?;
    }
    if mascot.skills_enabled {
        builder = assemble_skills(builder)?;
    }
    if mascot.function_calling_enabled {
        // 内置 ToolKit 过滤：移除 fs_write_file（任意路径写、无审批，会被用来
        // 绕过 project_file 的用户确认）；fs_execute_command 保留但纳入审批扩展
        let mut kit = ToolKit::new();
        for tool in ToolKit::default().into_tools() {
            if tool.schema().name != "fs_write_file" {
                kit = kit.with(tool);
            }
        }
        builder = builder.with_toolkit(kit);
        // 有打开的项目时，装配项目级工具（论文内容读取 + 项目内文件读写）
        if let Some(project_path) = project_path {
            let paper_provider = Arc::new(
                crate::agent_tools::paper::PaperContentToolProvider::new(
                    project_path.to_string(),
                ),
            );
            builder = builder.with_tool_provider(paper_provider);

            let file_provider = Arc::new(
                crate::agent_tools::file::ProjectFileToolProvider::new(
                    project_path.to_string(),
                ),
            );
            builder = builder.with_tool_provider(file_provider);
        }
    }

    // 5. 注入工具审批扩展（project_file 写操作调用前弹窗确认）
    builder = builder.with_extension(approval_extension);

    // 6. 构建运行时
    let runtime = builder.build().await?;
    Ok(runtime)
}

/// 将 [`LlmChatError`](crate::llm_chat::LlmChatError) 映射为 [`MotisChatError`]。
fn map_llm_chat_error(e: crate::llm_chat::LlmChatError) -> MotisChatError {
    match e {
        crate::llm_chat::LlmChatError::Config(msg) => MotisChatError::Config(msg),
        crate::llm_chat::LlmChatError::NoProvider => MotisChatError::NoProvider,
    }
}

/// 装配 MCP 适配器。
///
/// 从配置目录读取 `.mcp.json`，若文件不存在则跳过（不视为错误）。
async fn assemble_mcp(
    builder: ConfluentRuntimeBuilder,
) -> Result<ConfluentRuntimeBuilder, MotisChatError> {
    let config_dir = fluen_config_dir()?;
    let mcp_path = config_dir.join(MCP_CONFIG_FILE);

    if !mcp_path.exists() {
        // MCP 配置文件不存在，跳过装配
        return Ok(builder);
    }

    Ok(builder.with_mcp_from_file(&mcp_path).await?)
}

/// 装配 Skills 适配器。
///
/// 从配置目录读取 `skills/` 子目录，若目录不存在则跳过。
fn assemble_skills(
    builder: ConfluentRuntimeBuilder,
) -> Result<ConfluentRuntimeBuilder, MotisChatError> {
    let config_dir = fluen_config_dir()?;
    let skills_dir = config_dir.join(SKILLS_DIR_NAME);

    if !skills_dir.exists() {
        // Skills 目录不存在，跳过装配
        return Ok(builder);
    }

    Ok(builder.with_skills_from_dir(&skills_dir)?)
}
