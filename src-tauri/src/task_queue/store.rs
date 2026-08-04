//! 任务队列的 JSON 持久化层。
//!
//! 每个项目独立一份 `data/task-queue.json`，单项目隔离。
//! 写入采用原子操作（先写 `.tmp` 再 `rename`），避免读到半写状态。
//!
//! ## 设计
//!
//! - `TaskStore` 持有项目路径，所有操作围绕 `{project}/data/task-queue.json`。
//! - 读取时若文件不存在返回空队列（`TaskQueueFile::new()`），不视为错误。
//! - 文件损坏时返回错误，由上层决定备份重置或上报。

use std::path::{Path, PathBuf};

use super::error::TaskQueueError;
use super::types::{TaskKind, TaskQueueFile, TaskRecord, TaskStatus};

/// 任务队列文件名（位于项目 `data/` 目录下）。
const TASK_QUEUE_FILE_NAME: &str = "task-queue.json";

/// 项目级任务队列持久化层。
///
/// 单项目隔离：每个实例绑定一个项目路径，
/// 所有读写操作围绕 `{project_path}/data/task-queue.json`。
pub struct TaskStore {
    /// 项目根路径。
    project_path: PathBuf,
    /// 任务队列文件完整路径。
    queue_file: PathBuf,
}

impl TaskStore {
    /// 创建任务队列存储实例。
    ///
    /// 文件路径解析为 `{project_path}/data/task-queue.json`，
    /// 父目录在首次保存时按需创建。
    pub fn new(project_path: impl AsRef<Path>) -> Self {
        let project_path = project_path.as_ref().to_path_buf();
        let queue_file = project_path.join("data").join(TASK_QUEUE_FILE_NAME);
        Self {
            project_path,
            queue_file,
        }
    }

    /// 返回项目根路径。
    pub fn project_path(&self) -> &Path {
        &self.project_path
    }

    /// 返回任务队列文件完整路径。
    pub fn queue_file_path(&self) -> &Path {
        &self.queue_file
    }

    /// 从磁盘加载整个队列文件。
    ///
    /// 文件不存在时返回空结构（不视为错误）。
    /// 文件存在但解析失败时返回错误。
    pub fn load(&self) -> Result<TaskQueueFile, TaskQueueError> {
        match std::fs::read_to_string(&self.queue_file) {
            Ok(content) => {
                let file: TaskQueueFile = serde_json::from_str(&content)?;
                Ok(file)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(TaskQueueFile::new()),
            Err(e) => Err(e.into()),
        }
    }

    /// 原子写入整个队列文件。
    ///
    /// 父目录按需创建。先写入 `.tmp` 临时文件，再 `rename` 覆盖目标文件。
    pub fn save(&self, file: &TaskQueueFile) -> Result<(), TaskQueueError> {
        if let Some(parent) = self.queue_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.queue_file.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(file)?;
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &self.queue_file)?;
        Ok(())
    }

    /// 入队新任务（Pending 状态）。
    ///
    /// 返回新创建的任务记录（含生成的 ID）。
    pub fn enqueue(
        &self,
        kind: TaskKind,
    ) -> Result<TaskRecord, TaskQueueError> {
        let mut file = self.load()?;
        let record = TaskRecord::new_pending(self.project_path.to_string_lossy().as_ref(), kind);
        file.tasks.push(record.clone());
        self.save(&file)?;
        Ok(record)
    }

    /// 列出该项目下所有任务（按创建时间倒序）。
    pub fn list(&self) -> Result<Vec<TaskRecord>, TaskQueueError> {
        let mut file = self.load()?;
        file.tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(file.tasks)
    }

    /// 按状态过滤列出任务。
    pub fn list_with_status(
        &self,
        statuses: &[TaskStatus],
    ) -> Result<Vec<TaskRecord>, TaskQueueError> {
        let tasks = self.list()?;
        Ok(tasks
            .into_iter()
            .filter(|t| statuses.contains(&t.status))
            .collect())
    }

    /// 按 ID 查找任务。
    pub fn find(&self, task_id: &str) -> Result<TaskRecord, TaskQueueError> {
        let file = self.load()?;
        file.tasks
            .into_iter()
            .find(|t| t.id == task_id)
            .ok_or_else(|| TaskQueueError::NotFound(task_id.to_string()))
    }

    /// 取下一个 Pending 任务（按创建时间最早）。
    ///
    /// 无 Pending 任务时返回 `None`。
    pub fn next_pending(&self) -> Result<Option<TaskRecord>, TaskQueueError> {
        let file = self.load()?;
        let mut pending: Vec<_> = file
            .tasks
            .into_iter()
            .filter(|t| t.status == TaskStatus::Pending)
            .collect();
        pending.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(pending.into_iter().next())
    }

