//! 最近打开项目的存储层。
//!
//! 负责跨平台路径解析与 `recent_projects.json` 文件的读写。
//! 写入采用原子操作（先写 `.tmp` 再 `rename`），避免读到半写状态。
//!
//! ## 性能优化
//!
//! 与 [`crate::app_config::storage::AppConfigStorage`] 一致，内置
//! **内存缓存**（`RwLock<Option<...>>`），首次读取后缓存于内存，
//! 后续读取直接返回缓存副本，避免重复磁盘 I/O。写入时同步更新缓存。

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use super::error::RecentProjectsError;
use super::model::RecentProjectsData;
use crate::platform::{fluen_data_dir, PlatformError};

/// 数据文件名。
const DATA_FILE_NAME: &str = "recent_projects.json";

/// 默认保留的最大记录数量。
pub const DEFAULT_MAX_COUNT: usize = 4;

/// 最近打开项目存储（带内存缓存）。
///
/// 封装 `recent_projects.json` 的路径解析与读写操作。通过
/// [`RecentProjectsStorage::new`] 自动解析当前平台数据目录，或通过
/// [`RecentProjectsStorage::with_dir`] 指定自定义目录（用于测试）。
///
/// 内部使用 [`RwLock`] 缓存数据，读操作无锁竞争，写操作排他。
pub struct RecentProjectsStorage {
    /// 数据目录（由 [`crate::platform::fluen_data_dir`] 解析）。
    data_dir: PathBuf,
    /// 数据文件完整路径。
    data_file: PathBuf,
    /// 内存缓存（懒加载）。
    cache: RwLock<Option<RecentProjectsData>>,
}

impl RecentProjectsStorage {
    /// 创建存储实例，自动解析当前平台的数据目录。
    ///
    /// # 平台路径
    ///
    /// | 平台 | 路径 |
    /// |------|------|
    /// | macOS | `~/Library/Application Support/com.wppcp.fluen/recent_projects.json` |
    /// | Windows | `%LOCALAPPDATA%\Fluen\recent_projects.json` |
    /// | Linux | `~/.local/share/Fluen/recent_projects.json`（或 `$XDG_DATA_HOME/Fluen/`） |
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
    /// 文件不存在时返回默认数据（[`RecentProjectsData::default`]），不视为错误。
    /// 文件存在但解析或校验失败时返回对应错误。
    ///
    /// 调用此方法会**覆盖**内存缓存。
    pub fn load_from_disk(&self) -> Result<RecentProjectsData, RecentProjectsError> {
        let data = match std::fs::read_to_string(&self.data_file) {
            Ok(content) => {
                let data: RecentProjectsData = serde_json::from_str(&content)?;
                data.validate()?;
                data
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => RecentProjectsData::default(),
            Err(e) => return Err(e.into()),
        };

        // 更新缓存
        *self.cache.write().unwrap() = Some(data.clone());
        Ok(data)
    }

    /// 获取数据（优先从缓存读取，缓存未命中时从磁盘加载）。
    ///
    /// 这是前端读取数据的推荐入口，避免每次都命中磁盘。
    pub fn get(&self) -> Result<RecentProjectsData, RecentProjectsError> {
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
    /// 先校验数据完整性，然后写入同目录下的临时文件 `recent_projects.json.tmp`，
    /// 最后 `rename` 覆盖目标文件。父目录按需创建。
    pub fn save(&self, data: &RecentProjectsData) -> Result<(), RecentProjectsError> {
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
    use crate::recent_projects::model::{RecentProjectEntry, RecentProjectsData};

    fn sample_data() -> RecentProjectsData {
        let mut data = RecentProjectsData::default();
        data.upsert(
            RecentProjectEntry {
                project_path: "/tmp/proj1".into(),
                title: "项目1".into(),
                author: "张三".into(),
                opened_at: "2026-01-01T00:00:00Z".into(),
            },
            8,
        );
        data.upsert(
            RecentProjectEntry {
                project_path: "/tmp/proj2".into(),
                title: "项目2".into(),
                author: "李四".into(),
                opened_at: "2026-01-02T00:00:00Z".into(),
            },
            8,
        );
        data
    }

    #[test]
    fn load_missing_file_returns_default() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_recent_test_{}_default",
            std::process::id()
        ));
        let storage = RecentProjectsStorage::with_dir(&tmp);
        let data = storage.load_from_disk().unwrap();
        assert_eq!(data.version, "1.0.0");
        assert!(data.entries.is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_then_load_roundtrip() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_recent_test_{}_roundtrip",
            std::process::id()
        ));
        let storage = RecentProjectsStorage::with_dir(&tmp);
        let original = sample_data();

        storage.save(&original).unwrap();
        assert!(storage.path().exists());

        let loaded = storage.load_from_disk().unwrap();
        assert_eq!(loaded.version, original.version);
        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.entries[0].project_path, "/tmp/proj2");
        assert_eq!(loaded.entries[1].project_path, "/tmp/proj1");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn get_uses_cache_after_load() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_recent_test_{}_cache",
            std::process::id()
        ));
        let storage = RecentProjectsStorage::with_dir(&tmp);
        let original = sample_data();
        storage.save(&original).unwrap();

        // 第一次 get 从缓存（save 时已缓存）
        let data1 = storage.get().unwrap();
        assert_eq!(data1.entries.len(), 2);

        // 清除缓存后重新加载
        storage.invalidate_cache();
        let data2 = storage.get().unwrap();
        assert_eq!(data2.entries.len(), 2);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_invalid_data_fails() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_recent_test_{}_invalid",
            std::process::id()
        ));
        let storage = RecentProjectsStorage::with_dir(&tmp);

        let mut data = sample_data();
        data.version = "  ".into();

        let result = storage.save(&data);
        assert!(result.is_err());
        assert!(!storage.path().exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_creates_parent_dir() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_recent_test_{}_mkdir/nested/deep",
            std::process::id()
        ));
        let storage = RecentProjectsStorage::with_dir(&tmp);
        let data = sample_data();

        storage.save(&data).unwrap();
        assert!(storage.path().exists());

        let _ = std::fs::remove_dir_all(
            std::env::temp_dir()
                .join(format!("fluen_recent_test_{}_mkdir", std::process::id())),
        );
    }

    #[test]
    fn invalidate_cache_forces_disk_read() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_recent_test_{}_invalidate",
            std::process::id()
        ));
        let storage = RecentProjectsStorage::with_dir(&tmp);
        let data = sample_data();
        storage.save(&data).unwrap();

        // 清除缓存
        storage.invalidate_cache();

        // 重新读取仍应返回正确值
        let loaded = storage.get().unwrap();
        assert_eq!(loaded.entries.len(), 2);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
