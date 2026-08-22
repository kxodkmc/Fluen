//! 宠物助手的存储层。
//!
//! 负责跨平台路径解析与 `mascot_config.json` / `mascot_data.json` 文件的读写。
//! 写入采用原子操作（先写 `.tmp` 再 `rename`），避免读到半写状态。
//!
//! ## 性能优化
//!
//! 与 [`crate::app_config::storage::AppConfigStorage`] 一致，两个 Storage 均内置
//! **内存缓存**（`RwLock<Option<...>>`），首次读取后缓存于内存，
//! 后续读取直接返回缓存副本，避免重复磁盘 I/O。写入时同步更新缓存。

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use super::error::MascotError;
use super::model::{MascotConfig, MascotData};
use crate::platform::{fluen_config_dir, fluen_data_dir, PlatformError};

/// 配置文件名。
const CONFIG_FILE_NAME: &str = "mascot_config.json";

/// 数据文件名。
const DATA_FILE_NAME: &str = "mascot_data.json";

// ---------------------------------------------------------------------------
// 配置存储
// ---------------------------------------------------------------------------

/// 宠物配置存储（带内存缓存）。
///
/// 封装 `mascot_config.json` 的路径解析与读写操作。通过
/// [`MascotConfigStorage::new`] 自动解析当前平台配置目录，或通过
/// [`MascotConfigStorage::with_dir`] 指定自定义目录（用于测试）。
///
/// 内部使用 [`RwLock`] 缓存配置，读操作无锁竞争，写操作排他。
pub struct MascotConfigStorage {
    /// 配置目录（由 [`crate::platform::fluen_config_dir`] 解析）。
    config_dir: PathBuf,
    /// 配置文件完整路径。
    config_file: PathBuf,
    /// 内存缓存（懒加载）。
    cache: RwLock<Option<MascotConfig>>,
}

