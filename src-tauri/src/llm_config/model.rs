//! LLM 配置的纯数据模型与校验逻辑。
//!
//! 所有结构体仅承载数据，不涉及文件 I/O。
//! 序列化格式为 JSON，设计目标是清晰、易读、易扩展。

use std::collections::{HashMap, HashSet};

use confluent::llmkit::ApiStyle;
use serde::{Deserialize, Serialize};

use super::error::LlmConfigError;

// ---------------------------------------------------------------------------
// 提供商类型
// ---------------------------------------------------------------------------

/// 提供商类型。
///
/// `Custom` 为用户自定义的第三方提供商；`Cloud` 为 Fluen 云端模型服务（预留）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    /// 用户自定义提供商。
    #[default]
    Custom,
    /// Fluen 云端模型服务（未来拓展）。
    Cloud,
}

// ---------------------------------------------------------------------------
// 模型能力
// ---------------------------------------------------------------------------

/// 模型能力标志。
///
/// 描述单个模型支持的输入模态与功能特性，用于前端能力展示与请求构建时的条件判断。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// 思考 / 推理模式（如 DeepSeek-R1 extended thinking）。
    #[serde(default)]
    pub thinking: bool,
    /// 图片输入。
    #[serde(default)]
    pub vision: bool,
    /// 音频输入。
    #[serde(default)]
    pub audio: bool,
    /// 视频输入。
    #[serde(default)]
    pub video: bool,
    /// 工具 / 函数调用。
    #[serde(default = "default_true")]
    pub tool_calling: bool,
    /// 流式响应。
    #[serde(default = "default_true")]
    pub streaming: bool,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            thinking: false,
            vision: false,
            audio: false,
            video: false,
            tool_calling: true,
            streaming: true,
        }
    }
}

// ---------------------------------------------------------------------------
// 模型配置
// ---------------------------------------------------------------------------

/// 单个模型的配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// 模型 ID（如 `deepseek-chat`、`gpt-4o`），同一提供商内不可重复。
    pub id: String,
    /// 模型显示名称（如 `DeepSeek Chat`）。
    pub name: String,
    /// 模型能力标志。
    #[serde(default)]
    pub capabilities: ModelCapabilities,
    /// 最大输出 token 数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    /// 上下文窗口大小（token 数）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,
    /// 模型描述（可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 是否启用。
    #[serde(default = "default_true")]
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// 提供商配置
// ---------------------------------------------------------------------------

/// 单个 LLM 提供商的配置。
///
/// `api_key` 为敏感字段，[`Debug`] 实现中会被掩码为 `***`。
#[derive(Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// 提供商唯一标识（如 `deepseek`、`openai`），全局不可重复。
    pub id: String,
    /// 提供商显示名称（如 `DeepSeek`、`OpenAI`）。
    pub name: String,
    /// 提供商类型。
    #[serde(default)]
    pub provider_type: ProviderType,
    /// OpenAI 风格的 base URL。
    ///
    /// 与 `anthropic_base_url` 至少需填写一个。部分提供商（如 DeepSeek）同时支持两种风格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_base_url: Option<String>,
    /// Anthropic 风格的 base URL。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anthropic_base_url: Option<String>,
    /// API Key（`Custom` 类型必填，`Cloud` 类型由账号系统管理）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// 默认 API 风格。
    #[serde(default = "default_api_style")]
    pub default_style: ApiStyle,
    /// 额外请求头（如 `anthropic-version`）。
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub extra_headers: HashMap<String, String>,
    /// 是否启用。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 该提供商下的模型列表。
    #[serde(default)]
    pub models: Vec<ModelConfig>,
    /// 创建时间（ISO 8601）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// 更新时间（ISO 8601）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl std::fmt::Debug for ProviderConfig {
    /// 手动实现 Debug：`api_key` 值掩码为 `***`，避免日志泄露。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("provider_type", &self.provider_type)
            .field("openai_base_url", &self.openai_base_url)
            .field("anthropic_base_url", &self.anthropic_base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "***"))
            .field("default_style", &self.default_style)
            .field("extra_headers", &self.extra_headers)
            .field("enabled", &self.enabled)
            .field("models", &self.models)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

