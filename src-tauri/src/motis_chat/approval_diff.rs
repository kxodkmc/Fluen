//! 审批请求 diff 计算——在弹窗前把「将要发生什么变更」算给前端。
//!
//! [`MotisApprover`](super::approval::MotisApprover) 发起审批时调用
//! [`build_approval_diff`]：按工具语义在内存中推演目标文件的新内容，
//! 与磁盘当前原文做行级 diff（`similar`），产出**只含变更块**的
//! hunk 结构——未改动的大段论文文本以 `gap_before` 计数折叠，
//! 全文行数超出上限时截断并置 `truncated`，避免撑爆事件 payload。
//!
//! 推演失败（路径非法、old_string 不唯一等）时返回 `None`，
//! 前端回退到旧的截断摘要展示，绝不因 diff 计算失败阻断审批流程。

use std::path::Path;

use serde::Serialize;
use serde_json::Value;
use similar::{ChangeTag, TextDiff};

use crate::agent_tools::manuscript::MANUSCRIPT_TOOL_NAME;
use crate::agent_tools::project::edit::PROJECT_EDIT_TOOL_NAME;
use crate::agent_tools::project::write::PROJECT_WRITE_TOOL_NAME;
use crate::agent_tools::project::ProjectFs;

/// diff 序列化行数上限（超出截断并置 `truncated`）。
const MAX_DIFF_LINES: usize = 400;

/// hunk 上下文的原文行数。
const CONTEXT_LINES: usize = 3;

/// 一次写操作的行级 diff（审批卡片摘要 + 全屏 hunk 视图数据源）。
#[derive(Debug, Clone, Serialize)]
pub struct ApprovalDiff {
    /// 展示路径（项目相对；manuscript 固定为 `manuscript/main.md`）。
    pub path: String,
    /// 新增行数（全文统计，不受截断影响）。
    pub added: usize,
    /// 删除行数（同上）。
    pub removed: usize,
    /// 行数超出上限，hunk 列表被截断。
    pub truncated: bool,
    /// 变更块列表（按文件顺序）。
    pub hunks: Vec<DiffHunk>,
}

/// 单个变更块。
#[derive(Debug, Clone, Serialize)]
pub struct DiffHunk {
    /// 本块之前被折叠的未变更行数（首块 = 文件开头未变更行数）。
    pub gap_before: usize,
    /// 块内行（含上下文行）。
    pub lines: Vec<DiffLine>,
}

/// diff 中的一行。
#[derive(Debug, Clone, Serialize)]
pub struct DiffLine {
    /// 行类别：`ctx`（上下文）/ `add`（新增）/ `del`（删除）。
    pub kind: &'static str,
    /// 行文本（不含行尾换行符）。
    pub text: String,
}

/// 计算一次审批请求的 diff。
///
/// - `project_root`：当前论文项目根（`None` = 无项目，返回 `None`）；
/// - 路径经 [`ProjectFs`] 校验（拒绝越界 / `.git`），manuscript 固定
///   指向 `manuscript/main.md`。
pub fn build_approval_diff(
    project_root: Option<&str>,
    tool_name: &str,
    input: &Value,
) -> Option<ApprovalDiff> {
    let root = project_root?;
    let fs = ProjectFs::new(root.to_string());

    // 1. 目标路径与展示名
    let (display, abs) = match tool_name {
        MANUSCRIPT_TOOL_NAME => (
            "manuscript/main.md".to_string(),
            fs.resolve_for_read("manuscript/main.md").ok()?,
        ),
        PROJECT_WRITE_TOOL_NAME | PROJECT_EDIT_TOOL_NAME => {
            let rel = input.get("path").and_then(Value::as_str)?;
            // 仅作展示用解析：解析失败（越界 / 目录不存在等）直接无 diff，
            // 真正的路径校验在审批放行后由写工具自身执行
            (rel.to_string(), fs.resolve_for_read(rel).ok()?)
        }
        _ => return None,
    };

    // 2. 磁盘原文（不存在 = 新建，视为空）
    let old = read_text_if_exists(&abs);
    // 3. 按工具语义推演新内容
    let new = prospective_content(tool_name, &old, input)?;

    // 4. 行级 diff → hunk 结构
    Some(assemble_diff(display, &old, &new))
}

