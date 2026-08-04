//! 任务队列的核心数据类型。
//!
//! 这些类型描述任务记录、状态、种类以及持久化文件结构。
//! `TaskKind::KnowledgeBuild` 引用 [`crate::knowledge_builder::types::KnowledgeBuildOptions`]，
//! 未来可扩展 `Translation`、`OcrCorrection` 等种类。

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::knowledge_builder::types::KnowledgeBuildOptions;
use crate::llm_config::model::SceneModelRef;

// ---------------------------------------------------------------------------
// 任务状态
// ---------------------------------------------------------------------------

/// 任务状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// 待执行（已入队，等待 worker 拾取）。
    Pending,
    /// 执行中。
    Running,
    /// 已完成。
    Completed,
    /// 执行失败。
    Failed,
    /// 已取消（保留 checkpoint，可继续）。
    Cancelled,
}

impl TaskStatus {
    /// 是否终态（不可继续执行，需重试或继续）。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// 是否可重新入队（Failed / Cancelled）。
    pub fn can_resume(self) -> bool {
        matches!(self, Self::Failed | Self::Cancelled)
    }
}

// ---------------------------------------------------------------------------
// 任务种类
// ---------------------------------------------------------------------------

/// 任务种类（便于未来扩展翻译、纠错等任务）。
///
/// 序列化使用 internally tagged enum（`kind` 字段），便于前端按类型分发。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskKind {
    /// 知识库构建任务。
    KnowledgeBuild {
        /// 文献 ID。
        ref_id: String,
        /// 场景化模型引用（任务级锁定，不随配置变化）。
        model_ref: SceneModelRef,
        /// 构建选项。
        options: KnowledgeBuildOptions,
    },
    // 未来扩展：
    // Translation { ref_id, target_lang, ... }
    // OcrCorrection { ref_id, ... }
}

impl TaskKind {
    /// 获取任务的可读名称（用于日志与 UI 展示）。
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::KnowledgeBuild { .. } => "knowledge_build",
        }
    }

    /// 获取关联的文献 ID（若任务与文献相关）。
    pub fn ref_id(&self) -> Option<&str> {
        match self {
            Self::KnowledgeBuild { ref_id, .. } => Some(ref_id),
        }
    }
}

// ---------------------------------------------------------------------------
// 任务记录
// ---------------------------------------------------------------------------

/// 任务记录（持久化到 `task-queue.json`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    /// 任务 ID（`task-{uuid}`）。
    pub id: String,
    /// 项目绝对路径（多项目隔离）。
    pub project_path: String,
    /// 任务种类与参数。
    pub kind: TaskKind,
    /// 任务状态。
    pub status: TaskStatus,
    /// 断点续传信息（任务特定结构，存储为 JSON Value）。
    /// `serde_json::Value` 的 `Default` 实现返回 `Null`。
    #[serde(default)]
    pub checkpoint: serde_json::Value,
    /// 失败原因（仅 `Failed` 状态有值）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 创建时间（RFC3339）。
    pub created_at: String,
    /// 最后更新时间（RFC3339）。
    pub updated_at: String,
    /// 开始执行时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    /// 完成时间（含 Failed / Cancelled）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
}

impl TaskRecord {
    /// 文件格式版本。
    pub const VERSION: &'static str = "1.0.0";

    /// 创建新的 Pending 任务记录。
    pub fn new_pending(project_path: impl Into<String>, kind: TaskKind) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: format!("task-{}", Uuid::new_v4()),
            project_path: project_path.into(),
            kind,
            status: TaskStatus::Pending,
            checkpoint: serde_json::Value::Null,
            error: None,
            created_at: now.clone(),
            updated_at: now,
            started_at: None,
            finished_at: None,
        }
    }

    /// 标记为 Running，记录开始时间。
    pub fn mark_running(&mut self) {
        let now = Utc::now().to_rfc3339();
        self.status = TaskStatus::Running;
        self.started_at = Some(now.clone());
        self.updated_at = now;
    }

    /// 标记为 Completed，记录完成时间。
    pub fn mark_completed(&mut self) {
        let now = Utc::now().to_rfc3339();
        self.status = TaskStatus::Completed;
        self.finished_at = Some(now.clone());
        self.updated_at = now;
    }

    /// 标记为 Failed，记录错误与完成时间。
    pub fn mark_failed(&mut self, error: impl Into<String>) {
        let now = Utc::now().to_rfc3339();
        self.status = TaskStatus::Failed;
        self.error = Some(error.into());
        self.finished_at = Some(now.clone());
        self.updated_at = now;
    }

    /// 标记为 Cancelled，记录完成时间。
    pub fn mark_cancelled(&mut self) {
        let now = Utc::now().to_rfc3339();
        self.status = TaskStatus::Cancelled;
        self.finished_at = Some(now.clone());
        self.updated_at = now;
    }

    /// 重置为 Pending（用于 retry / resume），保留 checkpoint 与 created_at。
    pub fn reset_to_pending(&mut self) {
        let now = Utc::now().to_rfc3339();
        self.status = TaskStatus::Pending;
        self.error = None;
        self.started_at = None;
        self.finished_at = None;
        self.updated_at = now;
    }

    /// 更新 checkpoint。
    pub fn update_checkpoint(&mut self, checkpoint: serde_json::Value) {
        self.checkpoint = checkpoint;
        self.updated_at = Utc::now().to_rfc3339();
    }
}

