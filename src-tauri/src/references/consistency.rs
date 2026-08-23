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
use super::frontmatter::ReferenceFrontmatter;
use super::model::{ConsistencyReport, ReferenceEntry, ReferenceStatus};
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

/// 回填索引中缺失的元数据（`authors` / `year`）。
///
/// 旧版项目索引不含 `year` 字段，部分导入模式也不写 `authors`；
/// 而两者均已写入 MD frontmatter。本函数对 `Completed` 且缺字段的条目
/// 读取其 MD frontmatter 并回填索引（一次事务原子写回）。
///
/// - MD 文件缺失或无对应字段 → 跳过该条目，不影响其余；
/// - 无需回填时直接返回 0，不产生写事务。
///
/// 返回回填的条目数。
pub fn backfill_missing_metadata(
    project_dir: &Path,
    index: &ReferenceIndex,
) -> Result<usize, ReferenceError> {
    let entries = index.list()?;
    let candidates: Vec<&ReferenceEntry> = entries
        .iter()
        .filter(|e| e.status == ReferenceStatus::Completed && (e.authors.is_none() || e.year.is_none()))
        .collect();
    if candidates.is_empty() {
        return Ok(0);
    }

    // 逐条读 MD frontmatter，仅提取缺失字段
    let mut filled: Vec<(String, Option<Vec<String>>, Option<String>)> = Vec::new();
    for e in &candidates {
        let md = match std::fs::read_to_string(project_dir.join(&e.md_path)) {
            Ok(md) => md,
            Err(_) => continue, // MD 缺失交由 check_and_repair 标记，此处跳过
        };
        let (fm, _) = ReferenceFrontmatter::split_from_markdown(&md);
        let authors = if e.authors.is_none() { fm.authors_for_index() } else { None };
        let year = if e.year.is_none() { fm.year.clone() } else { None };
        if authors.is_some() || year.is_some() {
            filled.push((e.id.clone(), authors, year));
        }
    }
    if filled.is_empty() {
        return Ok(0);
    }

    let count = filled.len();
    index.update(|entries| {
        for (id, authors, year) in &filled {
            if let Some(e) = entries.iter_mut().find(|e| &e.id == id) {
                if e.authors.is_none() {
                    e.authors = authors.clone();
                }
                if e.year.is_none() {
                    e.year = year.clone();
                }
            }
        }
        Ok(())
    })?;
    Ok(count)
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
            authors: None,
            year: None,
            import_mode: crate::references::import_mode::ReferenceImportMode::Ocr,
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

    // ── backfill_missing_metadata ──

    /// 辅助：写一个带 frontmatter 的 MD 文件。
    fn write_md(dir: &std::path::Path, id: &str, frontmatter: &str) {
        let md_dir = dir.join("references").join("md");
        std::fs::create_dir_all(&md_dir).unwrap();
        std::fs::write(
            md_dir.join(format!("{id}.md")),
            format!("{frontmatter}\n# 正文\n内容"),
        )
        .unwrap();
    }

    // 缺失的 authors/year 从 MD frontmatter 回填
    #[test]
    fn backfill_fills_missing_metadata_from_frontmatter() {
        let dir = temp_dir();
        write_md(&dir, "ref-a", "---\ntitle: 标题\nauthors:\n  - 张三\nyear: '2025'\n---");

        let index = ReferenceIndex::new(&dir);
        index.upsert(sample_entry("ref-a")).unwrap();

        let filled = backfill_missing_metadata(&dir, &index).unwrap();
        assert_eq!(filled, 1);

        let entry = index.find("ref-a").unwrap().unwrap();
        assert_eq!(entry.authors.as_deref(), Some(["张三".to_string()].as_slice()));
        assert_eq!(entry.year.as_deref(), Some("2025"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // 仅回填缺失字段，已有字段不被覆盖
    #[test]
    fn backfill_preserves_existing_fields() {
        let dir = temp_dir();
        write_md(&dir, "ref-a", "---\ntitle: 标题\nauthors:\n  - 李四\nyear: '1999'\n---");

        let index = ReferenceIndex::new(&dir);
        let mut entry = sample_entry("ref-a");
        entry.authors = Some(vec!["张三".into()]);
        index.upsert(entry).unwrap();

        let filled = backfill_missing_metadata(&dir, &index).unwrap();
        assert_eq!(filled, 1);

        let entry = index.find("ref-a").unwrap().unwrap();
        // authors 保持原值，year 被回填
        assert_eq!(entry.authors.as_deref(), Some(["张三".to_string()].as_slice()));
        assert_eq!(entry.year.as_deref(), Some("1999"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // 全部字段齐全时不产生回填（返回 0）
    #[test]
    fn backfill_returns_zero_when_complete() {
        let dir = temp_dir();
        write_md(&dir, "ref-a", "---\ntitle: 标题\n---");

        let index = ReferenceIndex::new(&dir);
        let mut entry = sample_entry("ref-a");
        entry.authors = Some(vec!["张三".into()]);
        entry.year = Some("2025".into());
        index.upsert(entry).unwrap();

        assert_eq!(backfill_missing_metadata(&dir, &index).unwrap(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // MD 文件缺失时跳过该条目，不报错
    #[test]
    fn backfill_skips_missing_md() {
        let dir = temp_dir();

        let index = ReferenceIndex::new(&dir);
        index.upsert(sample_entry("ref-a")).unwrap();

        assert_eq!(backfill_missing_metadata(&dir, &index).unwrap(), 0);
        let entry = index.find("ref-a").unwrap().unwrap();
        assert!(entry.authors.is_none());
        assert!(entry.year.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // 非 Completed 状态不回填（其 MD 可能不完整）
    #[test]
    fn backfill_ignores_non_completed_entries() {
        let dir = temp_dir();
        write_md(&dir, "ref-a", "---\nyear: '2025'\n---");

        let index = ReferenceIndex::new(&dir);
        let mut entry = sample_entry("ref-a");
        entry.status = ReferenceStatus::Failed;
        index.upsert(entry).unwrap();

        assert_eq!(backfill_missing_metadata(&dir, &index).unwrap(), 0);
        let entry = index.find("ref-a").unwrap().unwrap();
        assert!(entry.year.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