impl ProviderConfig {
    /// 校验当前提供商配置的完整性。
    ///
    /// # 校验规则
    /// - `id` 非空
    /// - `openai_base_url` 与 `anthropic_base_url` 至少填一个
    /// - `default_style` 对应的 base_url 必须存在
    /// - `provider_type == Custom` 时 `api_key` 必填
    /// - `models` 中各 `id` 在同一提供商内不重复
    pub fn validate(&self) -> Result<(), LlmConfigError> {
        if self.id.trim().is_empty() {
            return Err(LlmConfigError::Validation("provider id 不能为空".into()));
        }

        if self.openai_base_url.is_none() && self.anthropic_base_url.is_none() {
            return Err(LlmConfigError::Validation(
                "openai_base_url 和 anthropic_base_url 至少需要填写一个".into(),
            ));
        }

        match self.default_style {
            ApiStyle::OpenAI => {
                if self.openai_base_url.is_none() {
                    return Err(LlmConfigError::Validation(
                        "default_style 为 OpenAI 时必须填写 openai_base_url".into(),
                    ));
                }
            }
            ApiStyle::Anthropic => {
                if self.anthropic_base_url.is_none() {
                    return Err(LlmConfigError::Validation(
                        "default_style 为 Anthropic 时必须填写 anthropic_base_url".into(),
                    ));
                }
            }
        }

        // api_key 在 onboarding 阶段可暂不填写，用户可在设置中后续补充。
        // 此处不强制校验 api_key，仅在发起 API 请求时检查。

        let mut seen: HashSet<&str> = HashSet::new();
        for model in &self.models {
            if !seen.insert(model.id.as_str()) {
                return Err(LlmConfigError::Validation(format!(
                    "模型 ID '{}' 在同一提供商内重复",
                    model.id
                )));
            }
        }

        Ok(())
    }

    /// 按 ID 查找模型。
    pub fn find_model(&self, id: &str) -> Option<&ModelConfig> {
        self.models.iter().find(|m| m.id == id)
    }

    /// 按 ID 查找模型（可变引用）。
    pub fn find_model_mut(&mut self, id: &str) -> Option<&mut ModelConfig> {
        self.models.iter_mut().find(|m| m.id == id)
    }
}

// ---------------------------------------------------------------------------
// 场景化模型槽位
// ---------------------------------------------------------------------------

/// 场景化模型引用——指向 `providers` 中具体的 provider+model 组合。
///
/// 用于为特定业务场景（如知识库构建、翻译、OCR 纠错）指定专用模型，
/// 与全局 `active_model_id` 解耦，互不影响。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneModelRef {
    /// 提供商 ID（必须在 `LlmConfig.providers` 中存在）。
    pub provider_id: String,
    /// 模型 ID（必须在对应提供商的 `models` 列表中存在）。
    pub model_id: String,
}

/// 场景化模型配置集合。
///
/// 每个字段对应一个业务场景的专用模型槽位。`None` 表示该场景回退到
/// 全局 `active_provider_id` / `active_model_id`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneModels {
    /// 知识库构建专用模型槽位。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge_build: Option<SceneModelRef>,
    // 未来扩展：
    // pub translation: Option<SceneModelRef>,
    // pub ocr_correction: Option<SceneModelRef>,
}

// ---------------------------------------------------------------------------
// 顶层配置
// ---------------------------------------------------------------------------

/// Embedding 模型配置。
///
/// 控制知识库语义检索的 Embedding 模型选择。
/// `model_ref` 为 `None` 时使用内置免费提供商（ModelScope Qwen3-Embedding-4B）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// 是否启用 Embedding（默认 `true`）。
    ///
    /// 禁用时知识库检索降级为纯关键词匹配。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 用户自定义 Embedding 模型引用。
    ///
    /// `None` 时使用内置免费提供商。引用失效时自动回退到内置提供商。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_ref: Option<SceneModelRef>,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            model_ref: None,
        }
    }
}

/// LLM 配置顶层结构，对应 `llm_config.json` 文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// 配置文件版本号。
    #[serde(default = "default_version")]
    pub version: String,
    /// 当前激活的提供商 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_provider_id: Option<String>,
    /// 当前激活的模型 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_model_id: Option<String>,
    /// 提供商列表。
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    /// 场景化模型槽位（按业务场景指定专用模型）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_models: Option<SceneModels>,
    /// Embedding 模型配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<EmbeddingConfig>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            version: default_version(),
            active_provider_id: None,
            active_model_id: None,
            providers: Vec::new(),
            scene_models: None,
            embedding: None,
        }
    }
}

