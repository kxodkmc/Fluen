//! 项目级持久化成果板——[`ArtifactStore`] 的 Fluen 自研实现。
//!
//! ## 为什么不用内置 [`InMemoryArtifactStore`](referee_agent::artifact::InMemoryArtifactStore)
//!
//! 内置实现按**会话 UUID** 分板且纯内存：Fluen 的会话模型是「每轮新建
//! `SessionId`、跨轮回放文本历史」，委派当轮结束即弃会话——下一轮
//! `list_by_creator(新会话)` 永远为空，重启更会全部丢失（跨轮失效的
//! 完整分析见 `docs/referee-issue-artifact-board-scoping.md`）。
//!
//! ## 本实现的作用域模型
//!
//! Federation 按项目指纹构建、store 实例与论文项目一一对应，故本实现
//! 把**整个 store 实例锚定为项目板**：`ensure_board` / `list_by_creator`
//! 忽略调用者会话键（内置 `ListMyBoard` 传什么都能列全量），板内条目
//! 跨会话轮次、跨应用重启可读。
//!
//! ## 持久化模型
//!
//! - 目录：`fluen_data_dir()/artifacts/<project_key>/`，按项目隔离；
//! - `board.json` 持久化板 ID；每个条目一个 `<artifact_id>.json`（
//!   `Artifact` 自带 serde 派生，原样落盘）；
//! - 内存镜像 + 写穿（write-through）：读取只走内存；磁盘写入失败仅
//!   告警降级（内存仍为事实源，不阻断委派主流程）；
//! - 容量：沿用有界硬约束，超限按 `updated_at` 最旧优先淘汰（区别于
//!   内置实现的拒绝写入），单条超总容量才报 `CapacityExceeded`。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use async_trait::async_trait;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use referee_agent::artifact::{Artifact, ArtifactStore, StoreConfig, StoreError};

/// 板元数据文件名（目录内与条目文件并存，加载时跳过）。
const BOARD_FILE: &str = "board.json";
/// 提示词注入的清单条目上限（防提示词膨胀）。
const SNAPSHOT_LIMIT: usize = 20;
/// 提示词注入的条目标题截断长度。
const SNAPSHOT_TITLE_CHARS: usize = 60;

/// 成果板条目摘要（提示词注入用，不含正文）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardEntry {
    /// 访问凭证（`read_artifact` 的入参）。
    pub artifact_id: String,
    /// 产出者标签（如 `delegate_agent:essay_writing`）。
    pub producer_label: String,
    /// 条目标题（委派任务原文，已截断）。
    pub title: String,
    /// 最近更新时间（Unix 秒）。
    pub updated_at: u64,
}

/// 板元数据落盘结构（板 ID 以字符串存储，避免依赖 uuid serde feature）。
#[derive(Serialize, Deserialize)]
struct BoardFile {
    board: String,
}

/// 存储内部状态（内存镜像，读路径零磁盘 IO）。
struct StoreInner {
    board: uuid::Uuid,
    artifacts: HashMap<String, Artifact>,
    total_bytes: usize,
}

/// 项目级持久化成果板。
pub struct ProjectArtifactStore {
    /// 落盘目录（按项目隔离）。
    dir: PathBuf,
    config: StoreConfig,
    inner: Arc<Mutex<StoreInner>>,
}

impl ProjectArtifactStore {
    /// 打开（或初始化）当前项目的成果板，从用户数据目录加载既有条目。
    pub fn open(project_path: &str, config: StoreConfig) -> Result<Self, String> {
        let base = crate::platform::fluen_data_dir()
            .map_err(|e| format!("解析用户数据目录失败: {e}"))?;
        Self::open_with_base(base, project_path, config)
    }

    /// 在指定根目录下打开成果板（依赖注入：测试与隔离场景用）。
    pub fn open_with_base(base: PathBuf, project_path: &str, config: StoreConfig) -> Result<Self, String> {
        let dir = base.join("artifacts").join(project_key(project_path));
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建成果板目录失败: {e}"))?;

        let board = Self::load_or_create_board(&dir)?;
        let (artifacts, total_bytes) = Self::load_artifacts(&dir);

        Ok(Self {
            dir,
            config,
            inner: Arc::new(Mutex::new(StoreInner {
                board,
                artifacts,
                total_bytes,
            })),
        })
    }

