//! LLM 连接层——provider 解析与 [`LLMProvider`] 构建的共享实现。
//!
//! 供多个智能体（Motis 宠物助手、学术助手等）复用，避免各自重复实现
//! provider 路由、endpoint 规范化与厂商适配逻辑：
//!
//! - [`resolve_provider_model`]：解析 provider 与 model（优先提示的 provider_id/model_id，
//!   否则回退到 LLM 全局激活项）
//! - [`build_llm_provider`]：按 provider 配置构造 [`LLMProvider`]
//!   （通用 OpenAI 兼容 / 智谱专用 thinking 注入）
//!
//! 依赖 [`LlmConfig`]（全局 LLM 配置存储），与具体的智能体配置解耦。
//! 基于 referee-ai 的 `GenericProvider` 适配器，统一走 OpenAI 兼容协议。

use std::sync::Arc;
use std::time::Duration;

use referee_ai::provider::generic::{GenericConfig, GenericProvider};
use referee_ai::provider::{LLMProvider, RetryPolicy};

use crate::llm_config::model::{ApiStyle, LlmConfig, ProviderConfig};

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

/// Anthropic API 默认版本头。
const ANTHROPIC_VERSION: &str = "2023-06-06";

/// LLM 请求默认超时——4 分钟（长文档 / 深度思考场景需要更大余量）。
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(240);

/// 智谱 thinking 注入字段路径（OpenAI 兼容协议的深度思考参数）。
const ZHIPU_THINKING_FIELD: &str = "thinking.type";

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
        tracing::error!(
            provider_id = pid,
            model_id = mid,
            active_provider_id = %llm.active_provider_id.as_deref().unwrap_or(""),
            active_model_id = %llm.active_model_id.as_deref().unwrap_or(""),
            "解析 LLM 提供商/模型失败：配置的提供商或模型不存在"
        );
        return Err(LlmChatError::Config(format!(
            "配置的提供商/模型不存在: {}/{}",
            pid, mid
        )));
    }

    // 回退 LLM 全局激活项
    match (llm.active_provider(), llm.active_model()) {
        (Some(provider), Some(model)) => Ok((provider, model.id.clone())),
        _ => {
            tracing::error!(
                active_provider_id = %llm.active_provider_id.as_deref().unwrap_or(""),
                active_model_id = %llm.active_model_id.as_deref().unwrap_or(""),
                "解析 LLM 提供商/模型失败：无全局激活项"
            );
            Err(LlmChatError::NoProvider)
        }
    }
}