/// 读取文本文件；不存在 / 非 UTF-8 时返回空串（新建或二进制走无 diff 之外的宽松路径）。
fn read_text_if_exists(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// 按工具入参推演操作后的完整新内容；推演失败返回 `None`。
fn prospective_content(tool_name: &str, old: &str, input: &Value) -> Option<String> {
    match tool_name {
        MANUSCRIPT_TOOL_NAME | PROJECT_WRITE_TOOL_NAME => {
            input.get("content").and_then(Value::as_str).map(str::to_string)
        }
        PROJECT_EDIT_TOOL_NAME => {
            let from = input.get("old_string").and_then(Value::as_str)?;
            let to = input.get("new_string").and_then(Value::as_str)?;
            if from.is_empty() {
                return None;
            }
            let count = old.matches(from).count();
            let replace_all = input
                .get("replace_all")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if replace_all {
                (count >= 1).then(|| old.replace(from, to))
            } else if count == 1 {
                Some(old.replacen(from, to, 1))
            } else {
                None // 0 次（必然失败）或多次（歧义）：不给 diff
            }
        }
        _ => None,
    }
}

/// old → new 行级 diff，折叠未变更区域为 gap 计数。
fn assemble_diff(path: String, old: &str, new: &str) -> ApprovalDiff {
    let diff = TextDiff::from_lines(old, new);

    let mut added = 0usize;
    let mut removed = 0usize;
    let mut all: Vec<(ChangeTag, String)> = Vec::new();
    for change in diff.iter_all_changes() {
        let text = change.to_string_lossy().trim_end_matches(['\n', '\r']).to_string();
        match change.tag() {
            ChangeTag::Insert => added += 1,
            ChangeTag::Delete => removed += 1,
            ChangeTag::Equal => {}
        }
        all.push((change.tag(), text));
    }

    // 第一遍：标记保留窗口（变更行前后 CONTEXT_LINES 内的上下文）
    let keep: Vec<bool> = {
        let mut k = vec![false; all.len()];
        for (i, (tag, _)) in all.iter().enumerate() {
            if *tag == ChangeTag::Equal {
                continue;
            }
            let lo = i.saturating_sub(CONTEXT_LINES);
            let hi = (i + CONTEXT_LINES).min(all.len().saturating_sub(1));
            if hi < lo {
                continue;
            }
            for slot in k.iter_mut().take(hi + 1).skip(lo) {
                *slot = true;
            }
        }
        k
    };

    // 第二遍：按保留段聚合 hunk
    let mut hunks: Vec<DiffHunk> = Vec::new();
    let mut gap = 0usize;
    let mut truncated = false;
    let mut emitted = 0usize;
    let mut i = 0usize;
    while i < all.len() {
        if !keep[i] {
            gap += 1;
            i += 1;
            continue;
        }
        if emitted >= MAX_DIFF_LINES {
            truncated = true;
            break;
        }
        let mut lines: Vec<DiffLine> = Vec::new();
        while i < all.len() && keep[i] {
            let kind = match all[i].0 {
                ChangeTag::Equal => "ctx",
                ChangeTag::Insert => "add",
                ChangeTag::Delete => "del",
            };
            lines.push(DiffLine {
                kind,
                text: all[i].1.clone(),
            });
            emitted += 1;
            i += 1;
            if emitted >= MAX_DIFF_LINES {
                truncated = true;
                break;
            }
        }
        hunks.push(DiffHunk {
            gap_before: std::mem::take(&mut gap),
            lines,
        });
    }

    ApprovalDiff {
        path,
        added,
        removed,
        truncated,
        hunks,
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    fn temp_project(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluen_approval_diff_{tag}_{}_{:?}_{}",
            std::process::id(),
            std::thread::current().id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn edit_diff_marks_removed_line() {
        let dir = temp_project("edit");
        fs::write(
            dir.join("a.md"),
            "# 委屈\n## asd\n旧内容\n# asda\n尾部一节\n",
        )
        .unwrap();
        let input = json!({
            "path": "a.md",
            "old_string": "## asd\n旧内容\n",
            "new_string": "## asd\n新内容\n",
        });
        let d = build_approval_diff(dir.to_str(), PROJECT_EDIT_TOOL_NAME, &input).unwrap();
        assert_eq!(d.path, "a.md");
        assert_eq!(d.added, 1);
        assert_eq!(d.removed, 1);
        assert!(!d.hunks.is_empty());
        // 删除行标红来源：kind=del 且文本为被删原文
        assert!(d
            .hunks
            .iter()
            .flat_map(|h| h.lines.iter())
            .any(|l| l.kind == "del" && l.text == "旧内容"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_new_file_all_add() {
        let dir = temp_project("newfile");
        let input = json!({ "path": "notes.md", "content": "# 新文件\n内容\n" });
        let d = build_approval_diff(dir.to_str(), PROJECT_WRITE_TOOL_NAME, &input).unwrap();
        assert_eq!(d.added, 2);
        assert_eq!(d.removed, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ambiguous_edit_yields_no_diff() {
        let dir = temp_project("ambig");
        fs::write(dir.join("a.md"), "x\nx\n").unwrap();
        let input = json!({ "path": "a.md", "old_string": "x", "new_string": "y" });
        assert!(build_approval_diff(dir.to_str(), PROJECT_EDIT_TOOL_NAME, &input).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn traversal_and_unknown_tools_rejected() {
        let dir = temp_project("safe");
        let input = json!({ "path": "../evil.md", "content": "x" });
        assert!(build_approval_diff(dir.to_str(), PROJECT_WRITE_TOOL_NAME, &input).is_none());
        assert!(build_approval_diff(None, PROJECT_WRITE_TOOL_NAME, &input).is_none());
        assert!(build_approval_diff(dir.to_str(), "project_read", &input).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unchanged_file_diff_is_empty() {
        let dir = temp_project("noop");
        fs::write(dir.join("a.md"), "same\n").unwrap();
        let input = json!({ "path": "a.md", "content": "same\n" });
        let d = build_approval_diff(dir.to_str(), PROJECT_WRITE_TOOL_NAME, &input).unwrap();
        assert_eq!(d.added, 0);
        assert_eq!(d.removed, 0);
        assert!(d.hunks.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn large_change_truncated() {
        let dir = temp_project("big");
        let old: String = (0..1000).map(|i| format!("line {i}\n")).collect();
        fs::write(dir.join("a.md"), &old).unwrap();
        let new: String = (0..1000).map(|i| format!("changed {i}\n")).collect();
        let input = json!({ "path": "a.md", "content": new });
        let d = build_approval_diff(dir.to_str(), PROJECT_WRITE_TOOL_NAME, &input).unwrap();
        assert!(d.truncated);
        assert_eq!(d.added, 1000);
        assert_eq!(d.removed, 1000);
        let total_lines: usize = d.hunks.iter().map(|h| h.lines.len()).sum();
        assert!(total_lines <= MAX_DIFF_LINES);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn manuscript_diff_uses_main_md() {
        let dir = temp_project("mainmd");
        fs::create_dir_all(dir.join("manuscript")).unwrap();
        fs::write(dir.join("manuscript").join("main.md"), "# 旧\n").unwrap();
        let input = json!({ "action": "update", "content": "# 新\n" });
        let d = build_approval_diff(dir.to_str(), MANUSCRIPT_TOOL_NAME, &input).unwrap();
        assert_eq!(d.path, "manuscript/main.md");
        assert_eq!(d.added, 1);
        assert_eq!(d.removed, 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