    /// 加载板 ID（缺失则生成并落盘）。
    fn load_or_create_board(dir: &PathBuf) -> Result<uuid::Uuid, String> {
        let path = dir.join(BOARD_FILE);
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(file) = serde_json::from_slice::<BoardFile>(&bytes) {
                if let Ok(board) = uuid::Uuid::parse_str(&file.board) {
                    return Ok(board);
                }
            }
            tracing::warn!("成果板元数据损坏，重新生成板 ID");
        }
        let board = uuid::Uuid::new_v4();
        let bytes = serde_json::to_vec(&BoardFile {
            board: board.to_string(),
        })
        .map_err(|e| format!("序列化板元数据失败: {e}"))?;
        std::fs::write(&path, bytes).map_err(|e| format!("写入板元数据失败: {e}"))?;
        Ok(board)
    }

    /// 扫描目录加载全部条目（跳过损坏文件并清理）。
    fn load_artifacts(dir: &PathBuf) -> (HashMap<String, Artifact>, usize) {
        let mut artifacts = HashMap::new();
        let mut total_bytes = 0usize;
        let Ok(entries) = std::fs::read_dir(dir) else {
            return (artifacts, total_bytes);
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if path.file_name().and_then(|n| n.to_str()) == Some(BOARD_FILE) {
                continue;
            }
            let loaded = std::fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<Artifact>(&b).ok());
            match loaded {
                Some(a) => {
                    total_bytes += a.bytes.len();
                    artifacts.insert(a.id.clone(), a);
                }
                None => {
                    tracing::warn!(path = %path.display(), "成果文件损坏，已移除");
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
        (artifacts, total_bytes)
    }

    /// 当前成果板清单（按产出顺序，取最近 [`SNAPSHOT_LIMIT`] 条）。
    ///
    /// 同步快照：内存镜像即全量，无需异步读取。供系统提示词注入，
    /// 让 Motis 跨轮持有真实 artifact_id。
    pub fn snapshot(&self) -> Vec<BoardEntry> {
        let inner = self.inner.lock();
        let mut items: Vec<&Artifact> = inner.artifacts.values().collect();
        items.sort_by_key(|a| a.seq);
        items
            .iter()
            .rev()
            .take(SNAPSHOT_LIMIT)
            .rev()
            .map(|a| BoardEntry {
                artifact_id: a.id.clone(),
                producer_label: a.producer_label.clone(),
                title: a.title.chars().take(SNAPSHOT_TITLE_CHARS).collect(),
                updated_at: unix_secs(a.updated_at),
            })
            .collect()
    }

    /// 条目落盘（写临时文件后原子重命名；失败仅告警，内存不回滚）。
    fn persist(&self, artifact: &Artifact) {
        let path = self.dir.join(format!("{}.json", artifact.id));
        let tmp = self.dir.join(format!("{}.json.tmp", artifact.id));
        let write = || -> std::io::Result<()> {
            let bytes = serde_json::to_vec(artifact)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            std::fs::write(&tmp, bytes)?;
            std::fs::rename(&tmp, &path)
        };
        if let Err(e) = write() {
            tracing::warn!(artifact_id = %artifact.id, error = %e, "成果落盘失败（本轮仅内存可见）");
        }
    }

    /// 删除条目文件（淘汰时调用；文件缺失视为已清理）。
    fn remove_file(&self, id: &str) {
        let _ = std::fs::remove_file(self.dir.join(format!("{id}.json")));
    }
}

#[async_trait]
impl ArtifactStore for ProjectArtifactStore {
    async fn store(&self, artifact: Artifact) -> Result<String, StoreError> {
        let size = artifact.bytes.len();
        if size > self.config.max_total_bytes {
            return Err(StoreError::CapacityExceeded);
        }

        let mut inner = self.inner.lock();
        let old = inner.artifacts.get(&artifact.id).cloned();
        let old_size = old.as_ref().map(|a| a.bytes.len()).unwrap_or(0);

        // 超限按 updated_at 最旧优先淘汰（本次写入对象受保护），
        // 直到新条目可放入：数量与总字节均回到上限内
        while (old.is_none() && inner.artifacts.len() >= self.config.max_artifacts)
            || inner
                .total_bytes
                .saturating_sub(old_size)
                .saturating_add(size)
                > self.config.max_total_bytes
        {
            let Some(victim) = inner
                .artifacts
                .values()
                .filter(|a| a.id != artifact.id)
                .min_by_key(|a| a.updated_at)
                .map(|a| a.id.clone())
            else {
                break;
            };
            if let Some(removed) = inner.artifacts.remove(&victim) {
                inner.total_bytes = inner.total_bytes.saturating_sub(removed.bytes.len());
            }
            self.remove_file(&victim);
        }

        // 序号 / 创建时间归一（与内置实现语义一致）
        let mut artifact = artifact;
        match old {
            Some(o) => {
                artifact.seq = o.seq;
                artifact.created_at = o.created_at;
            }
            None => {
                artifact.seq = inner
                    .artifacts
                    .values()
                    .map(|a| a.seq)
                    .max()
                    .unwrap_or(0)
                    + 1;
            }
        }
        artifact.updated_at = SystemTime::now();

        inner.total_bytes = inner.total_bytes.saturating_sub(old_size) + size;
        let id = artifact.id.clone();
        self.persist(&artifact);
        inner.artifacts.insert(id.clone(), artifact);
        Ok(id)
    }

    async fn get(&self, id: &str) -> Result<Option<Artifact>, StoreError> {
        Ok(self.inner.lock().artifacts.get(id).cloned())
    }

    /// 项目作用域：板内全量条目按产出顺序返回（调用者键仅兼容签名）。
    async fn list_by_creator(&self, _creator: uuid::Uuid) -> Result<Vec<Artifact>, StoreError> {
        let inner = self.inner.lock();
        let mut items: Vec<Artifact> = inner.artifacts.values().cloned().collect();
        items.sort_by_key(|a| a.seq);
        Ok(items)
    }

    /// 项目作用域：恒定返回唯一的项目板（幂等）。
    async fn ensure_board(&self, _creator: uuid::Uuid) -> Result<uuid::Uuid, StoreError> {
        Ok(self.inner.lock().board)
    }
}

/// SystemTime → Unix 秒（展示用时间戳，不可排序场景回退 0）。
fn unix_secs(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 项目路径 → 磁盘目录名：可读前缀（非字母数字替换为下划线，截断）
/// + 路径 FNV-1a 哈希后缀（确定性，跨重启稳定）。
fn project_key(project_path: &str) -> String {
    let prefix: String = project_path
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .take(24)
        .collect();
    format!("{prefix}-{:016x}", fnv1a(project_path.as_bytes()))
}

/// FNV-1a 64 位哈希（无依赖、跨进程稳定；仅用于目录名防碰撞，非安全用途）。
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

// ===========================================================================
// 测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn small_config(max_artifacts: usize) -> StoreConfig {
        StoreConfig {
            max_artifacts,
            max_total_bytes: 1024 * 1024,
        }
    }

    fn temp_base() -> PathBuf {
        std::env::temp_dir().join(format!("fluen_board_{}", uuid::Uuid::new_v4()))
    }

    fn make_artifact(board: uuid::Uuid, title: &str, bytes: usize) -> Artifact {
        Artifact::new(
            board,
            uuid::Uuid::new_v4(),
            "delegate_agent:essay_writing",
            title,
            "text/plain",
            vec![0u8; bytes],
        )
    }

    async fn open_and_store(base: PathBuf, project: &str, title: &str) -> (ProjectArtifactStore, String) {
        let store = ProjectArtifactStore::open_with_base(base, project, small_config(16)).unwrap();
        let board = store.ensure_board(uuid::Uuid::new_v4()).await.unwrap();
        let id = store
            .store(make_artifact(board, title, 8))
            .await
            .unwrap();
        (store, id)
    }

    #[tokio::test]
    async fn persists_across_reopen_with_stable_board() {
        let base = temp_base();
        let (store, id) = open_and_store(base.clone(), r"C:\proj\t4", "first").await;
        let board_before = store.ensure_board(uuid::Uuid::new_v4()).await.unwrap();

        let reopened = ProjectArtifactStore::open_with_base(base, r"C:\proj\t4", small_config(16))
            .unwrap();
        let got = reopened.get(&id).await.unwrap().expect("must persist");
        assert_eq!(got.title, "first");
        assert_eq!(
            reopened.ensure_board(uuid::Uuid::new_v4()).await.unwrap(),
            board_before,
            "board id must be stable across reopen"
        );
        assert_eq!(reopened.list_by_creator(uuid::Uuid::new_v4()).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn different_projects_are_isolated() {
        let base = temp_base();
        let (_, id_a) = open_and_store(base.clone(), r"C:\proj\a", "in-a").await;
        let store_b = ProjectArtifactStore::open_with_base(base, r"C:\proj\b", small_config(16)).unwrap();
        assert!(store_b.get(&id_a).await.unwrap().is_none());
        assert!(store_b.list_by_creator(uuid::Uuid::new_v4()).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn list_is_creator_agnostic_and_sorted_by_seq() {
        let base = temp_base();
        let store = ProjectArtifactStore::open_with_base(base, "p", small_config(16)).unwrap();
        let board = store.ensure_board(uuid::Uuid::new_v4()).await.unwrap();
        for title in ["one", "two", "three"] {
            store.store(make_artifact(board, title, 4)).await.unwrap();
        }
        // 任意调用者键都列出全量（项目作用域语义）
        let items = store
            .list_by_creator(uuid::Uuid::new_v4())
            .await
            .unwrap();
        let titles: Vec<&str> = items.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(titles, vec!["one", "two", "three"]);
    }

    #[tokio::test]
    async fn update_preserves_seq_and_created_at() {
        let base = temp_base();
        let store =
            ProjectArtifactStore::open_with_base(base.clone(), "p", small_config(16)).unwrap();
        let board = store.ensure_board(uuid::Uuid::new_v4()).await.unwrap();
        let artifact = make_artifact(board, "t", 4);
        let created = artifact.created_at;
        let id = store.store(artifact.clone()).await.unwrap();

        let mut updated = artifact;
        updated.id = id.clone();
        updated.bytes = vec![1u8; 8];
        store.store(updated).await.unwrap();

        let got = store.get(&id).await.unwrap().unwrap();
        assert_eq!(got.seq, 1);
        assert_eq!(got.created_at, created);
        assert_eq!(got.bytes.len(), 8);

        // 写穿落盘：重开同一项目读到的是更新后内容
        let reopened = ProjectArtifactStore::open_with_base(base, "p", small_config(16)).unwrap();
        let got = reopened.get(&id).await.unwrap().unwrap();
        assert_eq!(got.bytes.len(), 8);
        assert_eq!(got.seq, 1);
    }

    #[tokio::test]
    async fn capacity_evicts_oldest_and_syncs_disk() {
        let base = temp_base();
        let store = ProjectArtifactStore::open_with_base(base.clone(), "p", small_config(2)).unwrap();
        let board = store.ensure_board(uuid::Uuid::new_v4()).await.unwrap();
        let id_a = store.store(make_artifact(board, "a", 4)).await.unwrap();
        store.store(make_artifact(board, "b", 4)).await.unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        store.store(make_artifact(board, "c", 4)).await.unwrap();

        assert!(store.get(&id_a).await.unwrap().is_none(), "oldest must be evicted");
        assert_eq!(store.list_by_creator(uuid::Uuid::new_v4()).await.unwrap().len(), 2);

        let reopened = ProjectArtifactStore::open_with_base(base, "p", small_config(2)).unwrap();
        assert_eq!(reopened.list_by_creator(uuid::Uuid::new_v4()).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn snapshot_is_bounded_and_title_truncated() {
        let base = temp_base();
        let store = ProjectArtifactStore::open_with_base(base, "p", small_config(64)).unwrap();
        let board = store.ensure_board(uuid::Uuid::new_v4()).await.unwrap();
        let long_title = "很".repeat(200);
        for _ in 0..(SNAPSHOT_LIMIT + 5) {
            store.store(make_artifact(board, &long_title, 4)).await.unwrap();
        }
        let entries = store.snapshot();
        assert_eq!(entries.len(), SNAPSHOT_LIMIT);
        assert!(entries[0].title.chars().count() <= SNAPSHOT_TITLE_CHARS);
        assert!(!entries[0].artifact_id.is_empty());
    }

    #[tokio::test]
    async fn oversize_artifact_rejected() {
        let base = temp_base();
        let store = ProjectArtifactStore::open_with_base(base, "p", StoreConfig {
            max_artifacts: 16,
            max_total_bytes: 64,
        }).unwrap();
        let board = store.ensure_board(uuid::Uuid::new_v4()).await.unwrap();
        let err = store.store(make_artifact(board, "big", 128)).await.unwrap_err();
        assert!(matches!(err, StoreError::CapacityExceeded));
    }

    #[test]
    fn project_key_is_stable_and_sanitized() {
        let a = project_key(r"C:\Users\wppcp\Documents\Fluen\t4");
        let b = project_key(r"C:\Users\wppcp\Documents\Fluen\t4");
        let c = project_key(r"C:\Users\wppcp\Documents\Fluen\t5");
        assert_eq!(a, b, "same path must map to same key");
        assert_ne!(a, c);
        assert!(!a.contains(':') && !a.contains('\\'));
    }
}
