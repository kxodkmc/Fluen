//! 最近打开项目的纯数据模型与校验逻辑。
//!
//! 所有结构体仅承载数据，不涉及文件 I/O。
//! 序列化格式为 JSON，设计目标是清晰、易读、易扩展。

use serde::{Deserialize, Serialize};

use super::error::RecentProjectsError;

// ---------------------------------------------------------------------------
// 单条最近项目记录
// ---------------------------------------------------------------------------

/// 一条最近打开的项目记录。
///
/// 仅保留展示所需的最少信息（标题、作者、路径、最后打开时间），
/// 详细信息在用户点击打开时由 `open_project` 重新加载。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentProjectEntry {
    /// 项目根路径（绝对路径，作为唯一标识）。
    pub project_path: String,
    /// 文章标题（来自 `config.yaml`）。
    pub title: String,
    /// 作者姓名（来自 `config.yaml`）。
    pub author: String,
    /// 最后打开时间（ISO 8601 UTC）。
    pub opened_at: String,
}

impl RecentProjectEntry {
    /// 校验条目完整性。
    ///
    /// 仅校验关键字段非空，路径合法性由调用方在打开时再次校验。
    pub fn validate(&self) -> Result<(), RecentProjectsError> {
        if self.project_path.trim().is_empty() {
            return Err(RecentProjectsError::Validation(
                "project_path 不能为空".into(),
            ));
        }
        if self.title.trim().is_empty() {
            return Err(RecentProjectsError::Validation(
                "title 不能为空".into(),
            ));
        }
        if self.opened_at.trim().is_empty() {
            return Err(RecentProjectsError::Validation(
                "opened_at 不能为空".into(),
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 顶层结构
// ---------------------------------------------------------------------------

/// 最近打开项目列表，对应 `recent_projects.json` 文件。
///
/// 仅保留 `max_count` 条记录（按 `opened_at` 降序），
/// 超出时自动裁剪最早的条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentProjectsData {
    /// 数据文件版本号。
    #[serde(default = "default_version")]
    pub version: String,
    /// 最近打开项目列表（按 `opened_at` 降序）。
    #[serde(default)]
    pub entries: Vec<RecentProjectEntry>,
}

impl Default for RecentProjectsData {
    fn default() -> Self {
        Self {
            version: default_version(),
            entries: Vec::new(),
        }
    }
}

impl RecentProjectsData {
    /// 校验数据完整性。
    ///
    /// 当前仅校验版本号非空，后续可按需扩展。
    pub fn validate(&self) -> Result<(), RecentProjectsError> {
        if self.version.trim().is_empty() {
            return Err(RecentProjectsError::Validation(
                "version 不能为空".into(),
            ));
        }
        Ok(())
    }

    /// 插入或更新一条记录。
    ///
    /// 行为：
    /// 1. 若 `project_path` 已存在，移除旧记录
    /// 2. 将新记录插入到列表头部（最新打开）
    /// 3. 按 `max_count` 裁剪列表（保留前 `max_count` 条）
    pub fn upsert(&mut self, entry: RecentProjectEntry, max_count: usize) {
        // 移除同路径的旧记录
        self.entries
            .retain(|e| e.project_path != entry.project_path);

        // 插入到头部
        self.entries.insert(0, entry);

        // 裁剪到最大数量
        if self.entries.len() > max_count {
            self.entries.truncate(max_count);
        }
    }

    /// 移除指定路径的记录（路径失效时调用）。
    pub fn remove(&mut self, project_path: &str) {
        self.entries.retain(|e| e.project_path != project_path);
    }

    /// 按最大数量裁剪列表。
    ///
    /// 当用户在设置中调小显示数量后调用，确保存储与展示一致。
    pub fn trim(&mut self, max_count: usize) {
        if self.entries.len() > max_count {
            self.entries.truncate(max_count);
        }
    }
}

// ---------------------------------------------------------------------------
// serde 辅助函数
// ---------------------------------------------------------------------------

fn default_version() -> String {
    "1.0.0".to_string()
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(path: &str, title: &str, opened_at: &str) -> RecentProjectEntry {
        RecentProjectEntry {
            project_path: path.into(),
            title: title.into(),
            author: "作者".into(),
            opened_at: opened_at.into(),
        }
    }

    #[test]
    fn default_data() {
        let data = RecentProjectsData::default();
        assert_eq!(data.version, "1.0.0");
        assert!(data.entries.is_empty());
    }

    #[test]
    fn validate_ok() {
        assert!(RecentProjectsData::default().validate().is_ok());
    }

    #[test]
    fn validate_empty_version() {
        let mut data = RecentProjectsData::default();
        data.version = "  ".into();
        assert!(data.validate().is_err());
    }

    #[test]
    fn entry_validate_ok() {
        let entry = sample_entry("/tmp/proj", "标题", "2026-01-01T00:00:00Z");
        assert!(entry.validate().is_ok());
    }

    #[test]
    fn entry_validate_empty_path() {
        let mut entry = sample_entry("/tmp/proj", "标题", "2026-01-01T00:00:00Z");
        entry.project_path = "  ".into();
        assert!(entry.validate().is_err());
    }

    #[test]
    fn upsert_inserts_new_entry_at_head() {
        let mut data = RecentProjectsData::default();
        let e1 = sample_entry("/a", "A", "2026-01-01T00:00:00Z");
        let e2 = sample_entry("/b", "B", "2026-01-02T00:00:00Z");

        data.upsert(e1.clone(), 8);
        assert_eq!(data.entries.len(), 1);
        assert_eq!(data.entries[0].project_path, "/a");

        data.upsert(e2.clone(), 8);
        assert_eq!(data.entries.len(), 2);
        // 新条目插入头部
        assert_eq!(data.entries[0].project_path, "/b");
        assert_eq!(data.entries[1].project_path, "/a");
    }

    #[test]
    fn upsert_moves_existing_to_head() {
        let mut data = RecentProjectsData::default();
        data.upsert(sample_entry("/a", "A1", "2026-01-01T00:00:00Z"), 8);
        data.upsert(sample_entry("/b", "B", "2026-01-02T00:00:00Z"), 8);
        data.upsert(sample_entry("/c", "C", "2026-01-03T00:00:00Z"), 8);

        // 重新打开 /a，应移到头部
        data.upsert(sample_entry("/a", "A2", "2026-01-04T00:00:00Z"), 8);

        assert_eq!(data.entries.len(), 3);
        assert_eq!(data.entries[0].project_path, "/a");
        assert_eq!(data.entries[0].title, "A2");
        assert_eq!(data.entries[1].project_path, "/c");
        assert_eq!(data.entries[2].project_path, "/b");
    }

    #[test]
    fn upsert_trims_to_max_count() {
        let mut data = RecentProjectsData::default();
        data.upsert(sample_entry("/a", "A", "2026-01-01T00:00:00Z"), 2);
        data.upsert(sample_entry("/b", "B", "2026-01-02T00:00:00Z"), 2);
        data.upsert(sample_entry("/c", "C", "2026-01-03T00:00:00Z"), 2);

        // 仅保留最新 2 条
        assert_eq!(data.entries.len(), 2);
        assert_eq!(data.entries[0].project_path, "/c");
        assert_eq!(data.entries[1].project_path, "/b");
    }

    #[test]
    fn remove_existing_entry() {
        let mut data = RecentProjectsData::default();
        data.upsert(sample_entry("/a", "A", "2026-01-01T00:00:00Z"), 8);
        data.upsert(sample_entry("/b", "B", "2026-01-02T00:00:00Z"), 8);

        data.remove("/a");
        assert_eq!(data.entries.len(), 1);
        assert_eq!(data.entries[0].project_path, "/b");
    }

    #[test]
    fn remove_nonexistent_entry_is_noop() {
        let mut data = RecentProjectsData::default();
        data.upsert(sample_entry("/a", "A", "2026-01-01T00:00:00Z"), 8);

        data.remove("/nonexistent");
        assert_eq!(data.entries.len(), 1);
    }

    #[test]
    fn trim_shortens_list() {
        let mut data = RecentProjectsData::default();
        for i in 0..6 {
            data.upsert(
                sample_entry(&format!("/p{i}"), &format!("T{i}"), "2026-01-01T00:00:00Z"),
                8,
            );
        }
        assert_eq!(data.entries.len(), 6);

        data.trim(4);
        assert_eq!(data.entries.len(), 4);
        // 保留头部 4 条（最新）
        assert_eq!(data.entries[0].project_path, "/p5");
        assert_eq!(data.entries[3].project_path, "/p2");
    }

    #[test]
    fn json_roundtrip() {
        let mut data = RecentProjectsData::default();
        data.upsert(sample_entry("/a", "标题A", "2026-01-01T00:00:00Z"), 8);
        data.upsert(sample_entry("/b", "标题B", "2026-01-02T00:00:00Z"), 8);

        let json = serde_json::to_string_pretty(&data).unwrap();
        let parsed: RecentProjectsData = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, "1.0.0");
        assert_eq!(parsed.entries.len(), 2);
        assert_eq!(parsed.entries[0].project_path, "/b");
        assert_eq!(parsed.entries[0].title, "标题B");
    }

    #[test]
    fn default_deserialize_missing_fields() {
        // 仅 version 字段
        let json = r#"{"version":"2.0.0"}"#;
        let data: RecentProjectsData = serde_json::from_str(json).unwrap();
        assert_eq!(data.version, "2.0.0");
        assert!(data.entries.is_empty());
    }
}