    /// 更新任务状态。
    ///
    /// 任务不存在时返回 `NotFound` 错误。
    /// 仅更新 `status` 与时间戳，不影响其他字段。
    pub fn update_status(
        &self,
        task_id: &str,
        status: TaskStatus,
    ) -> Result<(), TaskQueueError> {
        let mut file = self.load()?;
        let task = file
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| TaskQueueError::NotFound(task_id.to_string()))?;
        // 根据终态/运行态调用对应的 mark 方法，保持时间戳一致性
        match status {
            TaskStatus::Running => task.mark_running(),
            TaskStatus::Completed => task.mark_completed(),
            TaskStatus::Failed => {
                // 保留原 error 信息，仅置位状态
                let err = task.error.clone().unwrap_or_default();
                task.mark_failed(err);
            }
            TaskStatus::Cancelled => task.mark_cancelled(),
            TaskStatus::Pending => task.reset_to_pending(),
        }
        self.save(&file)
    }

    /// 更新任务状态并设置错误信息（Failed 用）。
    pub fn update_status_with_error(
        &self,
        task_id: &str,
        status: TaskStatus,
        error: &str,
    ) -> Result<(), TaskQueueError> {
        let mut file = self.load()?;
        let task = file
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| TaskQueueError::NotFound(task_id.to_string()))?;
        match status {
            TaskStatus::Failed => task.mark_failed(error),
            TaskStatus::Cancelled => {
                task.mark_cancelled();
                task.error = Some(error.to_string());
            }
            _ => {
                return Err(TaskQueueError::InvalidState(format!(
                    "update_status_with_error 仅支持 Failed / Cancelled，传入 {status:?}"
                )));
            }
        }
        self.save(&file)
    }

    /// 更新任务 checkpoint（用于执行中持久化进度）。
    pub fn update_checkpoint(
        &self,
        task_id: &str,
        checkpoint: &serde_json::Value,
    ) -> Result<(), TaskQueueError> {
        let mut file = self.load()?;
        let task = file
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| TaskQueueError::NotFound(task_id.to_string()))?;
        task.update_checkpoint(checkpoint.clone());
        self.save(&file)
    }

    /// 删除任务记录。
    ///
    /// Running 状态的任务不允许删除（需先取消）。
    pub fn delete(&self, task_id: &str) -> Result<(), TaskQueueError> {
        let mut file = self.load()?;
        let task = file
            .tasks
            .iter()
            .find(|t| t.id == task_id)
            .ok_or_else(|| TaskQueueError::NotFound(task_id.to_string()))?;
        if task.status == TaskStatus::Running {
            return Err(TaskQueueError::InvalidState(format!(
                "任务 {task_id} 处于 Running 状态，无法删除（请先取消）"
            )));
        }
        file.tasks.retain(|t| t.id != task_id);
        self.save(&file)
    }

    /// 清除项目下所有已完成 / 失败 / 取消的任务。
    ///
    /// 返回清除的任务数。
    pub fn clear_finished(&self) -> Result<usize, TaskQueueError> {
        let mut file = self.load()?;
        let before = file.tasks.len();
        file.tasks.retain(|t| !t.status.is_terminal());
        let removed = before - file.tasks.len();
        self.save(&file)?;
        Ok(removed)
    }

    /// 将所有 Running 任务重置为 Pending（用于启动恢复）。
    ///
    /// 返回被重置的任务 ID 列表。
    pub fn reset_running_to_pending(&self) -> Result<Vec<String>, TaskQueueError> {
        let mut file = self.load()?;
        let mut reset_ids = Vec::new();
        for task in file.tasks.iter_mut() {
            if task.status == TaskStatus::Running {
                task.reset_to_pending();
                reset_ids.push(task.id.clone());
            }
        }
        if !reset_ids.is_empty() {
            self.save(&file)?;
        }
        Ok(reset_ids)
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge_builder::types::KnowledgeBuildOptions;
    use crate::llm_config::model::SceneModelRef;

    fn sample_kind() -> TaskKind {
        TaskKind::KnowledgeBuild {
            ref_id: "ref-abc".into(),
            model_ref: SceneModelRef {
                provider_id: "deepseek".into(),
                model_id: "deepseek-chat".into(),
            },
            options: KnowledgeBuildOptions::default(),
        }
    }

    fn temp_store() -> (tempfile::TempDir, TaskStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = TaskStore::new(dir.path());
        (dir, store)
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let (_dir, store) = temp_store();
        let file = store.load().unwrap();
        assert!(file.tasks.is_empty());
    }

    #[test]
    fn enqueue_and_find() {
        let (_dir, store) = temp_store();
        let record = store.enqueue(sample_kind()).unwrap();
        assert!(record.id.starts_with("task-"));

        let found = store.find(&record.id).unwrap();
        assert_eq!(found.id, record.id);
        assert_eq!(found.status, TaskStatus::Pending);
    }

    #[test]
    fn list_returns_in_descending_created_order() {
        let (_dir, store) = temp_store();
        let r1 = store.enqueue(sample_kind()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let r2 = store.enqueue(sample_kind()).unwrap();

        let list = store.list().unwrap();
        assert_eq!(list.len(), 2);
        // 倒序：r2 在前
        assert_eq!(list[0].id, r2.id);
        assert_eq!(list[1].id, r1.id);
    }

    #[test]
    fn next_pending_returns_earliest() {
        let (_dir, store) = temp_store();
        let r1 = store.enqueue(sample_kind()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let r2 = store.enqueue(sample_kind()).unwrap();

        let next = store.next_pending().unwrap().unwrap();
        assert_eq!(next.id, r1.id);

        store.update_status(&r1.id, TaskStatus::Completed).unwrap();
        let next = store.next_pending().unwrap().unwrap();
        assert_eq!(next.id, r2.id);
    }

    #[test]
    fn update_status_running_then_completed() {
        let (_dir, store) = temp_store();
        let record = store.enqueue(sample_kind()).unwrap();

        store.update_status(&record.id, TaskStatus::Running).unwrap();
        let updated = store.find(&record.id).unwrap();
        assert_eq!(updated.status, TaskStatus::Running);
        assert!(updated.started_at.is_some());

        store
            .update_status(&record.id, TaskStatus::Completed)
            .unwrap();
        let updated = store.find(&record.id).unwrap();
        assert_eq!(updated.status, TaskStatus::Completed);
        assert!(updated.finished_at.is_some());
    }

    #[test]
    fn update_status_with_error_failed() {
        let (_dir, store) = temp_store();
        let record = store.enqueue(sample_kind()).unwrap();
        store
            .update_status_with_error(&record.id, TaskStatus::Failed, "LLM timeout")
            .unwrap();
        let updated = store.find(&record.id).unwrap();
        assert_eq!(updated.status, TaskStatus::Failed);
        assert_eq!(updated.error.as_deref(), Some("LLM timeout"));
    }

    #[test]
    fn update_checkpoint_persists() {
        let (_dir, store) = temp_store();
        let record = store.enqueue(sample_kind()).unwrap();
        let ckpt = serde_json::json!({"stage": "planning"});
        store
            .update_checkpoint(&record.id, &ckpt)
            .unwrap();
        let updated = store.find(&record.id).unwrap();
        assert_eq!(updated.checkpoint["stage"], "planning");
    }

    #[test]
    fn delete_running_task_fails() {
        let (_dir, store) = temp_store();
        let record = store.enqueue(sample_kind()).unwrap();
        store.update_status(&record.id, TaskStatus::Running).unwrap();
        let err = store.delete(&record.id).unwrap_err();
        assert!(matches!(err, TaskQueueError::InvalidState(_)));
    }

    #[test]
    fn delete_completed_task_succeeds() {
        let (_dir, store) = temp_store();
        let record = store.enqueue(sample_kind()).unwrap();
        store
            .update_status(&record.id, TaskStatus::Completed)
            .unwrap();
        store.delete(&record.id).unwrap();
        assert!(store.find(&record.id).is_err());
    }

    #[test]
    fn clear_finished_removes_terminal() {
        let (_dir, store) = temp_store();
        let r1 = store.enqueue(sample_kind()).unwrap();
        let r2 = store.enqueue(sample_kind()).unwrap();
        let r3 = store.enqueue(sample_kind()).unwrap();
        store.update_status(&r1.id, TaskStatus::Completed).unwrap();
        store.update_status_with_error(&r2.id, TaskStatus::Failed, "err").unwrap();
        // r3 保持 Pending
        let removed = store.clear_finished().unwrap();
        assert_eq!(removed, 2);
        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, r3.id);
    }

    #[test]
    fn reset_running_to_pending() {
        let (_dir, store) = temp_store();
        let r1 = store.enqueue(sample_kind()).unwrap();
        let r2 = store.enqueue(sample_kind()).unwrap();
        store.update_status(&r1.id, TaskStatus::Running).unwrap();
        store.update_status(&r2.id, TaskStatus::Pending).unwrap();

        let reset_ids = store.reset_running_to_pending().unwrap();
        assert_eq!(reset_ids, vec![r1.id.clone()]);
        let updated = store.find(&r1.id).unwrap();
        assert_eq!(updated.status, TaskStatus::Pending);
    }

    #[test]
    fn list_with_status_filter() {
        let (_dir, store) = temp_store();
        let r1 = store.enqueue(sample_kind()).unwrap();
        let r2 = store.enqueue(sample_kind()).unwrap();
        store.update_status(&r1.id, TaskStatus::Completed).unwrap();
        store.update_status(&r2.id, TaskStatus::Failed).unwrap();

        let failed = store.list_with_status(&[TaskStatus::Failed]).unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].id, r2.id);
    }
}
