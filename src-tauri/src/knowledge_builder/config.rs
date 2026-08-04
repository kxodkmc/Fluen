//! 知识库构建全局配置（应用级，非项目级）。
//!
//! 存储于 `app_config.json` 中，作为 `AppConfig` 的可选扩展字段。
//! 控制"导入完成后是否询问构建"等默认行为。

use serde::{Deserialize, Serialize};

use super::types::KnowledgeBuildOptions;

/// 导入完成后的默认行为。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PostImportBehavior {
    /// 每次询问（默认）。
    #[default]
    AlwaysAsk,
    /// 自动加入队列。
    AlwaysBuild,
    /// 自动跳过。
    NeverBuild,
}

/// 知识库构建全局配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeBuildConfig {
    /// 导入后的默认行为。
    #[serde(default)]
    pub post_import_behavior: PostImportBehavior,
    /// 默认构建选项。
    #[serde(default)]
    pub default_options: KnowledgeBuildOptions,
    /// 自定义 prompt 模板（None 则用内置默认）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_prompt: Option<String>,
}

impl Default for KnowledgeBuildConfig {
    fn default() -> Self {
        Self {
            post_import_behavior: PostImportBehavior::AlwaysAsk,
            default_options: KnowledgeBuildOptions::default(),
            custom_prompt: None,
        }
    }
}

impl KnowledgeBuildConfig {
    /// 配置文件版本。
    pub const VERSION: &'static str = "1.0.0";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_always_ask() {
        let cfg = KnowledgeBuildConfig::default();
        assert_eq!(cfg.post_import_behavior, PostImportBehavior::AlwaysAsk);
        assert!(cfg.custom_prompt.is_none());
    }

    #[test]
    fn roundtrip() {
        let cfg = KnowledgeBuildConfig {
            post_import_behavior: PostImportBehavior::AlwaysBuild,
            default_options: KnowledgeBuildOptions {
                max_concepts: Some(20),
                ..KnowledgeBuildOptions::default()
            },
            custom_prompt: Some("自定义 prompt".into()),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: KnowledgeBuildConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.post_import_behavior, PostImportBehavior::AlwaysBuild);
        assert_eq!(parsed.default_options.max_concepts, Some(20));
        assert_eq!(parsed.custom_prompt.as_deref(), Some("自定义 prompt"));
    }

    #[test]
    fn deserialize_partial() {
        let json = r#"{"post_import_behavior":"never_build"}"#;
        let cfg: KnowledgeBuildConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.post_import_behavior, PostImportBehavior::NeverBuild);
        assert!(cfg.custom_prompt.is_none());
    }
}
