//! LLM 连接层——provider 解析与 [`ChatClient`] 构建的共享实现。
//!
//! 供多个智能体（Motis 宠物助手、学术助手等）复用，避免各自重复实现
//! provider 路由、endpoint 规范化与厂商适配逻辑：
//!
//! - [`resolve_provider_model`]：解析 provider 与 model（优先提示的 provider_id/model_id，
//!   否则回退到 LLM 全局激活项）
//! - [`build_chat_client`]：按 provider 配置构造 [`ChatClient`]
//!   （智谱专用 / OpenAI 风格 / Anthropic 风格 / 双风格）
//!
//! 依赖 [`LlmConfig`]（全局 LLM 配置存储），与具体的智能体配置解耦。

use std::sync::Arc;

use confluent::llmkit::{
    AnthropicProvider, AnthropicTransformer, ApiStyle, ChatClient, ChatClientConfig,
    DualStyleProvider, OpenAiProvider, OpenAiTransformer, RequestTransformer,
    ZhipuProvider, ZhipuTransformer,
};

use crate::llm_config::model::{LlmConfig, ProviderConfig};

/// 聊天连接层错误。
#[derive(Debug, thiserror::Error)]
pub enum LlmChatError {
    /// 配置缺失或非法。
    #[error("配置错误: {0}")]
    Config(String),
    /// 未配置任何可用的 LLM 提供商。
    #[error("未配置 LLM 提供商，请先在「LLM 配置」中添加")]
    NoProvider,
}

/// OpenAI 风格端点的路径后缀。
const OPENAI_CHAT_PATH: &str = "/chat/completions";

/// Anthropic 风格端点的路径后缀。
const ANTHROPIC_MESSAGES_PATH: &str = "/v1/messages";

/// Anthropic API 默认版本头。
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// 解析 provider 与 model。
///
/// 优先使用传入的 `provider_id` / `model_id`（智能体自身配置），
/// 回退到 [`LlmConfig`] 的全局激活项。
pub fn resolve_provider_model<'a>(
    provider_id: Option<&str>,
    model_id: Option<&str>,
    llm: &'a LlmConfig,
) -> Result<(&'a ProviderConfig, String), LlmChatError> {
    // 优先智能体配置
    if let (Some(pid), Some(mid)) = (provider_id, model_id) {
        if let Some(provider) = llm.find_provider(pid) {
            if provider.find_model(mid).is_some() {
                return Ok((provider, mid.to_string()));
            }
        }
        return Err(LlmChatError::Config(format!(
            "配置的提供商/模型不存在: {}/{}",
            pid, mid
        )));
    }

    // 回退 LLM 全局激活项
    match (llm.active_provider(), llm.active_model()) {
        (Some(provider), Some(model)) => Ok((provider, model.id.clone())),
        _ => Err(LlmChatError::NoProvider),
    }
}

/// 将 base URL 规范化为完整的 OpenAI Chat Completions 端点。
///
/// 用户配置中的 `openai_base_url` 可能是 base URL（如 `https://api.stepfun.com/v1`），
/// 也可能是完整端点（如 `https://api.stepfun.com/v1/chat/completions`）。
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
pub fn build_chat_client(provider: &ProviderConfig) -> Result<ChatClient, LlmChatError> {
    let api_key = provider.api_key.as_ref().ok_or_else(|| {
        LlmChatError::Config(format!("提供商 {} 未配置 api_key", provider.id))
    })?;

    let http_client = reqwest::Client::new();
    let config = ChatClientConfig {
        default_style: provider.default_style,
        ..Default::default()
    };

    // 深度适配提供商：按 provider.id 路由到专用 Provider，注入厂商特有请求字段。
    // 智谱（GLM）为 OpenAI 兼容协议，但深度思考参数（thinking.type / reasoning_effort）
    // 需要专用转换器注入；流式 reasoning_content 由 OpenAiTransformer 通用解析。
    if provider.id == "zhipu" {
        let endpoint = normalize_openai_endpoint(
            provider
                .openai_base_url
                .as_ref()
                .ok_or_else(|| {
                    LlmChatError::Config(format!(
                        "提供商 {} 未配置 openai_base_url",
                        provider.id
                    ))
                })?,
        );
        let p = ZhipuProvider::new(http_client, api_key.clone()).with_endpoint(endpoint);
        return Ok(ChatClient::new(
            Arc::new(p),
            Arc::new(ZhipuTransformer::new()),
            config,
        ));
    }

    let has_openai = provider.openai_base_url.is_some();
    let has_anthropic = provider.anthropic_base_url.is_some();
    let extra_headers = build_extra_headers(provider);

    if has_openai && has_anthropic {
        // 双风格 Provider
        let openai_endpoint = normalize_openai_endpoint(provider.openai_base_url.as_ref().unwrap());
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
        let endpoint = normalize_openai_endpoint(provider.openai_base_url.as_ref().unwrap());
        let p = OpenAiProvider::new(http_client, api_key.clone()).with_endpoint(endpoint);

        Ok(ChatClient::new(
            Arc::new(p),
            Arc::new(OpenAiTransformer::new()),
            config,
        ))
    } else {
        // Anthropic 风格
        let endpoint = normalize_anthropic_endpoint(provider.anthropic_base_url.as_ref().unwrap());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_config::model::LlmConfig;

    /// 构造一个最小可用的 LlmConfig。
    fn sample_config() -> LlmConfig {
        serde_json::from_value(serde_json::json!({
            "version": "1.0.0",
            "providers": [
                {
                    "id": "openai",
                    "name": "OpenAI",
                    "api_key": "sk-test",
                    "default_style": "OpenAI",
                    "openai_base_url": "https://api.example.com/v1",
                    "models": [{"id": "gpt-4o", "name": "GPT-4o"}]
                }
            ],
            "active_provider_id": "openai",
            "active_model_id": "gpt-4o"
        }))
        .unwrap()
    }

    #[test]
    fn resolves_active_provider_when_no_hint() {
        let config = sample_config();
        let (provider, model) = resolve_provider_model(None, None, &config).unwrap();
        assert_eq!(provider.id, "openai");
        assert_eq!(model, "gpt-4o");
    }

    #[test]
    fn resolves_hinted_provider_first() {
        let config = sample_config();
        let (provider, model) = resolve_provider_model(
            Some("openai"),
            Some("gpt-4o"),
            &config,
        )
        .unwrap();
        assert_eq!(provider.id, "openai");
        assert_eq!(model, "gpt-4o");
    }

    #[test]
    fn errors_when_hint_unknown() {
        let config = sample_config();
        let err = resolve_provider_model(Some("nope"), Some("x"), &config).unwrap_err();
        assert!(matches!(err, LlmChatError::Config(_)));
    }

    #[test]
    fn build_client_requires_api_key() {
        let config = sample_config();
        let provider = config.find_provider("openai").unwrap();
        // 构造成功即可（端点为 OpenAI 风格）
        let _client = build_chat_client(provider).unwrap();
    }
}
