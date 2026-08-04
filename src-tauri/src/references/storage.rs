//! 文献索引文件 I/O——带锁原子读写 + 内存缓存。
//!
//! ## 并发安全
//!
//! 所有读写操作经 [`ReferenceIndex::update`] 串行化，内部持 `Mutex` 锁
//! 完成 read → modify → write 事务，避免并发写丢更新。
//!
//! 写入采用原子操作：先写 `.tmp` 再 `rename`，避免读到半写状态。
//!
//! ## 缓存
//!
//! 首次读取后缓存于内存，后续读取直接返回缓存副本。
//! 写入时同步更新缓存。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::error::ReferenceError;
use super::model::ReferenceEntry;

/// 索引文件名。
const INDEX_FILE_NAME: &str = "references-index.json";

/// 文献索引存储（带锁 + 内存缓存）。
///
/// 每个 Tauri 窗口共享一个实例（通过 `State` 注入）。
/// 通过 [`ReferenceIndex::new`] 从项目路径创建。
pub struct ReferenceIndex {
    /// 索引文件完整路径。
    index_file: PathBuf,
    /// 文件级互斥锁——所有读写都经此锁串行化。
    lock: Mutex<()>,
    /// 内存缓存（懒加载）。
    cache: std::sync::RwLock<Option<Vec<ReferenceEntry>>>,
}

impl ReferenceIndex {
    /// 从项目根路径创建索引存储。
    pub fn new(project_dir: impl AsRef<Path>) -> Self {
        let index_file = project_dir.as_ref().join("references").join(INDEX_FILE_NAME);
        Self {
            index_file,
            lock: Mutex::new(()),
            cache: std::sync::RwLock::new(None),
        }
    }

    /// 返回索引文件路径。
    pub fn path(&self) -> &Path {
        &self.index_file
    }

    /// 从磁盘加载索引并更新缓存（持锁）。
    ///
    /// 文件不存在时返回空数组，不视为错误。
    pub fn load_from_disk(&self) -> Result<Vec<ReferenceEntry>, ReferenceError> {
        let _guard = self.lock.lock().unwrap();
        let entries = match std::fs::read_to_string(&self.index_file) {
            Ok(content) => {
                let entries: Vec<ReferenceEntry> = if content.trim().is_empty() {
                    Vec::new()
                } else {
                    serde_json::from_str(&content)?
                };
                entries
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };

        *self.cache.write().unwrap() = Some(entries.clone());
        Ok(entries)
    }

    /// 获取索引（优先缓存，未命中时从磁盘加载）。
    pub fn list(&self) -> Result<Vec<ReferenceEntry>, ReferenceError> {
        {
            let cache = self.cache.read().unwrap();
            if let Some(ref entries) = *cache {
                return Ok(entries.clone());
            }
        }
        self.load_from_disk()
    }

    /// 按文件哈希查找文献（去重检查）。
    pub fn find_by_hash(&self, file_hash: &str) -> Result<Option<ReferenceEntry>, ReferenceError> {
        let entries = self.list()?;
        Ok(entries.into_iter().find(|e| e.file_hash == file_hash))
    }

    /// 按 ID 查找文献。
    pub fn find(&self, id: &str) -> Result<Option<ReferenceEntry>, ReferenceError> {
        let entries = self.list()?;
        Ok(entries.into_iter().find(|e| e.id == id))
    }

    /// 原子写入索引文件并更新缓存（持锁）。
    fn save_locked(&self, entries: &[ReferenceEntry]) -> Result<(), ReferenceError> {
        let tmp = self.index_file.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(entries)?;
        // 确保父目录存在（支持首次写入或目录被外部清理后的恢复）
        if let Some(parent) = self.index_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &self.index_file)?;
        *self.cache.write().unwrap() = Some(entries.to_vec());
        Ok(())
    }

