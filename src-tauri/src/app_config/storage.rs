//! App 配置的存储层。
//!
//! 负责跨平台路径解析与 `app_config.json` 文件的读写。
//! 写入采用原子操作（先写 `.tmp` 再 `rename`），避免读到半写状态。
//!
//! ## 性能优化
//!
//! 与 [`crate::llm_config::storage::ConfigStorage`] 不同，本模块内置
//! **内存缓存**（`RwLock<Option<AppConfig>>`），首次读取后缓存于内存，
//! 后续读取直接返回缓存副本，避免重复磁盘 I/O。写入时同步更新缓存。

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use super::error::AppConfigError;
use super::model::AppConfig;
use crate::platform::{fluen_config_dir, PlatformError};

/// 配置文件名。
const CONFIG_FILE_NAME: &str = "app_config.json";

/// App 配置存储（带内存缓存）。
///
/// 封装配置文件的路径解析与读写操作。通过 [`AppConfigStorage::new`]
/// 自动解析当前平台路径，或通过 [`AppConfigStorage::with_dir`] 指定
/// 自定义目录（用于测试）。
///
/// 内部使用 [`RwLock`] 缓存配置，读操作无锁竞争，写操作排他。
pub struct AppConfigStorage {
    /// 配置目录（由 [`crate::platform::fluen_config_dir`] 解析）。
    config_dir: PathBuf,
    /// 配置文件完整路径。
    config_file: PathBuf,
    /// 内存缓存（懒加载）。
    cache: RwLock<Option<AppConfig>>,
}

impl AppConfigStorage {
    /// 创建存储实例，自动解析当前平台的配置目录。
    ///
    /// # 平台路径
    ///
    /// | 平台 | 路径 |
    /// |------|------|
    /// | macOS | `~/Library/Application Support/com.wppcp.fluen/app_config.json` |
    /// | Windows | `%APPDATA%\Fluen\app_config.json` |
    /// | Linux | `~/.config/Fluen/app_config.json`（或 `$XDG_CONFIG_HOME/Fluen/`） |
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
    /// 文件不存在时返回默认配置（[`AppConfig::default`]），不视为错误。
    /// 文件存在但解析或校验失败时返回对应错误。
    ///
    /// 调用此方法会**覆盖**内存缓存。
    pub fn load_from_disk(&self) -> Result<AppConfig, AppConfigError> {
        let config = match std::fs::read_to_string(&self.config_file) {
            Ok(content) => {
                let config: AppConfig = serde_json::from_str(&content)?;
                config.validate()?;
                config
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => AppConfig::default(),
            Err(e) => return Err(e.into()),
        };

        // 更新缓存
        *self.cache.write().unwrap() = Some(config.clone());
        Ok(config)
    }

    /// 获取配置（优先从缓存读取，缓存未命中时从磁盘加载）。
    ///
    /// 这是前端读取配置的推荐入口，避免每次都命中磁盘。
    pub fn get(&self) -> Result<AppConfig, AppConfigError> {
        // 先尝试读缓存
        {
            let cache = self.cache.read().unwrap();
            if let Some(ref config) = *cache {
                return Ok(config.clone());
            }
        }
        // 缓存未命中，从磁盘加载
        self.load_from_disk()
    }

    /// 保存配置（原子写入）并更新缓存。
    ///
    /// 先校验配置完整性，然后写入同目录下的临时文件 `app_config.json.tmp`，
    /// 最后 `rename` 覆盖目标文件。父目录按需创建。
    pub fn save(&self, config: &AppConfig) -> Result<(), AppConfigError> {
        config.validate()?;

        std::fs::create_dir_all(&self.config_dir)?;

        let tmp = self.config_file.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(config)?;
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &self.config_file)?;

        // 更新缓存
        *self.cache.write().unwrap() = Some(config.clone());

        Ok(())
    }

    /// 清除内存缓存，强制下次读取从磁盘加载。
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
    use crate::app_config::model::{AppConfig, Language, ThemeMode};

    fn sample_config() -> AppConfig {
        AppConfig {
            version: "1.0.0".into(),
            theme: ThemeMode::Dark,
            language: Language::En,
            onboarding_completed: true,
            logging: crate::logging::LogConfig::default(),
            recent_projects_count: 4,
        }
    }

    #[test]
    fn load_missing_file_returns_default() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_app_test_{}_default",
            std::process::id()
        ));
        let storage = AppConfigStorage::with_dir(&tmp);
        let config = storage.load_from_disk().unwrap();
        assert_eq!(config.version, "1.0.0");
        assert_eq!(config.theme, ThemeMode::Light);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_then_load_roundtrip() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_app_test_{}_roundtrip",
            std::process::id()
        ));
        let storage = AppConfigStorage::with_dir(&tmp);
        let original = sample_config();

        storage.save(&original).unwrap();
        assert!(storage.path().exists());

        let loaded = storage.load_from_disk().unwrap();
        assert_eq!(loaded.version, original.version);
        assert_eq!(loaded.theme, original.theme);
        assert_eq!(loaded.language, original.language);
        assert_eq!(loaded.onboarding_completed, original.onboarding_completed);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn get_uses_cache_after_load() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_app_test_{}_cache",
            std::process::id()
        ));
        let storage = AppConfigStorage::with_dir(&tmp);
        let original = sample_config();
        storage.save(&original).unwrap();

        // 第一次 get 从缓存（save 时已缓存）
        let config1 = storage.get().unwrap();
        assert_eq!(config1.theme, ThemeMode::Dark);

        // 清除缓存后重新加载
        storage.invalidate_cache();
        let config2 = storage.get().unwrap();
        assert_eq!(config2.theme, ThemeMode::Dark);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_invalid_config_fails() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_app_test_{}_invalid",
            std::process::id()
        ));
        let storage = AppConfigStorage::with_dir(&tmp);

        let mut config = sample_config();
        config.version = "  ".into();

        let result = storage.save(&config);
        assert!(result.is_err());
        assert!(!storage.path().exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_creates_parent_dir() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_app_test_{}_mkdir/nested/deep",
            std::process::id()
        ));
        let storage = AppConfigStorage::with_dir(&tmp);
        let config = sample_config();

        storage.save(&config).unwrap();
        assert!(storage.path().exists());

        let _ = std::fs::remove_dir_all(
            std::env::temp_dir()
                .join(format!("fluen_app_test_{}_mkdir", std::process::id())),
        );
    }

    #[test]
    fn invalidate_cache_forces_disk_read() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_app_test_{}_invalidate",
            std::process::id()
        ));
        let storage = AppConfigStorage::with_dir(&tmp);
        let config = sample_config();
        storage.save(&config).unwrap();

        // 清除缓存
        storage.invalidate_cache();

        // 重新读取仍应返回正确值
        let loaded = storage.get().unwrap();
        assert_eq!(loaded.theme, ThemeMode::Dark);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
