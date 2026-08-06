//! AI 服务配置的存储层。
//!
//! 负责跨平台路径解析与 `ai_services_config.json` 文件的读写。
//! 写入采用原子操作（先写 `.tmp` 再 `rename`），避免读到半写状态。
//!
//! 内置 `RwLock` 内存缓存，首次读取后缓存于内存，后续读取直接返回缓存副本。

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use super::error::AiServiceError;
use super::model::AiServicesConfig;
use crate::platform::{fluen_config_dir, PlatformError};

/// 配置文件名。
const CONFIG_FILE_NAME: &str = "ai_services_config.json";

/// AI 服务配置存储（带内存缓存）。
///
/// 封装配置文件的路径解析与读写操作。通过 [`ConfigStorage::new`]
/// 自动解析当前平台路径，或通过 [`ConfigStorage::with_dir`] 指定
/// 自定义目录（用于测试）。
pub struct ConfigStorage {
    /// 配置目录（由 [`crate::platform::fluen_config_dir`] 解析）。
    config_dir: PathBuf,
    /// 配置文件完整路径。
    config_file: PathBuf,
    /// 内存缓存（懒加载）。
    cache: RwLock<Option<AiServicesConfig>>,
}

impl Clone for ConfigStorage {
    /// 克隆存储实例（含当前内存缓存快照）。
    ///
    /// 供 [`crate::task_queue`] 以 `Arc` 共享给 runner 使用：
    /// 每个 runner 持有一份独立句柄，读写同一配置文件，缓存相互独立无竞争。
    fn clone(&self) -> Self {
        let cache_snapshot = self.cache.read().unwrap().clone();
        Self {
            config_dir: self.config_dir.clone(),
            config_file: self.config_file.clone(),
            cache: RwLock::new(cache_snapshot),
        }
    }
}

impl ConfigStorage {
    /// 创建存储实例，自动解析当前平台的配置目录。
    pub fn new() -> Result<Self, PlatformError> {
        let config_dir = fluen_config_dir()?;
        let config_file = config_dir.join(CONFIG_FILE_NAME);
        Ok(Self {
            config_dir,
            config_file,
            cache: RwLock::new(None),
        })
    }

    /// 使用自定义目录创建存储实例（主要用于测试）。
    pub fn with_dir(dir: impl AsRef<Path>) -> Self {
        let config_dir = dir.as_ref().to_path_buf();
        let config_file = config_dir.join(CONFIG_FILE_NAME);
        Self {
            config_dir,
            config_file,
            cache: RwLock::new(None),
        }
    }

    /// 返回配置文件的完整路径。
    pub fn path(&self) -> &Path {
        &self.config_file
    }

    /// 从磁盘加载配置并更新缓存。
    ///
    /// 文件不存在时返回默认空配置，不视为错误。
    pub fn load_from_disk(&self) -> Result<AiServicesConfig, AiServiceError> {
        let config = match std::fs::read_to_string(&self.config_file) {
            Ok(content) => {
                let config: AiServicesConfig = serde_json::from_str(&content)?;
                config.validate()?;
                config
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => AiServicesConfig::default(),
            Err(e) => return Err(e.into()),
        };

        *self.cache.write().unwrap() = Some(config.clone());
        Ok(config)
    }

    /// 获取配置（优先从缓存读取，缓存未命中时从磁盘加载）。
    pub fn get(&self) -> Result<AiServicesConfig, AiServiceError> {
        {
            let cache = self.cache.read().unwrap();
            if let Some(ref config) = *cache {
                return Ok(config.clone());
            }
        }
        self.load_from_disk()
    }

    /// 保存配置（原子写入）并更新缓存。
    pub fn save(&self, config: &AiServicesConfig) -> Result<(), AiServiceError> {
        config.validate()?;

        std::fs::create_dir_all(&self.config_dir)?;

        let tmp = self.config_file.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(config)?;
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &self.config_file)?;

        *self.cache.write().unwrap() = Some(config.clone());

        Ok(())
    }

    /// 清除内存缓存，强制下次读取从磁盘加载。
    #[allow(dead_code)]
    pub fn invalidate_cache(&self) {
        *self.cache.write().unwrap() = None;
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_services::model::{
        AiServiceModel, AiServiceProvider, AuthScheme, DeploymentMode, ServiceCategory,
    };
    use std::collections::HashMap;

    fn sample_config() -> AiServicesConfig {
        AiServicesConfig {
            version: "1.0.0".into(),
            active_providers: HashMap::from([(ServiceCategory::Ocr, "test-ocr".into())]),
            providers: vec![AiServiceProvider {
                id: "test-ocr".into(),
                name: "Test OCR".into(),
                category: ServiceCategory::Ocr,
                deployment: DeploymentMode::Api,
                api_base_url: Some("https://example.com/api".into()),
                api_key: Some("test-key".into()),
                auth_scheme: AuthScheme::Bearer,
                provider_config: None,
                models: vec![AiServiceModel {
                    id: "model-1".into(),
                    name: "Model 1".into(),
                    enabled: true,
                }],
                active_model_id: Some("model-1".into()),
                enabled: true,
                created_at: None,
                updated_at: None,
            }],
        }
    }

    #[test]
    fn load_missing_file_returns_default() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_ai_test_{}_default",
            std::process::id()
        ));
        let storage = ConfigStorage::with_dir(&tmp);
        let config = storage.load_from_disk().unwrap();
        assert!(config.providers.is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_then_load_roundtrip() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_ai_test_{}_roundtrip",
            std::process::id()
        ));
        let storage = ConfigStorage::with_dir(&tmp);
        let original = sample_config();

        storage.save(&original).unwrap();
        assert!(storage.path().exists());

        let loaded = storage.load_from_disk().unwrap();
        assert_eq!(loaded.version, original.version);
        assert_eq!(loaded.providers.len(), 1);
        assert_eq!(loaded.providers[0].id, "test-ocr");
        assert_eq!(loaded.providers[0].api_key, Some("test-key".into()));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn get_uses_cache_after_save() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_ai_test_{}_cache",
            std::process::id()
        ));
        let storage = ConfigStorage::with_dir(&tmp);
        storage.save(&sample_config()).unwrap();

        // get 应从缓存返回
        let config = storage.get().unwrap();
        assert_eq!(config.providers[0].id, "test-ocr");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_invalid_config_fails() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_ai_test_{}_invalid",
            std::process::id()
        ));
        let storage = ConfigStorage::with_dir(&tmp);

        let mut config = sample_config();
        config.providers[0].api_base_url = None;

        let result = storage.save(&config);
        assert!(result.is_err());
        assert!(!storage.path().exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