    /// 读改写事务——持锁期间完成 read → modify → write。
    ///
    /// 闭包接收当前索引的可变引用，修改后返回。事务结束后原子写入。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// index.update(|entries| {
    ///     entries.push(new_entry);
    ///     Ok(())
    /// })?;
    /// ```
    pub fn update<F>(&self, f: F) -> Result<Vec<ReferenceEntry>, ReferenceError>
    where
        F: FnOnce(&mut Vec<ReferenceEntry>) -> Result<(), ReferenceError>,
    {
        let _guard = self.lock.lock().unwrap();

        // 读取当前索引（优先缓存，否则从磁盘）
        let mut entries = {
            let cache = self.cache.read().unwrap();
            match &*cache {
                Some(e) => e.clone(),
                None => match std::fs::read_to_string(&self.index_file) {
                    Ok(content) => {
                        if content.trim().is_empty() {
                            Vec::new()
                        } else {
                            serde_json::from_str(&content)?
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
                    Err(e) => return Err(e.into()),
                },
            }
        };

        f(&mut entries)?;

        self.save_locked(&entries)?;
        Ok(entries)
    }

    /// 添加或替换条目（按 ID 匹配）。
    pub fn upsert(&self, entry: ReferenceEntry) -> Result<(), ReferenceError> {
        self.update(|entries| {
            if let Some(existing) = entries.iter_mut().find(|e| e.id == entry.id) {
                *existing = entry;
            } else {
                entries.push(entry);
            }
            Ok(())
        })?;
        Ok(())
    }

    /// 按 ID 删除条目，返回被删除的条目。
    pub fn remove(&self, id: &str) -> Result<Option<ReferenceEntry>, ReferenceError> {
        let mut removed = None;
        let _ = self.update(|entries| {
            if let Some(pos) = entries.iter().position(|e| e.id == id) {
                removed = Some(entries.remove(pos));
            }
            Ok(())
        })?;
        Ok(removed)
    }
}

// ---------------------------------------------------------------------------
// 文件操作辅助函数
// ---------------------------------------------------------------------------

/// 计算文件 SHA-256 哈希（十六进制字符串）。
pub fn compute_file_hash(path: &Path) -> Result<String, ReferenceError> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

/// 备份文件到目标路径（复制）。
pub fn backup_file(src: &Path, dst: &Path) -> Result<(), ReferenceError> {
    // 源与目标相同（如 retry 场景：文件已在 raw/ 中），无需复制
    if src == dst {
        return Ok(());
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(src, dst)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::references::model::{ReferenceFormat, ReferenceStatus};

    fn sample_entry(id: &str, hash: &str) -> ReferenceEntry {
        ReferenceEntry {
            id: id.into(),
            title: format!("Title {id}"),
            original_filename: format!("{id}.pdf"),
            format: ReferenceFormat::Pdf,
            file_hash: hash.into(),
            file_path: format!("references/raw/{id}.pdf"),
            md_path: format!("references/md/{id}.md"),
            resource_dir: format!("references/md/resource/{id}"),
            added_at: "2026-07-30T12:00:00Z".into(),
            source: None,
            ai_summary: None,
            status: ReferenceStatus::Pending,
            error: None,
        }
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluen_refs_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let dir = temp_dir();
        let index = ReferenceIndex::new(&dir);
        let entries = index.load_from_disk().unwrap();
        assert!(entries.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upsert_then_list() {
        let dir = temp_dir();
        let index = ReferenceIndex::new(&dir);

        index.upsert(sample_entry("ref-a", "hash1")).unwrap();
        index.upsert(sample_entry("ref-b", "hash2")).unwrap();

        let entries = index.list().unwrap();
        assert_eq!(entries.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upsert_replaces_by_id() {
        let dir = temp_dir();
        let index = ReferenceIndex::new(&dir);

        index.upsert(sample_entry("ref-a", "hash1")).unwrap();
        let mut updated = sample_entry("ref-a", "hash1");
        updated.title = "Updated".into();
        index.upsert(updated).unwrap();

        let entries = index.list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, "Updated");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_by_hash() {
        let dir = temp_dir();
        let index = ReferenceIndex::new(&dir);
        index.upsert(sample_entry("ref-a", "hash1")).unwrap();
        index.upsert(sample_entry("ref-b", "hash2")).unwrap();

        let found = index.find_by_hash("hash2").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "ref-b");

        let none = index.find_by_hash("hash3").unwrap();
        assert!(none.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_by_id() {
        let dir = temp_dir();
        let index = ReferenceIndex::new(&dir);
        index.upsert(sample_entry("ref-a", "hash1")).unwrap();

        let removed = index.remove("ref-a").unwrap();
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, "ref-a");

        let entries = index.list().unwrap();
        assert!(entries.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_creates_file() {
        let dir = temp_dir();
        let index = ReferenceIndex::new(&dir);
        index.upsert(sample_entry("ref-a", "hash1")).unwrap();

        assert!(index.path().exists());
        // 不应有残留 .tmp 文件
        let tmp = index.path().with_extension("json.tmp");
        assert!(!tmp.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reload_from_disk() {
        let dir = temp_dir();
        let index = ReferenceIndex::new(&dir);
        index.upsert(sample_entry("ref-a", "hash1")).unwrap();

        // 新实例从磁盘加载
        let index2 = ReferenceIndex::new(&dir);
        let entries = index2.load_from_disk().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "ref-a");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn compute_file_hash_deterministic() {
        let dir = temp_dir();
        let file = dir.join("test.txt");
        std::fs::write(&file, b"hello world").unwrap();

        let h1 = compute_file_hash(&file).unwrap();
        let h2 = compute_file_hash(&file).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA-256 hex = 64 chars

        let _ = std::fs::remove_dir_all(&dir);
    }
}
