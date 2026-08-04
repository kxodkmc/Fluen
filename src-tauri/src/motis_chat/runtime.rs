//! confluent 运行时构建。
//!
//! 依据 [`MascotConfig`] + [`MascotData`] + [`LlmConfig`] 构造 [`ConfluentRuntime`]：
//!
//! 1. 解析 provider/model（优先 Motis 配置，回退 LLM 全局激活项）
//! 2. 构造 [`ChatClient`]（按 provider 的 base_url 情况选择 OpenAI / Anthropic / 双风格）
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

use confluent::llmkit::{
    AnthropicProvider, AnthropicTransformer, ApiStyle, ChatClient, ChatClientConfig,
    DualStyleProvider, OpenAiProvider, OpenAiTransformer, RequestTransformer,
};
use confluent::{ConfluentRuntime, ConfluentRuntimeBuilder, ToolKit};

use crate::llm_config::model::{LlmConfig, ProviderConfig};
use crate::mascot::model::{MascotConfig, MascotData};
use crate::platform::fluen_config_dir;

use super::error::MotisChatError;
use super::prompt::{MotisContextInjector, MOTIS_PROFILE_ID, motis_registry};

/// MCP 配置文件名（位于配置目录下）。
const MCP_CONFIG_FILE: &str = ".mcp.json";

/// Skills 目录名（位于配置目录下）。
const SKILLS_DIR_NAME: &str = "skills";

/// Anthropic API 默认版本头。
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// 构建 confluent 运行时。
///
/// # 参数
///
/// - `mascot`: Motis 宠物助手配置（能力开关、provider/model 覆盖、人格）
/// - `data`: Motis 宠物运行时数据（心情、好感度）
/// - `llm`: LLM 全局配置（提供商与模型列表）
///
/// # 流程
///
/// 1. 解析 provider/model（优先 Motis，回退 LLM 全局激活项）
/// 2. 构造 ChatClient
/// 3. 装配模块化提示词系统（PromptRegistry + MotisContextInjector + PromptExtension）
/// 4. 按能力开关装配 MCP / Skills / Toolkit 适配器
/// 5. 构建并返回 ConfluentRuntime
pub async fn build_runtime(
    mascot: &MascotConfig,
    data: &MascotData,
    llm: &LlmConfig,
) -> Result<ConfluentRuntime, MotisChatError> {
    // 1. 解析 provider 与 model
    let (provider, model_id) = resolve_provider_model(mascot, llm)?;

    // 2. 构造 ChatClient
    let chat_client = build_chat_client(provider)?;

    // 3. 构造 builder——装配提示词系统
    let registry = motis_registry();
    let context_injector = Arc::new(MotisContextInjector::new(mascot, data));

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

    // 4. 按能力开关装配适配器
    if mascot.mcp_enabled {
        builder = assemble_mcp(builder).await?;
    }
    if mascot.skills_enabled {
        builder = assemble_skills(builder)?;
    }
    if mascot.function_calling_enabled {
        builder = builder.with_toolkit(ToolKit::default());
    }

    // 5. 构建运行时
    let runtime = builder.build().await?;
    Ok(runtime)
}

/// 解析 provider 与 model。
///
/// 优先使用 [`MascotConfig`] 中的 `provider_id` / `model_id`，
/// 回退到 [`LlmConfig`] 的全局激活项。
fn resolve_provider_model<'a>(
    mascot: &MascotConfig,
    llm: &'a LlmConfig,
) -> Result<(&'a ProviderConfig, String), MotisChatError> {
    // 优先 Motis 配置
    if let (Some(pid), Some(mid)) = (&mascot.provider_id, &mascot.model_id) {
        if let Some(provider) = llm.find_provider(pid) {
            if provider.find_model(mid).is_some() {
                return Ok((provider, mid.clone()));
            }
        }
        return Err(MotisChatError::Config(format!(
            "Motis 配置的提供商/模型不存在: {}/{}",
            pid, mid
        )));
    }

    // 回退 LLM 全局激活项
    match (llm.active_provider(), llm.active_model()) {
        (Some(provider), Some(model)) => Ok((provider, model.id.clone())),
        _ => Err(MotisChatError::NoProvider),
    }
}

/// OpenAI 风格端点的路径后缀。
const OPENAI_CHAT_PATH: &str = "/chat/completions";

/// Anthropic 风格端点的路径后缀。
const ANTHROPIC_MESSAGES_PATH: &str = "/v1/messages";