/// 将 base URL 规范化为完整的 OpenAI Chat Completions 端点。
///
/// 用户配置中的 `openai_base_url` 可能是 base URL（如 `https://api.stepfun.com/v1`），
/// 也可能是完整端点（如 `https://api.stepfun.com/v1/chat/completions`）。
/// 本函数确保最终 URL 以 `/chat/completions` 结尾。
fn normalize_openai_endpoint(base_url: &str) -> String {
    const CHAT_PATH: &str = "/chat/completions";
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with(CHAT_PATH) {
        trimmed.to_string()
    } else {
        format!("{trimmed}{CHAT_PATH}")
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

/// 从 `ModelConfig` 中提取模型规模规格，缺省时使用安全默认值。
///
/// `context_window` / `max_output_tokens` 未配置时回退到 128K / 16K，
/// 足以覆盖大多数主流模型的上下文窗口。
fn resolve_model_spec(provider: &ProviderConfig, model_id: &str) -> (usize, usize) {
    const DEFAULT_CONTEXT: usize = 128 * 1024;
    const DEFAULT_MAX_OUTPUT: usize = 16 * 1024;

    provider
        .find_model(model_id)
        .map(|m| {
            let ctx = m.context_window.map(|v| v as usize).unwrap_or(DEFAULT_CONTEXT);
            let max = m
                .max_output_tokens
                .map(|v| v as usize)
                .unwrap_or(DEFAULT_MAX_OUTPUT);
            (ctx, max)
        })
        .unwrap_or((DEFAULT_CONTEXT, DEFAULT_MAX_OUTPUT))
}

/// 构造 [`LLMProvider`]。
///
/// 按提供商配置选择适配策略：
/// - 所有提供商均走 `GenericProvider`（OpenAI 兼容协议底座）
/// - `openai_base_url` 优先；仅有 `anthropic_base_url` 时使用 Anthropic 风格端点
/// - 智谱（GLM）注入 `thinking.type` 字段（深度思考参数）
/// - base_url 自动规范化（补全 `/chat/completions`）
/// - 额外请求头逐条注入
///
/// 返回 `Arc<dyn LLMProvider>`，可直接用于构造 referee `Engine`。
///
/// 使用默认超时（4 分钟）。需要自定义超时的调用方使用
/// [`build_llm_provider_with_timeout`]。
pub fn build_llm_provider(
    provider: &ProviderConfig,
    model_id: &str,
) -> Result<Arc<dyn LLMProvider>, LlmChatError> {
    build_llm_provider_with_timeout(provider, model_id, DEFAULT_REQUEST_TIMEOUT)
}

/// 与 [`build_llm_provider`] 相同，但额外指定请求超时时间。
pub fn build_llm_provider_with_timeout(
    provider: &ProviderConfig,
    model_id: &str,
    timeout: Duration,
) -> Result<Arc<dyn LLMProvider>, LlmChatError> {
    let api_key = provider.api_key.as_ref().ok_or_else(|| {
        tracing::error!(provider_id = %provider.id, "构建 LLM Provider 失败：未配置 api_key");
        LlmChatError::Config(format!("提供商 {} 未配置 api_key", provider.id))
    })?;

    // 确定端点 URL：优先 OpenAI 风格，回退 Anthropic 风格
    let base_url = if let Some(openai_url) = provider.openai_base_url.as_ref() {
        normalize_openai_endpoint(openai_url)
    } else if let Some(anthropic_url) = provider.anthropic_base_url.as_ref() {
        // Anthropic 风格端点也走 OpenAI 兼容底座（GenericProvider 统一处理）
        let trimmed = anthropic_url.trim_end_matches('/');
        if trimmed.ends_with("/v1/messages") {
            // 去掉 /v1/messages 后缀，改用 /chat/completions
            // 因为 GenericProvider 内部会自动拼接 /chat/completions
            trimmed
                .strip_suffix("/v1/messages")
                .unwrap_or(trimmed)
                .to_string()
        } else {
            trimmed.to_string()
        }
    } else {
        tracing::error!(
            provider_id = %provider.id,
            "构建 LLM Provider 失败：未配置 openai_base_url 或 anthropic_base_url"
        );
        return Err(LlmChatError::Config(format!(
            "提供商 {} 未配置 openai_base_url 或 anthropic_base_url",
            provider.id
        )));
    };

    let extra_headers = build_extra_headers(provider);
    let (context_window, max_output) = resolve_model_spec(provider, model_id);

    let mut config = GenericConfig::new(api_key.clone(), base_url, model_id)
        .with_extra_headers(extra_headers)
        .with_model_spec(context_window, max_output)
        .with_timeout(timeout)
        .with_retry(RetryPolicy::default());

    // 智谱（GLM）：注入 thinking.type 字段（OpenAI 兼容协议，深度思考参数需专用转换器注入）
    if provider.id == "zhipu" {
        config = config.with_thinking(ZHIPU_THINKING_FIELD);
    }

    let generic = GenericProvider::new(&provider.id, config)
        .map_err(|e| {
            tracing::error!(
                provider_id = %provider.id,
                model_id = model_id,
                "构建 LLM Provider 失败: {e}"
            );
            LlmChatError::Config(format!("创建 LLM Provider 失败: {e}"))
        })?;

    Ok(Arc::new(generic))
}

/// 便捷方法：一步完成 provider 解析 + LLMProvider 构造。
///
/// 适用于不需要中间处理 provider/model 信息的调用方。
pub fn resolve_and_build(
    provider_id: Option<&str>,
    model_id: Option<&str>,
    llm: &LlmConfig,
) -> Result<Arc<dyn LLMProvider>, LlmChatError> {
    let (provider, model_id) = resolve_provider_model(provider_id, model_id, llm)?;
    build_llm_provider(provider, &model_id)
}

// 导出 referee 类型供上层模块使用
#[allow(unused_imports)]
pub use referee_ai::provider::{ChatRequest, ChatResponse, LlmError as ProviderError};

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
        let (provider, model) =
            resolve_provider_model(Some("openai"), Some("gpt-4o"), &config).unwrap();
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
    fn build_provider_requires_api_key() {
        let config = sample_config();
        let provider = config.find_provider("openai").unwrap();
        let _provider = build_llm_provider(provider, "gpt-4o").unwrap();
    }

    #[test]
    fn normalize_openai_endpoint_handles_base_url() {
        assert_eq!(
            normalize_openai_endpoint("https://api.example.com/v1"),
            "https://api.example.com/v1/chat/completions"
        );
    }

    #[test]
    fn normalize_openai_endpoint_handles_full_url() {
        assert_eq!(
            normalize_openai_endpoint("https://api.example.com/v1/chat/completions"),
            "https://api.example.com/v1/chat/completions"
        );
    }

    #[test]
    fn normalize_openai_endpoint_strips_trailing_slash() {
        assert_eq!(
            normalize_openai_endpoint("https://api.example.com/v1/"),
            "https://api.example.com/v1/chat/completions"
        );
    }
}
