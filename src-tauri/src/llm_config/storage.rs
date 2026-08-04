//! LLM 配置的存储层。
//!
//! 负责跨平台路径解析与 `llm_config.json` 文件的读写。
//! 写入采用原子操作（先写 `.tmp` 再 `rename`），避免读到半写状态。

use std::path::{Path, PathBuf};

use super::error::LlmConfigError;
use super::model::LlmConfig;
use crate::platform::{fluen_config_dir, PlatformError};

/// 配置文件名。
const CONFIG_FILE_NAME: &str = "llm_config.json";

/// LLM 配置存储。
///
/// 封装配置文件的路径解析与读写操作。通过 [`ConfigStorage::new`] 自动解析
/// 当前平台路径，或通过 [`ConfigStorage::with_dir`] 指定自定义目录（用于测试）。
///
/// 实现 `Clone`：仅包含 `PathBuf`，克隆廉价，便于跨任务传递（如 task_queue runner）。
#[derive(Clone)]
pub struct ConfigStorage {
    /// 配置目录（由 [`crate::platform::fluen_config_dir`] 解析）。
    config_dir: PathBuf,
    /// 配置文件完整路径。
    config_file: PathBuf,
}

impl ConfigStorage {
    /// 创建存储实例，自动解析当前平台的配置目录。
    ///
    /// # 平台路径
    ///
    /// | 平台 | 路径 |
    /// |------|------|
    /// | macOS | `~/Library/Application Support/com.wppcp.fluen/llm_config.json` |
    /// | Windows | `%APPDATA%\Fluen\llm_config.json` |
    /// | Linux | `~/.config/Fluen/llm_config.json`（或 `$XDG_CONFIG_HOME/Fluen/`） |
    pub fn new() -> Result<Self, PlatformError> {
        let config_dir = fluen_config_dir()?;
        let config_file = config_dir.join(CONFIG_FILE_NAME);
        Ok(Self {
            config_dir,
            config_file,
        })
    }

    /// 使用自定义目录创建存储实例（主要用于测试）。
    pub fn with_dir(dir: impl AsRef<Path>) -> Self {
        let config_dir = dir.as_ref().to_path_buf();
        let config_file = config_dir.join(CONFIG_FILE_NAME);
        Self {
            config_dir,
            config_file,
        }
    }

    /// 返回配置文件的完整路径。
    pub fn path(&self) -> &Path {
        &self.config_file
    }

    /// 加载配置。
    ///
    /// 文件不存在时返回默认空配置（[`LlmConfig::default`]），不视为错误。
    /// 文件存在但解析或校验失败时返回对应错误。
    pub fn load(&self) -> Result<LlmConfig, LlmConfigError> {
        match std::fs::read_to_string(&self.config_file) {
            Ok(content) => {
                let config: LlmConfig = serde_json::from_str(&content)?;
                config.validate()?;
                Ok(config)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(LlmConfig::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// 保存配置（原子写入）。
    ///
    /// 先校验配置完整性，然后写入同目录下的临时文件 `llm_config.json.tmp`，
    /// 最后 `rename` 覆盖目标文件。父目录按需创建。
    pub fn save(&self, config: &LlmConfig) -> Result<(), LlmConfigError> {
        config.validate()?;

        std::fs::create_dir_all(&self.config_dir)?;

        let tmp = self.config_file.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(config)?;
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &self.config_file)?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_config::model::{
        LlmConfig, ModelCapabilities, ModelConfig, ProviderConfig, ProviderType,
    };
    use confluent::llmkit::ApiStyle;
    use std::collections::HashMap;

    fn sample_config() -> LlmConfig {
        LlmConfig {
            version: "1.0.0".into(),
            active_provider_id: Some("test".into()),
            active_model_id: Some("model-1".into()),
            providers: vec![ProviderConfig {
                id: "test".into(),
                name: "Test Provider".into(),
                provider_type: ProviderType::Custom,
                openai_base_url: Some("https://api.test.com/v1/chat/completions".into()),
                anthropic_base_url: None,
                api_key: Some("sk-test".into()),
                default_style: ApiStyle::OpenAI,
                extra_headers: HashMap::new(),
                enabled: true,
                models: vec![ModelConfig {
                    id: "model-1".into(),
                    name: "Model 1".into(),
                    capabilities: ModelCapabilities::default(),
                    max_output_tokens: Some(4096),
                    context_window: Some(32000),
                    description: None,
                    enabled: true,
                }],
                created_at: None,
                updated_at: None,
            }],
            scene_models: None,
            embedding: None,
        }
    }

    #[test]
    fn load_missing_file_returns_default() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_test_{}_load_default",
            std::process::id()
        ));
        let storage = ConfigStorage::with_dir(&tmp);
        let config = storage.load().unwrap();
        assert!(config.providers.is_empty());
        assert_eq!(config.version, "1.0.0");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_then_load_roundtrip() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_test_{}_roundtrip",
            std::process::id()
        ));
        let storage = ConfigStorage::with_dir(&tmp);
        let original = sample_config();

        storage.save(&original).unwrap();
        assert!(storage.path().exists());

        let loaded = storage.load().unwrap();
        assert_eq!(loaded.version, original.version);
        assert_eq!(loaded.active_provider_id, original.active_provider_id);
        assert_eq!(loaded.providers.len(), 1);
        assert_eq!(loaded.providers[0].id, "test");
        assert_eq!(loaded.providers[0].models.len(), 1);
        assert_eq!(loaded.providers[0].api_key, Some("sk-test".into()));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_invalid_config_fails() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_test_{}_invalid",
            std::process::id()
        ));
        let storage = ConfigStorage::with_dir(&tmp);

        let mut config = sample_config();
        config.providers[0].openai_base_url = None;
        config.providers[0].anthropic_base_url = None;

        let result = storage.save(&config);
        assert!(result.is_err());
        assert!(!storage.path().exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_creates_parent_dir() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_test_{}_mkdir/nested/deep",
            std::process::id()
        ));
        let storage = ConfigStorage::with_dir(&tmp);
        let config = sample_config();

        storage.save(&config).unwrap();
        assert!(storage.path().exists());

        let _ = std::fs::remove_dir_all(
            std::env::temp_dir()
                .join(format!("fluen_test_{}_mkdir", std::process::id())),
        );
    }
}