impl LlmConfig {
    /// 校验全部配置。
    ///
    /// 逐个校验 [`ProviderConfig`]，并检查提供商 ID 全局唯一。
    pub fn validate(&self) -> Result<(), LlmConfigError> {
        let mut seen: HashSet<&str> = HashSet::new();
        for provider in &self.providers {
            provider.validate()?;
            if !seen.insert(provider.id.as_str()) {
                return Err(LlmConfigError::Validation(format!(
                    "提供商 ID '{}' 重复",
                    provider.id
                )));
            }
        }
        Ok(())
    }

    /// 按 ID 查找提供商。
    pub fn find_provider(&self, id: &str) -> Option<&ProviderConfig> {
        self.providers.iter().find(|p| p.id == id)
    }

    /// 按 ID 查找提供商（可变引用）。
    pub fn find_provider_mut(&mut self, id: &str) -> Option<&mut ProviderConfig> {
        self.providers.iter_mut().find(|p| p.id == id)
    }

    /// 获取当前激活的提供商。
    pub fn active_provider(&self) -> Option<&ProviderConfig> {
        self.active_provider_id
            .as_ref()
            .and_then(|id| self.find_provider(id))
    }

    /// 获取当前激活的模型。
    pub fn active_model(&self) -> Option<&ModelConfig> {
        let provider = self.active_provider()?;
        self.active_model_id
            .as_ref()
            .and_then(|id| provider.models.iter().find(|m| &m.id == id))
    }

    /// 解析场景化模型：优先使用场景槽位，回退到全局激活项。
    ///
    /// 返回 `(&ProviderConfig, model_id)`。若场景槽位未配置或引用失效，
    /// 回退到 `active_provider_id` / `active_model_id`。
    pub fn resolve_scene(
        &self,
        scene: &SceneModelRef,
    ) -> Option<(&ProviderConfig, String)> {
        let provider = self.find_provider(&scene.provider_id)?;
        if provider.find_model(&scene.model_id).is_some() {
            return Some((provider, scene.model_id.clone()));
        }
        None
    }

    /// 解析知识库构建场景模型。
    ///
    /// 解析顺序：
    /// 1. 若配置了 `scene_models.knowledge_build`：引用有效则用之；**引用失效则返回 `None`**。
    ///    用户已明确指定场景模型，失效应显式报错，而非悄悄回退到可能不符预期的全局激活项。
    /// 2. 未配置场景模型：回退到全局 `active_provider_id` / `active_model_id`。
    pub fn resolve_knowledge_build(&self) -> Option<(&ProviderConfig, String)> {
        if let Some(scene_models) = &self.scene_models {
            if let Some(kb) = &scene_models.knowledge_build {
                return self.resolve_scene(kb).or_else(|| {
                    tracing::warn!(
                        provider_id = %kb.provider_id,
                        model_id = %kb.model_id,
                        "scene_models.knowledge_build 引用失效（provider 或 model 不存在），拒绝回退到全局激活项"
                    );
                    None
                });
            }
        }
        let provider = self.active_provider()?;
        let model = self.active_model()?;
        Some((provider, model.id.clone()))
    }

    /// 解析 Embedding 配置。
    ///
    /// 返回值：
    /// - `None`：未配置 `embedding` 或 `enabled == false`，表示禁用 Embedding。
    /// - `Some(None)`：已启用但未指定自定义模型，使用内置免费提供商。
    /// - `Some(Some((&ProviderConfig, model_id)))`：已启用且指定了自定义模型，引用有效。
    ///   引用失效时回退到 `Some(None)`（使用内置提供商）。
    pub fn resolve_embedding(&self) -> Option<Option<(&ProviderConfig, String)>> {
        let emb = self.embedding.as_ref()?;
        if !emb.enabled {
            return None;
        }
        match &emb.model_ref {
            None => Some(None),
            Some(ref_) => match self.resolve_scene(ref_) {
                Some((provider, model_id)) => Some(Some((provider, model_id))),
                None => {
                    tracing::warn!(
                        provider_id = %ref_.provider_id,
                        model_id = %ref_.model_id,
                        "embedding.model_ref 引用失效，回退到内置提供商"
                    );
                    Some(None) // 引用失效回退到内置
                }
            },
        }
    }
}

// ---------------------------------------------------------------------------
// serde 辅助函数
// ---------------------------------------------------------------------------

