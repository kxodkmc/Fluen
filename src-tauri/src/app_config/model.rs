//! App 配置的纯数据模型与校验逻辑。
//!
//! 所有结构体仅承载数据，不涉及文件 I/O。
//! 序列化格式为 JSON，设计目标是清晰、易读、易扩展。

use serde::{Deserialize, Serialize};

use super::error::AppConfigError;
use crate::logging::LogConfig;

// ---------------------------------------------------------------------------
// 主题模式
// ---------------------------------------------------------------------------

/// 主题模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// 浅色模式（默认）。
    #[default]
    Light,
    /// 深色模式。
    Dark,
}

// ---------------------------------------------------------------------------
// 界面语言
// ---------------------------------------------------------------------------

/// 界面语言。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    /// 简体中文（默认）。
    #[default]
    #[serde(rename = "zh-CN")]
    ZhCN,
    /// 英文。
    #[serde(rename = "en")]
    En,
    /// 西班牙语。
    #[serde(rename = "es")]
    Es,
}

// ---------------------------------------------------------------------------
// 顶层配置
// ---------------------------------------------------------------------------

/// App 配置顶层结构，对应 `app_config.json` 文件。
///
/// 存储应用级别的用户偏好设置（主题、语言等），
/// 与 [`crate::llm_config::model::LlmConfig`] 分离，职责单一。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 配置文件版本号。
    #[serde(default = "default_version")]
    pub version: String,
    /// 主题模式。
    #[serde(default)]
    pub theme: ThemeMode,
    /// 界面语言。
    #[serde(default)]
    pub language: Language,
    /// 是否已完成 onboarding 流程。
    #[serde(default)]
    pub onboarding_completed: bool,
    /// 日志系统配置。
    #[serde(default)]
    pub logging: LogConfig,
    /// 最近打开项目显示数量（1-8，默认 4）。
    ///
    /// 控制欢迎页"最近打开"区域展示的项目数量上限。
    #[serde(default = "default_recent_projects_count")]
    pub recent_projects_count: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: default_version(),
            theme: ThemeMode::default(),
            language: Language::default(),
            onboarding_completed: false,
            logging: LogConfig::default(),
            recent_projects_count: default_recent_projects_count(),
        }
    }
}

impl AppConfig {
    /// 校验配置完整性。
    ///
    /// 校验版本号非空、`recent_projects_count` 在 1-8 范围、日志配置合法。
    pub fn validate(&self) -> Result<(), AppConfigError> {
        if self.version.trim().is_empty() {
            return Err(AppConfigError::Validation(
                "version 不能为空".into(),
            ));
        }
        if !(1..=8).contains(&self.recent_projects_count) {
            return Err(AppConfigError::Validation(format!(
                "recent_projects_count 必须在 1-8 范围内，当前为: {}",
                self.recent_projects_count
            )));
        }
        self.logging
            .validate()
            .map_err(AppConfigError::Validation)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// serde 辅助函数
// ---------------------------------------------------------------------------

fn default_version() -> String {
    "1.0.0".to_string()
}

/// `recent_projects_count` 字段的默认值。
fn default_recent_projects_count() -> u32 {
    4
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = AppConfig::default();
        assert_eq!(config.version, "1.0.0");
        assert_eq!(config.theme, ThemeMode::Light);
        assert_eq!(config.language, Language::ZhCN);
        assert!(!config.onboarding_completed);
        assert_eq!(config.recent_projects_count, 4);
    }

    #[test]
    fn validate_ok() {
        assert!(AppConfig::default().validate().is_ok());
    }

    #[test]
    fn validate_empty_version() {
        let mut config = AppConfig::default();
        config.version = "  ".into();
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_recent_projects_count_out_of_range() {
        let mut config = AppConfig::default();
        config.recent_projects_count = 0;
        assert!(config.validate().is_err());

        config.recent_projects_count = 9;
        assert!(config.validate().is_err());

        config.recent_projects_count = 1;
        assert!(config.validate().is_ok());

        config.recent_projects_count = 8;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn json_roundtrip() {
        let config = AppConfig {
            version: "1.0.0".into(),
            theme: ThemeMode::Dark,
            language: Language::En,
            onboarding_completed: true,
            logging: LogConfig::default(),
            recent_projects_count: 6,
        };
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, "1.0.0");
        assert_eq!(parsed.theme, ThemeMode::Dark);
        assert_eq!(parsed.language, Language::En);
        assert!(parsed.onboarding_completed);
        assert_eq!(parsed.recent_projects_count, 6);
    }

    #[test]
    fn serde_theme_lowercase() {
        let json = serde_json::to_string(&ThemeMode::Dark).unwrap();
        assert_eq!(json, "\"dark\"");

        let light: ThemeMode = serde_json::from_str("\"light\"").unwrap();
        assert_eq!(light, ThemeMode::Light);
    }

    #[test]
    fn serde_language_kebab_case() {
        let json = serde_json::to_string(&Language::ZhCN).unwrap();
        assert_eq!(json, "\"zh-CN\"");

        let en: Language = serde_json::from_str("\"en\"").unwrap();
        assert_eq!(en, Language::En);

        let es_json = serde_json::to_string(&Language::Es).unwrap();
        assert_eq!(es_json, "\"es\"");

        let es: Language = serde_json::from_str("\"es\"").unwrap();
        assert_eq!(es, Language::Es);
    }

    #[test]
    fn default_deserialize_missing_fields() {
        // 仅 version 字段，其余应使用默认值
        let json = r#"{"version":"2.0.0"}"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.version, "2.0.0");
        assert_eq!(config.theme, ThemeMode::Light);
        assert_eq!(config.language, Language::ZhCN);
        assert!(!config.onboarding_completed);
        assert_eq!(config.recent_projects_count, 4);
    }
}