/// 将 base URL 规范化为完整的 OpenAI Chat Completions 端点。
///
/// 用户配置（onboarding 或手动填写）中的 `openai_base_url` 可能是 base URL
/// （如 `https://api.stepfun.com/v1`），也可能是完整端点
/// （如 `https://api.stepfun.com/v1/chat/completions`）。
/// 本函数确保最终 URL 以 `/chat/completions` 结尾。
fn normalize_openai_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with(OPENAI_CHAT_PATH) {
        trimmed.to_string()
    } else {
        format!("{trimmed}{OPENAI_CHAT_PATH}")
    }
}

/// 将 base URL 规范化为完整的 Anthropic Messages 端点。
///
/// 确保最终 URL 以 `/v1/messages` 结尾。
fn normalize_anthropic_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with(ANTHROPIC_MESSAGES_PATH) {
        trimmed.to_string()
    } else {
        format!("{trimmed}{ANTHROPIC_MESSAGES_PATH}")
    }
}

/// 构造 [`ChatClient`]。
///
/// 按提供商配置的 base_url 情况选择 Provider：
/// - 同时有 `openai_base_url` 和 `anthropic_base_url` → [`DualStyleProvider`]
/// - 仅有 `openai_base_url` → [`OpenAiProvider`]
/// - 仅有 `anthropic_base_url` → [`AnthropicProvider`]
///
/// base_url 会被自动规范化为完整端点（补全 `/chat/completions` 或 `/v1/messages`）。
fn build_chat_client(provider: &ProviderConfig) -> Result<ChatClient, MotisChatError> {
    let api_key = provider.api_key.as_ref().ok_or_else(|| {
        MotisChatError::Config(format!("提供商 {} 未配置 api_key", provider.id))
    })?;

    let http_client = reqwest::Client::new();
    let config = ChatClientConfig {
        default_style: provider.default_style,
        ..Default::default()
    };

    let has_openai = provider.openai_base_url.is_some();
    let has_anthropic = provider.anthropic_base_url.is_some();
    let extra_headers = build_extra_headers(provider);

    if has_openai && has_anthropic {
        // 双风格 Provider
        let openai_endpoint =
            normalize_openai_endpoint(provider.openai_base_url.as_ref().unwrap());
        let anthropic_endpoint =
            normalize_anthropic_endpoint(provider.anthropic_base_url.as_ref().unwrap());
        let dual = DualStyleProvider::new(
            http_client,
            api_key.clone(),
            openai_endpoint,
            anthropic_endpoint,
        )
        .with_extra_headers(extra_headers);

        let transformer: Arc<dyn RequestTransformer> = match provider.default_style {
            ApiStyle::OpenAI => Arc::new(OpenAiTransformer::new()),
            ApiStyle::Anthropic => Arc::new(AnthropicTransformer::new()),
        };

        Ok(ChatClient::new(Arc::new(dual), transformer, config))
    } else if has_openai {
        // OpenAI 风格
        let endpoint =
            normalize_openai_endpoint(provider.openai_base_url.as_ref().unwrap());
        let p = OpenAiProvider::new(http_client, api_key.clone())
            .with_endpoint(endpoint);

        Ok(ChatClient::new(
            Arc::new(p),
            Arc::new(OpenAiTransformer::new()),
            config,
        ))
    } else {
        // Anthropic 风格
        let endpoint =
            normalize_anthropic_endpoint(provider.anthropic_base_url.as_ref().unwrap());
        let p = AnthropicProvider::new(http_client, api_key.clone())
            .with_endpoint(endpoint)
            .with_extra_headers(extra_headers);

        Ok(ChatClient::new(
            Arc::new(p),
            Arc::new(AnthropicTransformer::new()),
            config,
        ))
    }
}

/// 构建提供商的额外请求头。
///
/// 对 Anthropic 风格的提供商，自动补充 `anthropic-version` 默认头。
fn build_extra_headers(provider: &ProviderConfig) -> Vec<(String, String)> {
    let mut headers: Vec<(String, String)> = provider
        .extra_headers
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    // Anthropic 风格需要 anthropic-version 头
    if matches!(provider.default_style, ApiStyle::Anthropic)
        && !headers.iter().any(|(k, _)| k == "anthropic-version")
    {
        headers.push(("anthropic-version".into(), ANTHROPIC_VERSION.into()));
    }

    headers
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
