//! 编辑操作记录——统一描述文本编辑、文件删除、自动优化三类操作。
//!
//! 每个 [`Edit`] 自包含正向（apply）与逆向（revert）应用逻辑，
//! 由 [`History`](super::history::History) 双栈管理。

use std::ops::Range;
use std::path::PathBuf;

/// 操作来源标记。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditLabel {
    /// 用户手动操作。
    User,
    /// 自动优化（如格式化、引用重排）。
    AutoOptimize,
    /// 系统操作（如文件删除）。
    System,
}

/// 历史记录摘要（用于 UI 展示，不暴露内部数据）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct EditSummary {
    /// 操作类型名称（"text" / "file_delete" / "auto_optimize"）。
    pub kind: &'static str,
    /// 时间戳（RFC3339）。
    pub timestamp: String,
    /// 人类可读描述。
    pub description: String,
}

/// 可撤销/重做的编辑操作。
///
/// 每个变体记录操作前后的完整信息，支持正向应用（apply）与逆向回退（revert）。
#[derive(Debug, Clone)]
pub enum Edit {
    /// 文本区间替换。
    Text {
        /// 被替换的区间（UTF-8 字节偏移）。
        range: Range<usize>,
        /// 替换前的文本。
        before: String,
        /// 替换后的文本。
        after: String,
        /// 操作来源。
        label: EditLabel,
        /// 时间戳（RFC3339）。
        timestamp: String,
    },
    /// 文件删除。
    FileDelete {
        /// 被删除的文件路径。
        path: PathBuf,
        /// 文件原内容（供调用方恢复文件使用，apply/revert 不读取）。
        #[allow(dead_code)]
        content_before: String,
        /// 时间戳。
        timestamp: String,
    },
    /// 自动优化（全文替换）。
    AutoOptimize {
        /// 优化前全文。
        before: String,
        /// 优化后全文。
        after: String,
        /// 优化描述。
        description: String,
        /// 时间戳。
        timestamp: String,
    },
}

impl Edit {
    /// 生成当前时间的 RFC3339 时间戳。
    fn now_timestamp() -> String {
        chrono::Utc::now().to_rfc3339()
    }

    /// 创建文本替换记录。
    pub fn text(range: Range<usize>, before: String, after: String, label: EditLabel) -> Self {
        Self::Text {
            range,
            before,
            after,
            label,
            timestamp: Self::now_timestamp(),
        }
    }

    /// 创建文件删除记录。
    pub fn file_delete(path: PathBuf, content_before: String) -> Self {
        Self::FileDelete {
            path,
            content_before,
            timestamp: Self::now_timestamp(),
        }
    }

    /// 创建自动优化记录。
    pub fn auto_optimize(before: String, after: String, description: String) -> Self {
        Self::AutoOptimize {
            before,
            after,
            description,
            timestamp: Self::now_timestamp(),
        }
    }

    /// 正向应用到给定文本，返回新文本。
    ///
    /// - `Text`：将 `before` 替换为 `after`
    /// - `FileDelete`：不影响文本，返回原文本（文件恢复由调用方处理）
    /// - `AutoOptimize`：返回 `after`
    pub fn apply(&self, source: &str) -> String {
        match self {
            Self::Text {
                range,
                before,
                after,
                ..
            } => {
                // 校验 source 中 range 位置确实是 before
                if source.get(range.clone()) == Some(before.as_str()) {
                    let mut result =
                        String::with_capacity(source.len() + after.len() - before.len());
                    result.push_str(&source[..range.start]);
                    result.push_str(after);
                    result.push_str(&source[range.end..]);
                    result
                } else {
                    // 区间不匹配，返回原文本（防御性）
                    source.to_string()
                }
            }
            Self::FileDelete { .. } => source.to_string(),
            Self::AutoOptimize { after, .. } => after.clone(),
        }
    }

    /// 逆向回退给定文本，返回新文本。
    ///
    /// - `Text`：将 `after` 替换回 `before`。range.start 保持不变（替换起始位置
    ///   不受 before/after 长度差异影响），end 按 after 长度重新计算。
    /// - `FileDelete`：不影响文本，返回原文本
    /// - `AutoOptimize`：返回 `before`
    pub fn revert(&self, source: &str) -> String {
        match self {
            Self::Text {
                range,
                before,
                after,
                ..
            } => {
                // range.start 在 apply 后仍指向 after 的起始位置
                let after_range = range.start..range.start + after.len();
                if source.get(after_range.clone()) == Some(after.as_str()) {
                    let mut result =
                        String::with_capacity(source.len() + before.len() - after.len());
                    result.push_str(&source[..after_range.start]);
                    result.push_str(before);
                    result.push_str(&source[after_range.end..]);
                    result
                } else {
                    // 区间不匹配，返回原文本（防御性）
                    source.to_string()
                }
            }
            Self::FileDelete { .. } => source.to_string(),
            Self::AutoOptimize { before, .. } => before.clone(),
        }
    }

