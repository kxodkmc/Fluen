//! LLM 连接层——provider 解析与 [`LLMProvider`] 构建的共享实现。
//!
//! 供多个智能体（Motis 宠物助手、学术助手等）复用，避免各自重复实现
//! provider 路由、endpoint 规范化与厂商适配逻辑：
//!
//! - [`resolve_provider_model`]：解析 provider 与 model（优先提示的 provider_id/model_id，
//!   否则回退到 LLM 全局激活项）
//! - [`build_llm_provider`]：按 provider 配置构造 [`LLMProvider`]（通用 OpenAI 兼容适配器）
//!
//! 依赖 [`LlmConfig`]（全局 LLM 配置存储），与具体的智能体配置解耦。
//! 基于 referee-ai 的 `OpenAiProvider` 适配器，统一走 OpenAI 兼容协议。

use std::sync::Arc;
use std::time::Duration;

use referee_ai::provider::openai::{OpenAiConfig, OpenAiProvider};
use referee_ai::provider::{LLMProvider, ModelSpec, RetryPolicy};

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

/// LLM 请求默认超时——4 分钟（长文档 / 深度思考场景需要更大余量）。
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(240);

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

/// 将用户配置的 URL 规范化为 base URL。
///
/// 用户配置的 `openai_base_url` 可能是 base（如 `https://api.deepseek.com`、
/// `https://api.stepfun.com/v1`），也可能是完整端点（如
/// `https://api.stepfun.com/v1/chat/completions`）。`OpenAiProvider` 底层客户端
/// 会自行拼接 `/chat/completions`，故此处统一剥离该后缀，确保以 base 形式传入。
fn normalize_base_url(url: &str) -> String {
    const CHAT_PATH: &str = "/chat/completions";
    let trimmed = url.trim_end_matches('/');
    if let Some(stripped) = trimmed.strip_suffix(CHAT_PATH) {
        stripped.to_string()
    } else {
        trimmed.to_string()
    }
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
/// 所有提供商均走 `OpenAiProvider`（OpenAI 兼容协议底座）：
/// - `openai_base_url` 优先；仅有 `anthropic_base_url` 时使用其作为 base
/// - base_url 自动规范化（剥离 `/chat/completions` 后缀）
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

    // 确定 base URL：优先 OpenAI 风格，回退 Anthropic 风格
    let base_url = if let Some(openai_url) = provider.openai_base_url.as_ref() {
        normalize_base_url(openai_url)
    } else if let Some(anthropic_url) = provider.anthropic_base_url.as_ref() {
        // Anthropic 风格端点同样由 OpenAI 兼容底座处理，剥离开各自后缀统一走 /chat/completions
        normalize_base_url(anthropic_url)
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

    let (context_window, max_output) = resolve_model_spec(provider, model_id);

    let config = OpenAiConfig::new(base_url, api_key.clone(), model_id.to_string())
        .with_model_spec(ModelSpec {
            context_window_tokens: context_window,
            max_output_tokens: max_output,
        })
        .with_timeout(timeout)
        .with_retry(RetryPolicy::default());

    let openai = OpenAiProvider::new(config).map_err(|e| {
        tracing::error!(
            provider_id = %provider.id,
            model_id = model_id,
            "构建 LLM Provider 失败: {e}"
        );
        LlmChatError::Config(format!("创建 LLM Provider 失败: {e}"))
    })?;

    Ok(Arc::new(openai))
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
    fn normalize_base_url_preserves_base() {
        assert_eq!(
            normalize_base_url("https://api.example.com/v1"),
            "https://api.example.com/v1"
        );
    }

    #[test]
    fn normalize_base_url_strips_full_endpoint() {
        assert_eq!(
            normalize_base_url("https://api.example.com/v1/chat/completions"),
            "https://api.example.com/v1"
        );
    }

    #[test]
    fn normalize_base_url_strips_trailing_slash() {
        assert_eq!(
            normalize_base_url("https://api.example.com/v1/"),
            "https://api.example.com/v1"
        );
    }
}