fn default_true() -> bool {
    true
}

fn default_version() -> String {
    "1.0.0".to_string()
}

fn default_api_style() -> ApiStyle {
    ApiStyle::OpenAI
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_provider() -> ProviderConfig {
        ProviderConfig {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            provider_type: ProviderType::Custom,
            openai_base_url: Some("https://api.deepseek.com/v1/chat/completions".into()),
            anthropic_base_url: Some("https://api.deepseek.com/anthropic".into()),
            api_key: Some("sk-test".into()),
            default_style: ApiStyle::OpenAI,
            extra_headers: HashMap::from([("anthropic-version".into(), "2023-06-01".into())]),
            enabled: true,
            models: vec![
                ModelConfig {
                    id: "deepseek-chat".into(),
                    name: "DeepSeek Chat".into(),
                    capabilities: ModelCapabilities::default(),
                    max_output_tokens: Some(8192),
                    context_window: Some(64000),
                    description: None,
                    enabled: true,
                },
                ModelConfig {
                    id: "deepseek-reasoner".into(),
                    name: "DeepSeek Reasoner".into(),
                    capabilities: ModelCapabilities {
                        thinking: true,
                        ..ModelCapabilities::default()
                    },
                    max_output_tokens: Some(8192),
                    context_window: Some(64000),
                    description: None,
                    enabled: true,
                },
            ],
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn provider_validate_ok() {
        assert!(sample_provider().validate().is_ok());
    }

    #[test]
    fn provider_validate_empty_id() {
        let mut p = sample_provider();
        p.id = "  ".into();
        let err = p.validate().unwrap_err();
        assert!(matches!(err, LlmConfigError::Validation(_)));
    }

    #[test]
    fn provider_validate_missing_both_urls() {
        let mut p = sample_provider();
        p.openai_base_url = None;
        p.anthropic_base_url = None;
        let err = p.validate().unwrap_err();
        assert!(matches!(err, LlmConfigError::Validation(_)));
    }

    #[test]
    fn provider_validate_style_without_matching_url() {
        let mut p = sample_provider();
        p.default_style = ApiStyle::Anthropic;
        p.anthropic_base_url = None;
        let err = p.validate().unwrap_err();
        assert!(matches!(err, LlmConfigError::Validation(_)));
    }

#[test]
fn provider_validate_custom_without_api_key_ok() {
    let mut p = sample_provider();
    p.api_key = None;
    // api_key 可在 onboarding 阶段暂不填写
    assert!(p.validate().is_ok());
}

    #[test]
    fn provider_validate_cloud_without_api_key_ok() {
        let mut p = sample_provider();
        p.provider_type = ProviderType::Cloud;
        p.api_key = None;
        assert!(p.validate().is_ok());
    }

    #[test]
    fn provider_validate_duplicate_model_id() {
        let mut p = sample_provider();
        p.models[1].id = p.models[0].id.clone();
        let err = p.validate().unwrap_err();
        assert!(matches!(err, LlmConfigError::Validation(_)));
    }

    #[test]
    fn config_validate_duplicate_provider_id() {
        let p = sample_provider();
        let config = LlmConfig {
            providers: vec![p.clone(), p],
            ..LlmConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(matches!(err, LlmConfigError::Validation(_)));
    }

    #[test]
    fn find_provider_and_model() {
        let config = LlmConfig {
            active_provider_id: Some("deepseek".into()),
            active_model_id: Some("deepseek-chat".into()),
            providers: vec![sample_provider()],
            ..LlmConfig::default()
        };
        assert!(config.find_provider("deepseek").is_some());
        assert!(config.find_provider("openai").is_none());
        assert!(config.active_provider().is_some());
        assert!(config.active_model().is_some());
        assert_eq!(config.active_model().unwrap().id, "deepseek-chat");
    }

    #[test]
    fn find_provider_mut_updates_model() {
        let mut config = LlmConfig {
            providers: vec![sample_provider()],
            ..LlmConfig::default()
        };
        let provider = config.find_provider_mut("deepseek").unwrap();
        let model = provider.find_model_mut("deepseek-chat").unwrap();
        model.enabled = false;
        assert!(!config.find_provider("deepseek").unwrap().find_model("deepseek-chat").unwrap().enabled);
    }

    #[test]
    fn debug_masks_api_key() {
        let p = sample_provider();
        let debug_str = format!("{p:?}");
        assert!(debug_str.contains("***"));
        assert!(!debug_str.contains("sk-test"));
    }

    #[test]
    fn json_roundtrip_preserves_data() {
        let config = LlmConfig {
            active_provider_id: Some("deepseek".into()),
            active_model_id: Some("deepseek-chat".into()),
            providers: vec![sample_provider()],
            ..LlmConfig::default()
        };
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: LlmConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, "1.0.0");
        assert_eq!(parsed.providers.len(), 1);
        assert_eq!(parsed.providers[0].id, "deepseek");
        assert_eq!(parsed.providers[0].models.len(), 2);
        assert_eq!(parsed.providers[0].api_key, Some("sk-test".into()));
    }

    #[test]
    fn default_capabilities_has_tool_calling_and_streaming() {
        let caps = ModelCapabilities::default();
        assert!(caps.tool_calling);
        assert!(caps.streaming);
        assert!(!caps.thinking);
        assert!(!caps.vision);
    }

    #[test]
    fn provider_type_default_is_custom() {
        assert_eq!(ProviderType::default(), ProviderType::Custom);
    }

    #[test]
    fn scene_models_default_is_none() {
        let config = LlmConfig::default();
        assert!(config.scene_models.is_none());
    }

    #[test]
    fn scene_models_roundtrip() {
        let config = LlmConfig {
            active_provider_id: Some("deepseek".into()),
            active_model_id: Some("deepseek-chat".into()),
            providers: vec![sample_provider()],
            scene_models: Some(SceneModels {
                knowledge_build: Some(SceneModelRef {
                    provider_id: "deepseek".into(),
                    model_id: "deepseek-reasoner".into(),
                }),
            }),
            ..LlmConfig::default()
        };
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: LlmConfig = serde_json::from_str(&json).unwrap();
        let kb = parsed.scene_models.unwrap().knowledge_build.unwrap();
        assert_eq!(kb.provider_id, "deepseek");
        assert_eq!(kb.model_id, "deepseek-reasoner");
    }

    #[test]
    fn resolve_knowledge_build_uses_scene_slot() {
        let config = LlmConfig {
            active_provider_id: Some("deepseek".into()),
            active_model_id: Some("deepseek-chat".into()),
            providers: vec![sample_provider()],
            scene_models: Some(SceneModels {
                knowledge_build: Some(SceneModelRef {
                    provider_id: "deepseek".into(),
                    model_id: "deepseek-reasoner".into(),
                }),
            }),
            ..LlmConfig::default()
        };
        let (provider, model_id) = config.resolve_knowledge_build().unwrap();
        assert_eq!(provider.id, "deepseek");
        assert_eq!(model_id, "deepseek-reasoner");
    }

    #[test]
    fn resolve_knowledge_build_falls_back_to_active() {
        let config = LlmConfig {
            active_provider_id: Some("deepseek".into()),
            active_model_id: Some("deepseek-chat".into()),
            providers: vec![sample_provider()],
            scene_models: None,
            ..LlmConfig::default()
        };
        let (provider, model_id) = config.resolve_knowledge_build().unwrap();
        assert_eq!(provider.id, "deepseek");
        assert_eq!(model_id, "deepseek-chat");
    }

    #[test]
    fn resolve_knowledge_build_errors_when_scene_invalid() {
        // 用户明确配了场景模型但引用失效：应返回 None（拒绝静默回退到 active）
        let config = LlmConfig {
            active_provider_id: Some("deepseek".into()),
            active_model_id: Some("deepseek-chat".into()),
            providers: vec![sample_provider()],
            scene_models: Some(SceneModels {
                knowledge_build: Some(SceneModelRef {
                    provider_id: "nonexistent".into(),
                    model_id: "gpt-4o".into(),
                }),
            }),
            ..LlmConfig::default()
        };
        assert!(
            config.resolve_knowledge_build().is_none(),
            "场景模型引用失效时应返回 None，而非回退到 active"
        );
    }

    #[test]
    fn deserialize_old_config_without_scene_models() {
        // 旧版配置文件无 scene_models 字段，应能正常反序列化
        let json = r#"{
            "version": "1.0.0",
            "active_provider_id": "deepseek",
            "active_model_id": "deepseek-chat",
            "providers": []
        }"#;
        let config: LlmConfig = serde_json::from_str(json).unwrap();
        assert!(config.scene_models.is_none());
    }
}