// ---------------------------------------------------------------------------
// 持久化文件结构
// ---------------------------------------------------------------------------

/// 持久化文件结构（`task-queue.json`）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskQueueFile {
    /// 文件格式版本。
    #[serde(default = "default_version")]
    pub version: String,
    /// 任务记录列表。
    #[serde(default)]
    pub tasks: Vec<TaskRecord>,
}

fn default_version() -> String {
    TaskRecord::VERSION.to_string()
}

impl TaskQueueFile {
    /// 创建空文件结构。
    pub fn new() -> Self {
        Self {
            version: default_version(),
            tasks: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_kb_kind() -> TaskKind {
        TaskKind::KnowledgeBuild {
            ref_id: "ref-abc".into(),
            model_ref: SceneModelRef {
                provider_id: "deepseek".into(),
                model_id: "deepseek-chat".into(),
            },
            options: KnowledgeBuildOptions::default(),
        }
    }

    #[test]
    fn task_status_terminal() {
        assert!(TaskStatus::Completed.is_terminal());
        assert!(TaskStatus::Failed.is_terminal());
        assert!(TaskStatus::Cancelled.is_terminal());
        assert!(!TaskStatus::Pending.is_terminal());
        assert!(!TaskStatus::Running.is_terminal());
    }

    #[test]
    fn task_status_can_resume() {
        assert!(TaskStatus::Failed.can_resume());
        assert!(TaskStatus::Cancelled.can_resume());
        assert!(!TaskStatus::Completed.can_resume());
        assert!(!TaskStatus::Pending.can_resume());
    }

    #[test]
    fn kind_name_and_ref_id() {
        let kind = sample_kb_kind();
        assert_eq!(kind.kind_name(), "knowledge_build");
        assert_eq!(kind.ref_id(), Some("ref-abc"));
    }

    #[test]
    fn new_pending_has_pending_status() {
        let record = TaskRecord::new_pending("/tmp/project", sample_kb_kind());
        assert!(record.id.starts_with("task-"));
        assert_eq!(record.status, TaskStatus::Pending);
        assert_eq!(record.project_path, "/tmp/project");
        assert!(record.error.is_none());
        assert!(record.started_at.is_none());
        assert!(record.finished_at.is_none());
        assert_eq!(record.created_at, record.updated_at);
    }

    #[test]
    fn mark_running_then_completed() {
        let mut record = TaskRecord::new_pending("/tmp/project", sample_kb_kind());
        record.mark_running();
        assert_eq!(record.status, TaskStatus::Running);
        assert!(record.started_at.is_some());

        record.mark_completed();
        assert_eq!(record.status, TaskStatus::Completed);
        assert!(record.finished_at.is_some());
    }

    #[test]
    fn mark_failed_records_error() {
        let mut record = TaskRecord::new_pending("/tmp/project", sample_kb_kind());
        record.mark_running();
        record.mark_failed("LLM 调用超时");
        assert_eq!(record.status, TaskStatus::Failed);
        assert_eq!(record.error.as_deref(), Some("LLM 调用超时"));
    }

    #[test]
    fn reset_to_pending_preserves_checkpoint() {
        let mut record = TaskRecord::new_pending("/tmp/project", sample_kb_kind());
        record.checkpoint = serde_json::json!({"stage": "planning"});
        record.mark_running();
        record.mark_failed("err");

        record.reset_to_pending();
        assert_eq!(record.status, TaskStatus::Pending);
        assert!(record.error.is_none());
        assert!(record.started_at.is_none());
        assert!(record.finished_at.is_none());
        // checkpoint 保留
        assert_eq!(record.checkpoint["stage"], "planning");
    }

    #[test]
    fn task_queue_file_roundtrip() {
        let mut file = TaskQueueFile::new();
        file.tasks
            .push(TaskRecord::new_pending("/tmp/project", sample_kb_kind()));
        file.tasks
            .push(TaskRecord::new_pending("/tmp/other", sample_kb_kind()));

        let json = serde_json::to_string(&file).unwrap();
        let parsed: TaskQueueFile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, "1.0.0");
        assert_eq!(parsed.tasks.len(), 2);
        assert_eq!(parsed.tasks[0].project_path, "/tmp/project");
    }

    #[test]
    fn task_kind_serde_tagged() {
        let kind = sample_kb_kind();
        let json = serde_json::to_string(&kind).unwrap();
        assert!(json.contains("\"kind\":\"knowledge_build\""));
        let parsed: TaskKind = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.kind_name(), "knowledge_build");
    }

    #[test]
    fn deserialize_old_file_without_version() {
        let json = r#"{"tasks":[]}"#;
        let parsed: TaskQueueFile = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.version, "1.0.0");
        assert!(parsed.tasks.is_empty());
    }
}
