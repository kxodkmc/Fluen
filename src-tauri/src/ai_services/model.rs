//! AI 服务的纯数据模型与校验逻辑。
//!
//! 所有结构体仅承载数据，不涉及文件 I/O。
//! 序列化格式为 JSON，设计目标是清晰、易读、易扩展。
//!
//! ## 扩展指南
//!
//! 新增服务类型（如 TTS）时：
//! 1. 在 [`ServiceCategory`] 中添加变体
//! 2. 创建对应的 provider 实现（参考 [`super::paddleocr`]）
//! 3. 配置模型与存储层无需改动

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::error::AiServiceError;

// ---------------------------------------------------------------------------
// 服务类型与部署模式
// ---------------------------------------------------------------------------

/// AI 服务类型。
///
/// 每个类型对应一类 AI 能力，配置中通过此字段区分提供商归属。
/// 新增服务类型时在此添加变体即可。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceCategory {
    /// 文字识别（OCR）。
    Ocr,
    /// 语音合成（TTS）——预留。
    Tts,
    /// 语音识别（ASR）——预留。
    Asr,
}

/// 部署模式。
///
/// `Api` 为云端 API 调用；`Local` 为本地部署模型调用（预留）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentMode {
    /// 云端 API 调用。
    #[default]
    Api,
    /// 本地部署（预留）。
    Local,
}

/// API 认证方案。
///
/// 不同提供商使用不同的 Authorization 头格式。
/// PaddleOCR Job API 使用 `bearer`，Sync API 使用 `token`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthScheme {
    /// `Authorization: bearer {token}`
    #[default]
    Bearer,
    /// `Authorization: token {token}`
    Token,
}

impl AuthScheme {
    /// 构建 Authorization 头值。
    pub fn header_value(&self, token: &str) -> String {
        match self {
            AuthScheme::Bearer => format!("bearer {token}"),
            AuthScheme::Token => format!("token {token}"),
        }
    }
}

// ---------------------------------------------------------------------------
// 模型配置
// ---------------------------------------------------------------------------

/// 单个 AI 服务模型的配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiServiceModel {
    /// 模型 ID（如 `PaddleOCR-VL-1.6`），同一提供商内不可重复。
    pub id: String,
    /// 模型显示名称。
    pub name: String,
    /// 是否启用。
    #[serde(default = "default_true")]
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// 提供商配置
// ---------------------------------------------------------------------------

/// 单个 AI 服务提供商的配置。
///
/// `api_key` 为敏感字段，[`Debug`] 实现中会被掩码为 `***`。
///
/// 提供商专属配置（如 PaddleOCR 的 `api_mode`、`optional_payload`）存于
/// `provider_config`，由对应 provider 自行反序列化。
#[derive(Clone, Serialize, Deserialize)]
pub struct AiServiceProvider {
    /// 提供商唯一标识（如 `paddleocr-aistudio`），全局不可重复。
    pub id: String,
    /// 提供商显示名称（如 `PaddleOCR (AIStudio)`）。
    pub name: String,
    /// 服务类型。
    pub category: ServiceCategory,
    /// 部署模式。
    #[serde(default)]
    pub deployment: DeploymentMode,
    /// API 基础 URL（`Api` 模式必填）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_base_url: Option<String>,
    /// API Key / Token（`Api` 模式必填）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// 认证方案。
    #[serde(default)]
    pub auth_scheme: AuthScheme,
    /// 提供商专属配置（JSON），由 provider 自行解析。
    ///
    /// 例如 PaddleOCR 的配置结构为 [`super::paddleocr::PaddleOcrConfig`]。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_config: Option<serde_json::Value>,
    /// 该提供商下的模型列表。
    #[serde(default)]
    pub models: Vec<AiServiceModel>,
    /// 当前激活的模型 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_model_id: Option<String>,
    /// 是否启用。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 创建时间（ISO 8601）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// 更新时间（ISO 8601）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl std::fmt::Debug for AiServiceProvider {
    /// 手动实现 Debug：`api_key` 值掩码为 `***`，避免日志泄露。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiServiceProvider")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("category", &self.category)
            .field("deployment", &self.deployment)
            .field("api_base_url", &self.api_base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "***"))
            .field("auth_scheme", &self.auth_scheme)
            .field("provider_config", &self.provider_config)
            .field("models", &self.models)
            .field("active_model_id", &self.active_model_id)
            .field("enabled", &self.enabled)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

impl AiServiceProvider {
    /// 校验当前提供商配置的完整性。
    ///
    /// # 校验规则
    /// - `id` 非空
    /// - `deployment == Api` 时 `api_base_url` 与 `api_key` 必填
    /// - `models` 中各 `id` 在同一提供商内不重复
    pub fn validate(&self) -> Result<(), AiServiceError> {
        if self.id.trim().is_empty() {
            return Err(AiServiceError::Validation("provider id 不能为空".into()));
        }

        if self.deployment == DeploymentMode::Api {
            if self.api_base_url.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true) {
                return Err(AiServiceError::Validation(
                    "Api 模式下 api_base_url 必填".into(),
                ));
            }
            // api_key 可在 onboarding 阶段暂不填写，用户可在设置中后续补充。
        }

        let mut seen: HashSet<&str> = HashSet::new();
        for model in &self.models {
            if !seen.insert(model.id.as_str()) {
                return Err(AiServiceError::Validation(format!(
                    "模型 ID '{}' 在同一提供商内重复",
                    model.id
                )));
            }
        }