impl MascotConfigStorage {
    /// 创建存储实例，自动解析当前平台的配置目录。
    ///
    /// # 平台路径
    ///
    /// | 平台 | 路径 |
    /// |------|------|
    /// | macOS | `~/Library/Application Support/com.wppcp.fluen/mascot_config.json` |
    /// | Windows | `%APPDATA%\Fluen\mascot_config.json` |
    /// | Linux | `~/.config/Fluen/mascot_config.json`（或 `$XDG_CONFIG_HOME/Fluen/`） |
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
    /// 文件不存在时返回默认配置（[`MascotConfig::default`]），不视为错误。
    /// 文件存在但解析或校验失败时返回对应错误。
    ///
    /// 调用此方法会**覆盖**内存缓存。
    pub fn load_from_disk(&self) -> Result<MascotConfig, MascotError> {
        let config = match std::fs::read_to_string(&self.config_file) {
            Ok(content) => {
                let config: MascotConfig = serde_json::from_str(&content)?;
                config.validate()?;
                config
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => MascotConfig::default(),
            Err(e) => return Err(e.into()),
        };

        // 更新缓存
        *self.cache.write().unwrap() = Some(config.clone());
        Ok(config)
    }

    /// 获取配置（优先从缓存读取，缓存未命中时从磁盘加载）。
    ///
    /// 这是前端读取配置的推荐入口，避免每次都命中磁盘。
    pub fn get(&self) -> Result<MascotConfig, MascotError> {
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
    /// 先校验配置完整性，然后写入同目录下的临时文件 `mascot_config.json.tmp`，
    /// 最后 `rename` 覆盖目标文件。父目录按需创建。
    pub fn save(&self, config: &MascotConfig) -> Result<(), MascotError> {
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
// 数据存储
// ---------------------------------------------------------------------------

/// 宠物数据存储（带内存缓存）。
///
/// 封装 `mascot_data.json` 的路径解析与读写操作。通过
/// [`MascotDataStorage::new`] 自动解析当前平台数据目录，或通过
/// [`MascotDataStorage::with_dir`] 指定自定义目录（用于测试）。
///
/// 内部使用 [`RwLock`] 缓存数据，读操作无锁竞争，写操作排他。
pub struct MascotDataStorage {
    /// 数据目录（由 [`crate::platform::fluen_data_dir`] 解析）。
    data_dir: PathBuf,
    /// 数据文件完整路径。
    data_file: PathBuf,
    /// 内存缓存（懒加载）。
    cache: RwLock<Option<MascotData>>,
}

impl MascotDataStorage {
    /// 创建存储实例，自动解析当前平台的数据目录。
    ///
    /// # 平台路径
    ///
    /// | 平台 | 路径 |
    /// |------|------|
    /// | macOS | `~/Library/Application Support/com.wppcp.fluen/mascot_data.json` |
    /// | Windows | `%LOCALAPPDATA%\Fluen\mascot_data.json` |
    /// | Linux | `~/.local/share/Fluen/mascot_data.json`（或 `$XDG_DATA_HOME/Fluen/`） |
    pub fn new() -> Result<Self, PlatformError> {
        let data_dir = fluen_data_dir()?;
        let data_file = data_dir.join(DATA_FILE_NAME);
        Ok(Self {
            data_dir,
            data_file,
            cache: RwLock::new(None),
        })
    }

    /// 使用自定义目录创建存储实例（主要用于测试）。
    pub fn with_dir(dir: impl AsRef<Path>) -> Self {
        let data_dir = dir.as_ref().to_path_buf();
        let data_file = data_dir.join(DATA_FILE_NAME);
        Self {
            data_dir,
            data_file,
            cache: RwLock::new(None),
        }
    }

    /// 返回数据文件的完整路径。
    pub fn path(&self) -> &Path {
        &self.data_file
    }

    /// 从磁盘加载数据并更新缓存。
    ///
    /// 文件不存在时返回默认数据（[`MascotData::default`]），不视为错误。
    /// 文件存在但解析或校验失败时返回对应错误。
    ///
    /// 调用此方法会**覆盖**内存缓存。
    pub fn load_from_disk(&self) -> Result<MascotData, MascotError> {
        let data = match std::fs::read_to_string(&self.data_file) {
            Ok(content) => {
                let data: MascotData = serde_json::from_str(&content)?;
                data.validate()?;
                data
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => MascotData::default(),
            Err(e) => return Err(e.into()),
        };

        // 更新缓存
        *self.cache.write().unwrap() = Some(data.clone());
        Ok(data)
    }

    /// 获取数据（优先从缓存读取，缓存未命中时从磁盘加载）。
    ///
    /// 这是前端读取数据的推荐入口，避免每次都命中磁盘。
    pub fn get(&self) -> Result<MascotData, MascotError> {
        // 先尝试读缓存
        {
            let cache = self.cache.read().unwrap();
            if let Some(ref data) = *cache {
                return Ok(data.clone());
            }
        }
        // 缓存未命中，从磁盘加载
        self.load_from_disk()
    }

    /// 保存数据（原子写入）并更新缓存。
    ///
    /// 先校验数据完整性，然后写入同目录下的临时文件 `mascot_data.json.tmp`，
    /// 最后 `rename` 覆盖目标文件。父目录按需创建。
    pub fn save(&self, data: &MascotData) -> Result<(), MascotError> {
        data.validate()?;

        std::fs::create_dir_all(&self.data_dir)?;

        let tmp = self.data_file.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(data)?;
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &self.data_file)?;

        // 更新缓存
        *self.cache.write().unwrap() = Some(data.clone());

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
    use crate::mascot::model::{MascotConfig, MascotData, Mood};

    fn sample_config() -> MascotConfig {
        MascotConfig {
            version: "1.0.0".into(),
            name: "小Fluen".into(),
            enabled: false,
            provider_id: Some("openai".into()),
            model_id: Some("gpt-4".into()),
            mcp_enabled: true,
            skills_enabled: true,
            function_calling_enabled: true,
            personality: "calm".into(),
            show_thinking_content: false,
            professional_expression: false,
            enabled_agents: Vec::new(),
        }
    }

    fn sample_data() -> MascotData {
        MascotData {
            version: "1.0.0".into(),
            mood: Mood::Happy,
            affinity: 42,
            mood_updated_at: Some("2026-07-11T00:00:00Z".into()),
            affinity_updated_at: Some("2026-07-11T00:00:00Z".into()),
        }
    }

    // --- 配置存储测试 ---

    #[test]
    fn load_missing_config_file_returns_default() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_mascot_config_test_{}_default",
            std::process::id()
        ));
        let storage = MascotConfigStorage::with_dir(&tmp);
        let config = storage.load_from_disk().unwrap();
        assert_eq!(config.version, "1.0.0");
        assert_eq!(config.name, "Motis");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_then_load_config_roundtrip() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_mascot_config_test_{}_roundtrip",
            std::process::id()
        ));
        let storage = MascotConfigStorage::with_dir(&tmp);
        let original = sample_config();

        storage.save(&original).unwrap();
        assert!(storage.path().exists());

        let loaded = storage.load_from_disk().unwrap();
        assert_eq!(loaded.version, original.version);
        assert_eq!(loaded.name, original.name);
        assert_eq!(loaded.enabled, original.enabled);
        assert_eq!(loaded.provider_id, original.provider_id);
        assert_eq!(loaded.model_id, original.model_id);
        assert_eq!(loaded.mcp_enabled, original.mcp_enabled);
        assert_eq!(loaded.skills_enabled, original.skills_enabled);
        assert_eq!(loaded.function_calling_enabled, original.function_calling_enabled);
        assert_eq!(loaded.personality, original.personality);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn get_uses_cache_after_load_config() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_mascot_config_test_{}_cache",
            std::process::id()
        ));
        let storage = MascotConfigStorage::with_dir(&tmp);
        let original = sample_config();
        storage.save(&original).unwrap();

        // 第一次 get 从缓存（save 时已缓存）
        let config1 = storage.get().unwrap();
        assert_eq!(config1.name, "小Fluen");

        // 清除缓存后重新加载
        storage.invalidate_cache();
        let config2 = storage.get().unwrap();
        assert_eq!(config2.name, "小Fluen");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_invalid_config_fails() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_mascot_config_test_{}_invalid",
            std::process::id()
        ));
        let storage = MascotConfigStorage::with_dir(&tmp);

        let mut config = sample_config();
        config.version = "  ".into();

        let result = storage.save(&config);
        assert!(result.is_err());
        assert!(!storage.path().exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_creates_parent_dir_config() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_mascot_config_test_{}_mkdir/nested/deep",
            std::process::id()
        ));
        let storage = MascotConfigStorage::with_dir(&tmp);
        let config = sample_config();

        storage.save(&config).unwrap();
        assert!(storage.path().exists());

        let _ = std::fs::remove_dir_all(
            std::env::temp_dir()
                .join(format!("fluen_mascot_config_test_{}_mkdir", std::process::id())),
        );
    }

    #[test]
    fn invalidate_cache_forces_disk_read_config() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_mascot_config_test_{}_invalidate",
            std::process::id()
        ));
        let storage = MascotConfigStorage::with_dir(&tmp);
        let config = sample_config();
        storage.save(&config).unwrap();

        // 清除缓存
        storage.invalidate_cache();

        // 重新读取仍应返回正确值
        let loaded = storage.get().unwrap();
        assert_eq!(loaded.name, "小Fluen");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    // --- 数据存储测试 ---

    #[test]
    fn load_missing_data_file_returns_default() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_mascot_data_test_{}_default",
            std::process::id()
        ));
        let storage = MascotDataStorage::with_dir(&tmp);
        let data = storage.load_from_disk().unwrap();
        assert_eq!(data.version, "1.0.0");
        assert_eq!(data.mood, Mood::Neutral);
        assert_eq!(data.affinity, 0);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_then_load_data_roundtrip() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_mascot_data_test_{}_roundtrip",
            std::process::id()
        ));
        let storage = MascotDataStorage::with_dir(&tmp);
        let original = sample_data();

        storage.save(&original).unwrap();
        assert!(storage.path().exists());

        let loaded = storage.load_from_disk().unwrap();
        assert_eq!(loaded.version, original.version);
        assert_eq!(loaded.mood, original.mood);
        assert_eq!(loaded.affinity, original.affinity);
        assert_eq!(loaded.mood_updated_at, original.mood_updated_at);
        assert_eq!(loaded.affinity_updated_at, original.affinity_updated_at);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn get_uses_cache_after_load_data() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_mascot_data_test_{}_cache",
            std::process::id()
        ));
        let storage = MascotDataStorage::with_dir(&tmp);
        let original = sample_data();
        storage.save(&original).unwrap();

        // 第一次 get 从缓存（save 时已缓存）
        let data1 = storage.get().unwrap();
        assert_eq!(data1.mood, Mood::Happy);

        // 清除缓存后重新加载
        storage.invalidate_cache();
        let data2 = storage.get().unwrap();
        assert_eq!(data2.mood, Mood::Happy);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_invalid_data_fails() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_mascot_data_test_{}_invalid",
            std::process::id()
        ));
        let storage = MascotDataStorage::with_dir(&tmp);

        let mut data = sample_data();
        data.affinity = 101;

        let result = storage.save(&data);
        assert!(result.is_err());
        assert!(!storage.path().exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_creates_parent_dir_data() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_mascot_data_test_{}_mkdir/nested/deep",
            std::process::id()
        ));
        let storage = MascotDataStorage::with_dir(&tmp);
        let data = sample_data();

        storage.save(&data).unwrap();
        assert!(storage.path().exists());

        let _ = std::fs::remove_dir_all(
            std::env::temp_dir()
                .join(format!("fluen_mascot_data_test_{}_mkdir", std::process::id())),
        );
    }

    #[test]
    fn invalidate_cache_forces_disk_read_data() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_mascot_data_test_{}_invalidate",
            std::process::id()
        ));
        let storage = MascotDataStorage::with_dir(&tmp);
        let data = sample_data();
        storage.save(&data).unwrap();

        // 清除缓存
        storage.invalidate_cache();

        // 重新读取仍应返回正确值
        let loaded = storage.get().unwrap();
        assert_eq!(loaded.mood, Mood::Happy);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
