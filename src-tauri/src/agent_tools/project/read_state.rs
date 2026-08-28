//! 项目文件「已完整读取」跟踪器——read-before-write 门的状态层。
//!
//! 背景：智能体用 `project_write` 整文件覆盖或 `project_edit` 局部替换时，
//! 若从未（或不完整地）读过原文，极易静默丢失未读部分的内容。本模块
//! 以**绝对路径**为 key 记录两类事实：
//!
//! - 通过 `project_read` 读到的字符区间（合并后的 `[offset, end)` 列表）
//!   与当时的 `total_chars`；
//! - 文件指纹（mtime + 字节数）：任何外部改动都会使记录失效，强制重读。
//!
//! `is_fully_read` 的判定：指纹与当前文件一致，且已读区间的并集覆盖
//! `[0, total_chars)`（或经由本方 `record_full` 标记为整体已知——
//! write/edit 成功落盘后模型对文件内容有完整认知，不必再强制重读）。
//!
//! 实例由 `MotisChatState` 持有、经装配层共享给所有智能体（总督 /
//! 子代理 / 学术助手），按项目文件路径全局记账。

use std::collections::HashMap;
use std::fs::File;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// 文件指纹：mtime（纳秒，相对 UNIX epoch）+ 字节数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fingerprint {
    mtime_nanos: i128,
    size: u64,
}

impl Fingerprint {
    /// 读取文件元数据构造指纹；文件不存在或无权限时返回 `None`。
    fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        if !meta.is_file() {
            return None;
        }
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as i128)
            .unwrap_or(0);
        Some(Self {
            mtime_nanos: mtime,
            size: meta.len(),
        })
    }
}

/// 单个文件的读取记录。
#[derive(Debug)]
struct ReadRecord {
    /// 已读字符区间（构造上保持有序、两两不相邻不相交）。
    intervals: Vec<(usize, usize)>,
    /// 读取时报告的全文字符数（区间覆盖判定的右边界）。
    total_chars: usize,
    /// 整体已知标记（本方 write / edit 成功后置位）。
    full: bool,
    /// 记录时的文件指纹。
    fingerprint: Fingerprint,
}

/// 项目文件读取状态跟踪器（线程安全，Arc 共享）。
#[derive(Default)]
pub struct ReadTracker {
    records: Mutex<HashMap<PathBuf, ReadRecord>>,
}

