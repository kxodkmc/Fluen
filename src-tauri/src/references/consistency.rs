//! 一致性校验——扫描孤儿文件与缺失条目。
//!
//! 在 `check_references_consistency` 命令中调用，或惰性执行于
//! 首次 `list_references` 时。
//!
//! ## 孤儿文件处理
//!
//! `references/raw/` 下有文件但 index 无记录 → 移动到 `.orphan/` 子目录
//! （不直接删除，避免误删用户数据）。
//!
//! ## 缺失条目
//!
//! index 有记录但 raw/md 文件缺失 → 标记 `status=Failed`。

use std::path::Path;

use super::error::ReferenceError;
use super::model::{ConsistencyReport, ReferenceStatus};
use super::storage::ReferenceIndex;

/// 执行一致性校验并修复孤儿文件。
///
/// # 执行流程
///
/// 1. 扫描 `references/raw/` 下所有文件，提取 ID
/// 2. 与 index 中的条目比对，孤儿文件移动到 `.orphan/`
/// 3. 检查 index 中条目的 raw/md 文件是否存在，缺失则标记 Failed
pub fn check_and_repair(project_dir: &Path, index: &ReferenceIndex) -> Result<ConsistencyReport, ReferenceError> {
    let raw_dir = project_dir.join("references").join("raw");
    let orphan_dir = raw_dir.join(".orphan");

    let entries = index.list()?;
    let indexed_ids: std::collections::HashSet<&str> =
        entries.iter().map(|e| e.id.as_str()).collect();

    let mut orphan_files = Vec::new();
    let mut repaired = 0usize;

    // 扫描 raw 目录下的文件
    if raw_dir.is_dir() {
        for entry in std::fs::read_dir(&raw_dir)? {
            let entry = entry?;
            let path = entry.path();

            // 跳过目录（包括 .orphan）
            if path.is_dir() {
                continue;
            }

            let filename = entry.file_name();
            let name = filename.to_string_lossy();
            // 提取 ID（去掉扩展名）
            let id = name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(&name);

            if !indexed_ids.contains(id) {
                // 孤儿文件
                orphan_files.push(name.to_string());
                std::fs::create_dir_all(&orphan_dir)?;
                let dst = orphan_dir.join(&*name);
                std::fs::rename(&path, &dst)?;
                repaired += 1;
            }
        }
    }

    // 检查 index 中条目的文件完整性
    let mut broken_entries = Vec::new();
    let to_fix: Vec<_> = entries
        .iter()
        .filter(|e| {
            let raw_exists = project_dir.join(&e.file_path).exists();
            let md_exists = project_dir.join(&e.md_path).exists();
            !raw_exists || (e.status == ReferenceStatus::Completed && !md_exists)
        })
        .map(|e| e.id.clone())
        .collect();

    for id in &to_fix {
        broken_entries.push(id.clone());
        index.update(|entries| {
            if let Some(entry) = entries.iter_mut().find(|e| &e.id == id) {
                entry.status = ReferenceStatus::Failed;
                entry.error = Some("文件缺失（一致性校验）".into());
            }
            Ok(())
        })?;
    }

    Ok(ConsistencyReport {
        orphan_files,
        broken_entries,
        repaired,
    })
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::references::model::{ReferenceEntry, ReferenceFormat, ReferenceStatus};

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluen_consistency_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_entry(id: &str) -> ReferenceEntry {
        ReferenceEntry {
            id: id.into(),
            title: format!("Title {id}"),
            original_filename: format!("{id}.pdf"),
            format: ReferenceFormat::Pdf,
            file_hash: format!("hash-{id}"),
            file_path: format!("references/raw/{id}.pdf"),
            md_path: format!("references/md/{id}.md"),
            resource_dir: format!("references/md/resource/{id}"),
            added_at: "2026-07-30T12:00:00Z".into(),
            source: None,
            ai_summary: None,
            status: ReferenceStatus::Completed,
            error: None,
        }
    }

    #[test]
    fn orphan_file_moved() {
        let dir = temp_dir();
        let raw_dir = dir.join("references").join("raw");
        let md_dir = dir.join("references").join("md");
        std::fs::create_dir_all(&raw_dir).unwrap();
        std::fs::create_dir_all(&md_dir).unwrap();

        // 孤儿文件
        std::fs::write(raw_dir.join("ref-orphan.pdf"), b"orphan").unwrap();
        // 有索引的文件
        std::fs::write(raw_dir.join("ref-indexed.pdf"), b"indexed").unwrap();
        std::fs::write(md_dir.join("ref-indexed.md"), b"# Title").unwrap();

        let index = ReferenceIndex::new(&dir);
        index.upsert(sample_entry("ref-indexed")).unwrap();

        let report = check_and_repair(&dir, &index).unwrap();

        assert_eq!(report.orphan_files.len(), 1);
        assert!(report.orphan_files[0].contains("ref-orphan"));
        assert_eq!(report.repaired, 1);
        assert!(report.broken_entries.is_empty());

        // 孤儿文件已移动到 .orphan
        assert!(!raw_dir.join("ref-orphan.pdf").exists());
        assert!(raw_dir.join(".orphan").join("ref-orphan.pdf").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn broken_entry_marked_failed() {
        let dir = temp_dir();
        let raw_dir = dir.join("references").join("raw");
        let md_dir = dir.join("references").join("md");
        std::fs::create_dir_all(&raw_dir).unwrap();
        std::fs::create_dir_all(&md_dir).unwrap();

        // raw 文件缺失
        // md 文件存在
        std::fs::write(md_dir.join("ref-broken.md"), b"# Title").unwrap();

        let index = ReferenceIndex::new(&dir);
        index.upsert(sample_entry("ref-broken")).unwrap();

        let report = check_and_repair(&dir, &index).unwrap();

        assert!(report.orphan_files.is_empty());
        assert_eq!(report.broken_entries.len(), 1);
        assert_eq!(report.broken_entries[0], "ref-broken");

        // 条目已标记为 Failed
        let entry = index.find("ref-broken").unwrap().unwrap();
        assert_eq!(entry.status, ReferenceStatus::Failed);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
