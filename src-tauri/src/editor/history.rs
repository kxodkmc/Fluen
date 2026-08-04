//! 历史栈——基于双 VecDeque 的撤销/重做管理。
//!
//! - `undo_stack` 栈顶为最近操作
//! - `redo_stack` 栈顶为最近撤销的操作
//! - 超过 `max_size` 时自动淘汰栈底最旧记录

use std::collections::VecDeque;

use super::edit::{Edit, EditSummary};

/// 撤销/重做历史栈。
#[derive(Debug, Clone)]
pub struct History {
    /// 可撤销操作栈（栈顶 = back）。
    undo_stack: VecDeque<Edit>,
    /// 可重做操作栈（栈顶 = back）。
    redo_stack: VecDeque<Edit>,
    /// 单栈最大记录数。
    max_size: usize,
}

impl History {
    /// 创建空历史栈。
    pub fn new(max_size: usize) -> Self {
        Self {
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            max_size,
        }
    }

    /// 记录新操作：推入 undo_stack 栈顶，清空 redo_stack。
    /// 超过 max_size 时淘汰栈底最旧记录。
    pub fn push(&mut self, edit: Edit) {
        self.redo_stack.clear();
        self.undo_stack.push_back(edit);
        // 超限淘汰
        while self.undo_stack.len() > self.max_size {
            self.undo_stack.pop_front();
        }
    }

    /// 撤销：弹出 undo_stack 栈顶，推入 redo_stack。
    /// 返回 Some(edit) 若成功，None 若 undo_stack 为空。
    pub fn pop_undo(&mut self) -> Option<Edit> {
        let edit = self.undo_stack.pop_back()?;
        self.redo_stack.push_back(edit.clone());
        // redo 也需遵守 max_size
        while self.redo_stack.len() > self.max_size {
            self.redo_stack.pop_front();
        }
        Some(edit)
    }

    /// 重做：弹出 redo_stack 栈顶，推入 undo_stack。
    /// 返回 Some(edit) 若成功，None 若 redo_stack 为空。
    pub fn pop_redo(&mut self) -> Option<Edit> {
        let edit = self.redo_stack.pop_back()?;
        self.undo_stack.push_back(edit.clone());
        while self.undo_stack.len() > self.max_size {
            self.undo_stack.pop_front();
        }
        Some(edit)
    }

    /// 清空两个栈。
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// 返回 (undo_count, redo_count)。
    pub fn len(&self) -> (usize, usize) {
        (self.undo_stack.len(), self.redo_stack.len())
    }

    /// 是否可撤销。
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// 是否可重做。
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// 返回最近 n 条历史摘要（从栈顶开始）。
    pub fn preview(&self, n: usize) -> Vec<EditSummary> {
        self.undo_stack
            .iter()
            .rev()
            .take(n)
            .map(|e| e.summary())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::edit::EditLabel;

    /// 构造一条简单的 Text 编辑记录，便于测试。
    fn make_edit(tag: &str) -> Edit {
        Edit::text(
            0..tag.len(),
            tag.to_string(),
            tag.to_uppercase(),
            EditLabel::User,
        )
    }

    #[test]
    fn push_enables_undo_disables_redo() {
        let mut h = History::new(10);
        assert!(!h.can_undo());
        assert!(!h.can_redo());

        h.push(make_edit("a"));
        assert!(h.can_undo());
        assert!(!h.can_redo());
        assert_eq!(h.len(), (1, 0));
    }

    #[test]
    fn push_new_edit_clears_redo_stack() {
        let mut h = History::new(10);
        h.push(make_edit("a"));
        h.push(make_edit("b"));
        // 撤销一次，redo 栈应有一条
        h.pop_undo();
        assert_eq!(h.len(), (1, 1));
        assert!(h.can_redo());

        // 再 push 新操作，redo 应被清空
        h.push(make_edit("c"));
        assert_eq!(h.len(), (2, 0));
        assert!(!h.can_redo());
    }

    #[test]
    fn pop_undo_pop_redo_roundtrip() {
        let mut h = History::new(10);
        h.push(make_edit("a"));
        h.push(make_edit("b"));

        // 撤销最近一条（"b"）
        let undone = h.pop_undo().expect("应可撤销");
        assert_eq!(undone.summary().description, make_edit("b").summary().description);
        assert_eq!(h.len(), (1, 1));
        assert!(h.can_redo());

        // 重做该条
        let redone = h.pop_redo().expect("应可重做");
        assert_eq!(redone.summary().description, make_edit("b").summary().description);
        assert_eq!(h.len(), (2, 0));
        assert!(!h.can_redo());
    }

    #[test]
    fn pop_undo_returns_none_when_empty() {
        let mut h = History::new(10);
        assert!(h.pop_undo().is_none());
    }

    #[test]
    fn pop_redo_returns_none_when_empty() {
        let mut h = History::new(10);
        assert!(h.pop_redo().is_none());
    }

    #[test]
    fn evicts_oldest_when_exceeding_max_size() {
        let mut h = History::new(2);
        h.push(make_edit("a"));
        h.push(make_edit("b"));
        h.push(make_edit("c"));
        // max_size=2，应淘汰 "a"
        assert_eq!(h.len(), (2, 0));
        // 栈顶应是最近 "c"，次之 "b"
        let top = h.pop_undo().expect("应可撤销");
        assert_eq!(top.summary().description, make_edit("c").summary().description);
        let next = h.pop_undo().expect("应可撤销");
        assert_eq!(next.summary().description, make_edit("b").summary().description);
        // 再撤销应为 None（"a" 已被淘汰）
        assert!(h.pop_undo().is_none());
    }

    #[test]
    fn clear_empties_both_stacks() {
        let mut h = History::new(10);
        h.push(make_edit("a"));
        h.push(make_edit("b"));
        h.pop_undo(); // 产生 redo 记录
        assert_eq!(h.len(), (1, 1));

        h.clear();
        assert_eq!(h.len(), (0, 0));
        assert!(!h.can_undo());
        assert!(!h.can_redo());
    }

    #[test]
    fn preview_returns_most_recent_n_summaries() {
        let mut h = History::new(10);
        h.push(make_edit("a"));
        h.push(make_edit("b"));
        h.push(make_edit("c"));

        let previews = h.preview(2);
        assert_eq!(previews.len(), 2);
        // 栈顶在前
        assert_eq!(previews[0].description, make_edit("c").summary().description);
        assert_eq!(previews[1].description, make_edit("b").summary().description);

        // 请求超过栈大小，返回全部
        let all = h.preview(10);
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].description, make_edit("c").summary().description);
        assert_eq!(all[2].description, make_edit("a").summary().description);
    }

    #[test]
    fn preview_empty_history_returns_empty() {
        let h = History::new(10);
        let previews = h.preview(5);
        assert!(previews.is_empty());
    }

    #[test]
    fn redo_stack_also_respects_max_size() {
        let mut h = History::new(2);
        h.push(make_edit("a"));
        h.push(make_edit("b"));
        // 撤销两次，redo 栈会有两条
        h.pop_undo();
        h.pop_undo();
        assert_eq!(h.len(), (0, 2));

        // 再 push 一条新操作，触发 redo 清空（push 总是清空 redo）
        h.push(make_edit("c"));
        assert_eq!(h.len(), (1, 0));
    }

    #[test]
    fn clone_preserves_state() {
        let mut h = History::new(10);
        h.push(make_edit("a"));
        h.pop_undo();

        let h2 = h.clone();
        assert_eq!(h2.len(), (0, 1));
        assert!(h2.can_redo());
    }
}