/// 记录 key 的规范化：canonicalize 统一大小写与 `\\?\` 前缀
///（Windows 文件系统大小写不敏感，模型两次给出不同写法也应命中同一条目），
/// 目标不存在时回退原路径。
fn canon_key(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

impl ReadTracker {
    /// 构造共享实例。
    pub fn new_arc() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// 登记一次 `project_read` 的成功读取（字符区间 `[offset, end)`）。
    ///
    /// 指纹以登记瞬间的文件元数据为准；若与既有记录的指纹不同（文件
    /// 在外部被改动），旧记录作废、以新窗口重新开始。
    pub fn record_read(&self, path: &Path, offset: usize, end: usize, total_chars: usize) {
        let path = canon_key(path);
        let Some(fp) = Fingerprint::of(&path) else {
            return;
        };
        let mut map = self.records.lock().expect("read tracker poisoned");
        let entry = map.entry(path).or_insert_with(|| ReadRecord {
            intervals: Vec::new(),
            total_chars,
            full: false,
            fingerprint: fp,
        });
        if entry.fingerprint != fp || entry.total_chars != total_chars {
            *entry = ReadRecord {
                intervals: Vec::new(),
                total_chars,
                full: false,
                fingerprint: fp,
            };
        }
        merge_interval(&mut entry.intervals, offset, end);
    }

    /// 标记文件内容整体已知（本方写 / 编辑成功落盘后调用）。
    pub fn record_full(&self, path: &Path) {
        let path = canon_key(path);
        let Some(fp) = Fingerprint::of(&path) else {
            self.records.lock().expect("read tracker poisoned").remove(&path);
            return;
        };
        let mut map = self.records.lock().expect("read tracker poisoned");
        map.insert(
            path,
            ReadRecord {
                intervals: Vec::new(),
                total_chars: usize::MAX,
                full: true,
                fingerprint: fp,
            },
        );
    }

    /// 该文件当前是否「已被完整读取且内容未变更」。
    pub fn is_fully_read(&self, path: &Path) -> bool {
        let path = canon_key(path);
        let Some(fp) = Fingerprint::of(&path) else {
            return false;
        };
        let mut map = self.records.lock().expect("read tracker poisoned");
        let key = path.clone();
        match map.get(&key) {
            None => false,
            Some(record) if record.fingerprint != fp => {
                map.remove(&key);
                false
            }
            Some(record) if record.full => true,
            Some(record) => {
                let need = char_count_of(&path).unwrap_or(record.total_chars);
                covers(&record.intervals, need)
            }
        }
    }

    /// 移除记录（测试 / 清理用途）。
    pub fn forget(&self, path: &Path) {
        self.records
            .lock()
            .expect("read tracker poisoned")
            .remove(&canon_key(path));
    }
}

/// 判断有序不相交区间列表的并集是否覆盖 `[0, total)`。
fn covers(intervals: &[(usize, usize)], total: usize) -> bool {
    if total == 0 {
        // 空文件：有任何针对它的读取记录即视为已读（区间可能为空）。
        return true;
    }
    let mut cursor = 0usize;
    for &(start, end) in intervals {
        if start > cursor {
            return false;
        }
        cursor = cursor.max(end);
        if cursor >= total {
            return true;
        }
    }
    false
}

/// 将 `[start, end)` 并入有序区间列表（合并重叠与相邻区间）。
fn merge_interval(intervals: &mut Vec<(usize, usize)>, start: usize, end: usize) {
    if end <= start {
        return;
    }
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(intervals.len() + 1);
    let mut pending = (start, end);
    let mut inserted = false;
    for &(s, e) in intervals.iter() {
        if e < pending.0 || s > pending.1 {
            // 不相邻：按序输出既有区间；pending 应先行插入一次。
            if !inserted && s > pending.1 {
                merged.push(pending);
                inserted = true;
            }
            merged.push((s, e));
        } else {
            pending = (pending.0.min(s), pending.1.max(e));
        }
    }
    if !inserted {
        merged.push(pending);
    }
    *intervals = merged;
}

/// 文件的当前字符数（UTF-8 解码失败时返回 None）。
fn char_count_of(path: &Path) -> Option<usize> {
    let mut buf = Vec::new();
    File::open(path).ok()?.read_to_end(&mut buf).ok()?;
    String::from_utf8(buf).ok().map(|s| s.chars().count())
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_file(tag: &str, content: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluen_read_tracker_{tag}_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("f.md");
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn single_full_window_read_counts_as_fully_read() {
        let path = temp_file("single", "# 委屈\n## asd\n# asda\n");
        let tracker = ReadTracker::new_arc();
        let total = "# 委屈\n## asd\n# asda\n".chars().count();

        assert!(!tracker.is_fully_read(&path));
        tracker.record_read(&path, 0, total, total);
        assert!(tracker.is_fully_read(&path));

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn partial_window_read_is_not_fully_read() {
        let path = temp_file("partial", "# 委屈\n## asd\n# asda\n");
        let tracker = ReadTracker::new_arc();
        let total = "# 委屈\n## asd\n# asda\n".chars().count();

        tracker.record_read(&path, 0, 5, total);
        assert!(!tracker.is_fully_read(&path));
        tracker.record_read(&path, 5, 12, total);
        assert!(!tracker.is_fully_read(&path));
        tracker.record_read(&path, 12, total, total);
        assert!(tracker.is_fully_read(&path));

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn external_change_invalidates_record() {
        let path = temp_file("external", "old\n");
        let tracker = ReadTracker::new_arc();
        let total = "old\n".chars().count();
        tracker.record_read(&path, 0, total, total);
        assert!(tracker.is_fully_read(&path));

        std::thread::sleep(std::time::Duration::from_millis(15));
        fs::write(&path, "rewritten by user, much longer\n").unwrap();
        assert!(!tracker.is_fully_read(&path), "指纹变化必须作废记录");

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn record_full_bypasses_coverage_until_change() {
        let path = temp_file("full_flag", "# asda\n");
        let tracker = ReadTracker::new_arc();
        tracker.record_full(&path);
        assert!(tracker.is_fully_read(&path));

        std::thread::sleep(std::time::Duration::from_millis(15));
        fs::write(&path, "# asda\n# lost\n").unwrap();
        assert!(!tracker.is_fully_read(&path));

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn merge_interval_joins_adjacent_and_overlapping() {
        let mut v = Vec::new();
        merge_interval(&mut v, 10, 20);
        merge_interval(&mut v, 0, 10); // 相邻 → 合并
        merge_interval(&mut v, 15, 30); // 重叠 → 合并
        merge_interval(&mut v, 50, 60); // 分离
        assert_eq!(v, vec![(0, 30), (50, 60)]);
    }
}
