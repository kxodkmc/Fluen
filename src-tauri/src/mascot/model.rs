//! 宠物助手的纯数据模型与校验逻辑。
//!
//! 所有结构体仅承载数据，不涉及文件 I/O。
//! 序列化格式为 JSON，设计目标是清晰、易读、易扩展。

use serde::{Deserialize, Serialize};

use super::error::MascotError;

// ---------------------------------------------------------------------------
// 心情
// ---------------------------------------------------------------------------

/// 宠物心情。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mood {
    /// 开心。
    Happy,
    /// 平静（默认）。
    #[default]
    Neutral,
    /// 难过。
    Sad,
}

// ---------------------------------------------------------------------------
// 宠物配置
// ---------------------------------------------------------------------------

/// 宠物助手配置顶层结构，对应 `mascot_config.json` 文件。
///
/// 存储宠物助手的全局配置（启用状态、人格、能力开关等），
/// 与 [`MascotData`] 分离——后者仅承载运行时状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MascotConfig {
    /// 配置文件版本号。
    #[serde(default = "default_version")]
    pub version: String,
    /// 宠物名称。
    #[serde(default = "default_name")]
    pub name: String,
    /// 是否启用宠物助手。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// LLM 提供商 ID（可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    /// LLM 模型 ID（可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// 是否启用 MCP 工具调用。
    #[serde(default)]
    pub mcp_enabled: bool,
    /// 是否启用 Skills 能力。
    #[serde(default)]
    pub skills_enabled: bool,
    /// 是否启用函数调用。
    #[serde(default)]
    pub function_calling_enabled: bool,
    /// 宠物人格描述。
    #[serde(default = "default_personality")]
    pub personality: String,
    /// 是否展示详细思考内容（默认 false，仅显示"思考中…"）。
    #[serde(default)]
    pub show_thinking_content: bool,
    /// 是否使用专业化表述（默认 false，使用拟人化文案）。
    #[serde(default)]
    pub professional_expression: bool,
    /// 已启用的子智能体 ID 列表（空列表 = 全部可用）。
    ///
    /// 控制 Motis 总督角色可通过 `delegate_agent` 调度的子智能体集合。
    /// 在设置页面可逐个开关。值为子智能体 ID 字符串（如 `"academic_writer"`）。
    #[serde(default)]
    pub enabled_agents: Vec<String>,
}

impl Default for MascotConfig {
    fn default() -> Self {
        Self {
            version: default_version(),
            name: default_name(),
            enabled: default_true(),
            provider_id: None,
            model_id: None,
            mcp_enabled: false,
            skills_enabled: false,
            function_calling_enabled: false,
            personality: default_personality(),
            show_thinking_content: false,
            professional_expression: false,
            enabled_agents: Vec::new(), // 空列表 = 全部可用
        }
    }
}