        Ok(())
    }

    /// 按 ID 查找模型。
    pub fn find_model(&self, id: &str) -> Option<&AiServiceModel> {
        self.models.iter().find(|m| m.id == id)
    }

    /// 获取当前激活的模型。
    pub fn active_model(&self) -> Option<&AiServiceModel> {
        self.active_model_id
            .as_ref()
            .and_then(|id| self.find_model(id))
    }
}

// ---------------------------------------------------------------------------
// 顶层配置
// ---------------------------------------------------------------------------

/// AI 服务配置顶层结构，对应 `ai_services_config.json` 文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiServicesConfig {
    /// 配置文件版本号。
    #[serde(default = "default_version")]
    pub version: String,
    /// 每个服务类型的活跃提供商 ID。
    #[serde(default)]
    pub active_providers: HashMap<ServiceCategory, String>,
    /// 提供商列表。
    #[serde(default)]
    pub providers: Vec<AiServiceProvider>,
}

impl Default for AiServicesConfig {
    fn default() -> Self {
        Self {
            version: default_version(),
            active_providers: HashMap::new(),
            providers: Vec::new(),
        }
    }
}

impl AiServicesConfig {
    /// 校验全部配置。
    ///
    /// 逐个校验 [`AiServiceProvider`]，并检查提供商 ID 全局唯一。
    pub fn validate(&self) -> Result<(), AiServiceError> {
        let mut seen: HashSet<&str> = HashSet::new();
        for provider in &self.providers {
            provider.validate()?;
            if !seen.insert(provider.id.as_str()) {
                return Err(AiServiceError::Validation(format!(
                    "提供商 ID '{}' 重复",
                    provider.id
                )));
            }
        }
        Ok(())
    }

    /// 按 ID 查找提供商。
    pub fn find_provider(&self, id: &str) -> Option<&AiServiceProvider> {
        self.providers.iter().find(|p| p.id == id)
    }

    /// 按 ID 查找提供商（可变引用）。
    pub fn find_provider_mut(&mut self, id: &str) -> Option<&mut AiServiceProvider> {
        self.providers.iter_mut().find(|p| p.id == id)
    }

    /// 获取指定服务类型的活跃提供商。
    pub fn active_provider(&self, category: ServiceCategory) -> Option<&AiServiceProvider> {
        self.active_providers
            .get(&category)
            .and_then(|id| self.find_provider(id))
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

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_provider() -> AiServiceProvider {
        AiServiceProvider {
            id: "paddleocr-aistudio".into(),
            name: "PaddleOCR (AIStudio)".into(),
            category: ServiceCategory::Ocr,
            deployment: DeploymentMode::Api,
            api_base_url: Some("https://paddleocr.aistudio-app.com/api/v2/ocr/jobs".into()),
            api_key: Some("test-token".into()),
            auth_scheme: AuthScheme::Bearer,
            provider_config: Some(serde_json::json!({
                "api_mode": "job",
                "options": {
                    "use_doc_orientation_classify": false,
                    "use_doc_unwarping": false,
                    "use_chart_recognition": false
                }
            })),
            models: vec![AiServiceModel {
                id: "PaddleOCR-VL-1.6".into(),
                name: "PaddleOCR VL 1.6".into(),
                enabled: true,
            }],
            active_model_id: Some("PaddleOCR-VL-1.6".into()),
            enabled: true,
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
        assert!(p.validate().is_err());
    }

    #[test]
    fn provider_validate_api_without_base_url() {
        let mut p = sample_provider();
        p.api_base_url = None;
        assert!(p.validate().is_err());
    }

    #[test]
    fn provider_validate_api_without_api_key_ok() {
        // api_key 可在 onboarding 阶段暂不填写
        let mut p = sample_provider();
        p.api_key = None;
        assert!(p.validate().is_ok());
    }

    #[test]
    fn provider_validate_duplicate_model_id() {
        let mut p = sample_provider();
        p.models.push(AiServiceModel {
            id: "PaddleOCR-VL-1.6".into(),
            name: "Duplicate".into(),
            enabled: true,
        });
        assert!(p.validate().is_err());
    }

    #[test]
    fn config_validate_duplicate_provider_id() {
        let p = sample_provider();
        let config = AiServicesConfig {
            providers: vec![p.clone(), p],
            ..AiServicesConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn active_provider_lookup() {
        let config = AiServicesConfig {
            active_providers: HashMap::from([(ServiceCategory::Ocr, "paddleocr-aistudio".into())]),
            providers: vec![sample_provider()],
            ..AiServicesConfig::default()
        };
        assert!(config.active_provider(ServiceCategory::Ocr).is_some());
        assert!(config.active_provider(ServiceCategory::Tts).is_none());
    }

    #[test]
    fn debug_masks_api_key() {
        let p = sample_provider();
        let debug_str = format!("{p:?}");
        assert!(debug_str.contains("***"));
        assert!(!debug_str.contains("test-token"));
    }

    #[test]
    fn json_roundtrip_preserves_data() {
        let config = AiServicesConfig {
            active_providers: HashMap::from([(ServiceCategory::Ocr, "paddleocr-aistudio".into())]),
            providers: vec![sample_provider()],
            ..AiServicesConfig::default()
        };
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: AiServicesConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, "1.0.0");
        assert_eq!(parsed.providers.len(), 1);
        assert_eq!(parsed.providers[0].id, "paddleocr-aistudio");
        assert_eq!(parsed.providers[0].api_key, Some("test-token".into()));
    }

    #[test]
    fn auth_scheme_header_value() {
        assert_eq!(AuthScheme::Bearer.header_value("abc"), "bearer abc");
        assert_eq!(AuthScheme::Token.header_value("abc"), "token abc");
    }
}