    /// 生成操作摘要。
    pub fn summary(&self) -> EditSummary {
        match self {
            Self::Text {
                before,
                after,
                label,
                timestamp,
                ..
            } => EditSummary {
                kind: "text",
                timestamp: timestamp.clone(),
                description: format!(
                    "{:?}: {:?} → {:?}",
                    label,
                    before.chars().take(20).collect::<String>(),
                    after.chars().take(20).collect::<String>(),
                ),
            },
            Self::FileDelete { path, timestamp, .. } => EditSummary {
                kind: "file_delete",
                timestamp: timestamp.clone(),
                description: format!("{}", path.display()),
            },
            Self::AutoOptimize {
                description,
                timestamp,
                ..
            } => EditSummary {
                kind: "auto_optimize",
                timestamp: timestamp.clone(),
                description: description.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_apply_replaces_before_with_after() {
        // 原文 "hello world"，替换 "world" -> "rust"
        let source = "hello world";
        // "world" 起始于字节 6，结束于字节 11
        let edit = Edit::text(6..11, "world".to_string(), "rust".to_string(), EditLabel::User);
        let applied = edit.apply(source);
        assert_eq!(applied, "hello rust");
    }

    #[test]
    fn text_revert_restores_before_from_after() {
        // before/after 等长，保证 range 在 apply 后仍指向 after
        // 原文 "hello world"，"world" -> "WORLD"，应用后为 "hello WORLD"
        let source = "hello WORLD";
        let edit = Edit::text(
            6..11,
            "world".to_string(),
            "WORLD".to_string(),
            EditLabel::User,
        );
        let reverted = edit.revert(source);
        assert_eq!(reverted, "hello world");
    }

    #[test]
    fn text_apply_roundtrip_preserves_original() {
        // before/after 等长，roundtrip 后应恢复原文
        let original = "hello world";
        let edit = Edit::text(
            6..11,
            "world".to_string(),
            "WORLD".to_string(),
            EditLabel::User,
        );
        let applied = edit.apply(original);
        assert_eq!(applied, "hello WORLD");
        let reverted = edit.revert(&applied);
        assert_eq!(reverted, original);
    }

    #[test]
    fn text_roundtrip_unequal_length() {
        // before/after 不等长：before="world"(5), after="rust"(4)
        // 验证 revert 正确计算 after 的区间
        let original = "hello world";
        let edit = Edit::text(
            6..11,
            "world".to_string(),
            "rust".to_string(),
            EditLabel::User,
        );
        let applied = edit.apply(original);
        assert_eq!(applied, "hello rust");
        let reverted = edit.revert(&applied);
        assert_eq!(reverted, original);
    }

    #[test]
    fn text_roundtrip_insertion() {
        // 零长度区间（插入）：before="", after="new "
        let original = "hello world";
        let edit = Edit::text(
            6..6,
            "".to_string(),
            "new ".to_string(),
            EditLabel::User,
        );
        let applied = edit.apply(original);
        assert_eq!(applied, "hello new world");
        let reverted = edit.revert(&applied);
        assert_eq!(reverted, original);
    }

    #[test]
    fn text_apply_mismatch_range_returns_original() {
        // source 的 range 位置不是 before，应原样返回
        let source = "hello rust";
        let edit = Edit::text(6..11, "world".to_string(), "rust".to_string(), EditLabel::User);
        // 这里 source[6..11] = "rust"，但 before 是 "world"，不匹配
        let applied = edit.apply(source);
        assert_eq!(applied, source);
    }

    #[test]
    fn auto_optimize_apply_returns_after() {
        let edit = Edit::auto_optimize(
            "old content".to_string(),
            "new content".to_string(),
            "格式化".to_string(),
        );
        let applied = edit.apply("任意原文");
        assert_eq!(applied, "new content");
    }

    #[test]
    fn auto_optimize_revert_returns_before() {
        let edit = Edit::auto_optimize(
            "old content".to_string(),
            "new content".to_string(),
            "格式化".to_string(),
        );
        let reverted = edit.revert("任意原文");
        assert_eq!(reverted, "old content");
    }

    #[test]
    fn file_delete_apply_does_not_affect_text() {
        let edit = Edit::file_delete(PathBuf::from("/tmp/foo.md"), "原内容".to_string());
        let applied = edit.apply("当前文本");
        assert_eq!(applied, "当前文本");
    }

    #[test]
    fn file_delete_revert_does_not_affect_text() {
        let edit = Edit::file_delete(PathBuf::from("/tmp/foo.md"), "原内容".to_string());
        let reverted = edit.revert("当前文本");
        assert_eq!(reverted, "当前文本");
    }

    #[test]
    fn summary_text_returns_correct_kind() {
        let edit = Edit::text(
            0..5,
            "hello".to_string(),
            "hi".to_string(),
            EditLabel::User,
        );
        let s = edit.summary();
        assert_eq!(s.kind, "text");
        assert!(s.description.contains("User"));
        assert!(!s.timestamp.is_empty());
    }

    #[test]
    fn summary_file_delete_returns_correct_kind() {
        let edit = Edit::file_delete(PathBuf::from("/tmp/foo.md"), "x".to_string());
        let s = edit.summary();
        assert_eq!(s.kind, "file_delete");
        assert!(s.description.contains("foo.md"));
    }

    #[test]
    fn summary_auto_optimize_returns_correct_kind() {
        let edit = Edit::auto_optimize(
            "a".to_string(),
            "b".to_string(),
            "引用重排".to_string(),
        );
        let s = edit.summary();
        assert_eq!(s.kind, "auto_optimize");
        assert_eq!(s.description, "引用重排");
    }
}