impl MascotConfig {
    /// 校验配置完整性。
    ///
    /// 当前仅校验版本号非空，后续可按需扩展。
    pub fn validate(&self) -> Result<(), MascotError> {
        if self.version.trim().is_empty() {
            return Err(MascotError::Validation(
                "version 不能为空".into(),
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 宠物运行时数据
// ---------------------------------------------------------------------------

/// 宠物助手运行时数据顶层结构，对应 `mascot_data.json` 文件。
///
/// 存储运行时可变状态（心情、好感度等），与 [`MascotConfig`] 分离，
/// 便于配置与数据独立持久化。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MascotData {
    /// 数据文件版本号。
    #[serde(default = "default_version")]
    pub version: String,
    /// 当前心情。
    #[serde(default)]
    pub mood: Mood,
    /// 好感度（0-100）。
    #[serde(default = "default_zero")]
    pub affinity: u32,
    /// 心情最近更新时间（ISO 8601 字符串，可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mood_updated_at: Option<String>,
    /// 好感度最近更新时间（ISO 8601 字符串，可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affinity_updated_at: Option<String>,
}

impl Default for MascotData {
    fn default() -> Self {
        Self {
            version: default_version(),
            mood: Mood::default(),
            affinity: default_zero(),
            mood_updated_at: None,
            affinity_updated_at: None,
        }
    }
}

impl MascotData {
    /// 校验数据完整性。
    ///
    /// 校验版本号非空，且好感度位于 0-100 范围内。
    pub fn validate(&self) -> Result<(), MascotError> {
        if self.version.trim().is_empty() {
            return Err(MascotError::Validation(
                "version 不能为空".into(),
            ));
        }
        if self.affinity > 100 {
            return Err(MascotError::Validation(
                "affinity 必须在 0-100 范围内".into(),
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// serde 辅助函数
// ---------------------------------------------------------------------------

fn default_true() -> bool {
    true
}

fn default_zero() -> u32 {
    0
}

fn default_version() -> String {
    "1.0.0".to_string()
}

fn default_name() -> String {
    "Motis".to_string()
}

fn default_personality() -> String {
    "cheerful".to_string()
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = MascotConfig::default();
        assert_eq!(config.version, "1.0.0");
        assert_eq!(config.name, "Motis");
        assert!(config.enabled);
        assert_eq!(config.provider_id, None);
        assert_eq!(config.model_id, None);
        assert!(!config.mcp_enabled);
        assert!(!config.skills_enabled);
        assert!(!config.function_calling_enabled);
        assert_eq!(config.personality, "cheerful");
        assert!(!config.show_thinking_content);
        assert!(!config.professional_expression);
        assert!(config.enabled_agents.is_empty());
    }

    #[test]
    fn validate_config_ok() {
        assert!(MascotConfig::default().validate().is_ok());
    }

    #[test]
    fn validate_empty_version() {
        let mut config = MascotConfig::default();
        config.version = "  ".into();
        assert!(config.validate().is_err());
    }

    #[test]
    fn json_roundtrip_config() {
        let config = MascotConfig {
            version: "1.0.0".into(),
            name: "小Fluen".into(),
            enabled: false,
            provider_id: Some("openai".into()),
            model_id: Some("gpt-4".into()),
            mcp_enabled: true,
            skills_enabled: true,
            function_calling_enabled: true,
            personality: "calm".into(),
            show_thinking_content: true,
            professional_expression: true,
            enabled_agents: vec!["academic_writer".into(), "data_analyst".into()],
        };
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: MascotConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, "1.0.0");
        assert_eq!(parsed.name, "小Fluen");
        assert!(!parsed.enabled);
        assert_eq!(parsed.provider_id.as_deref(), Some("openai"));
        assert_eq!(parsed.model_id.as_deref(), Some("gpt-4"));
        assert!(parsed.mcp_enabled);
        assert!(parsed.skills_enabled);
        assert!(parsed.function_calling_enabled);
        assert_eq!(parsed.personality, "calm");
        assert!(parsed.show_thinking_content);
        assert!(parsed.professional_expression);
        assert_eq!(parsed.enabled_agents, vec!["academic_writer", "data_analyst"]);
    }

    #[test]
    fn default_data() {
        let data = MascotData::default();
        assert_eq!(data.version, "1.0.0");
        assert_eq!(data.mood, Mood::Neutral);
        assert_eq!(data.affinity, 0);
        assert_eq!(data.mood_updated_at, None);
        assert_eq!(data.affinity_updated_at, None);
    }

    #[test]
    fn validate_data_ok() {
        assert!(MascotData::default().validate().is_ok());
    }

    #[test]
    fn validate_affinity_out_of_range() {
        let mut data = MascotData::default();
        data.affinity = 101;
        assert!(data.validate().is_err());

        data.affinity = 100;
        assert!(data.validate().is_ok());

        data.affinity = 0;
        assert!(data.validate().is_ok());
    }

    #[test]
    fn json_roundtrip_data() {
        let data = MascotData {
            version: "1.0.0".into(),
            mood: Mood::Happy,
            affinity: 42,
            mood_updated_at: Some("2026-07-11T00:00:00Z".into()),
            affinity_updated_at: Some("2026-07-11T00:00:00Z".into()),
        };
        let json = serde_json::to_string_pretty(&data).unwrap();
        let parsed: MascotData = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, "1.0.0");
        assert_eq!(parsed.mood, Mood::Happy);
        assert_eq!(parsed.affinity, 42);
        assert_eq!(parsed.mood_updated_at.as_deref(), Some("2026-07-11T00:00:00Z"));
        assert_eq!(parsed.affinity_updated_at.as_deref(), Some("2026-07-11T00:00:00Z"));
    }

    #[test]
    fn serde_mood_lowercase() {
        let json = serde_json::to_string(&Mood::Happy).unwrap();
        assert_eq!(json, "\"happy\"");

        let neutral: Mood = serde_json::from_str("\"neutral\"").unwrap();
        assert_eq!(neutral, Mood::Neutral);

        let sad: Mood = serde_json::from_str("\"sad\"").unwrap();
        assert_eq!(sad, Mood::Sad);
    }
}
