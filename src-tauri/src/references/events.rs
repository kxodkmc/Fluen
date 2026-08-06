//! 文献导入 Tauri 事件常量与 payload 结构。
//!
//! 文献导入作为 `task_queue::TaskKind::ReferenceImport` 任务由
//! [`crate::task_queue::runner::TaskRunner`] 串行执行，本模块的事件
//! 由 runner 在执行任务时通过 `Window::emit` 推送到前端。
//!
//! 事件语义与前端 `useReferences.ts` 中的 `reference:*` 监听一一对应。
//! `job_id` 即任务队列的 `task_id`。

use serde::Serialize;

use crate::ai_services::paddleocr::OcrProgress;

use super::model::ReferenceEntry;

/// 任务开始（单个文件开始导入）。
pub const EVENT_IMPORT_STARTED: &str = "reference:import_started";
/// 任务进度（OCR 页数 / 保存阶段）。
pub const EVENT_IMPORT_PROGRESS: &str = "reference:import_progress";
/// 任务完成（单个文件导入成功）。
pub const EVENT_IMPORT_COMPLETED: &str = "reference:import_completed";
/// 任务失败（单个文件导入失败）。
pub const EVENT_IMPORT_FAILED: &str = "reference:import_failed";

/// 任务开始 payload。
#[derive(Debug, Clone, Serialize)]
pub struct ImportStartedPayload {
    /// 任务 ID（`task-{uuid}`）。
    pub job_id: String,
    /// 文献 ID（`ref-{uuid}`）。
    pub reference_id: String,
    /// 原始文件名。
    pub filename: String,
}

/// 任务进度 payload。
#[derive(Debug, Clone, Serialize)]
pub struct ImportProgressPayload {
    /// 任务 ID（`task-{uuid}`）。
    pub job_id: String,
    /// 文献 ID（`ref-{uuid}`）。
    pub reference_id: String,
    /// 当前阶段：`"ocr"` / `"saving"`。
    pub stage: String,
    /// OCR 进度（仅 `stage == "ocr"` 时有值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ocr_progress: Option<OcrProgress>,
}

/// 任务完成 payload。
#[derive(Debug, Clone, Serialize)]
pub struct ImportCompletedPayload {
    /// 任务 ID（`task-{uuid}`）。
    pub job_id: String,
    /// 文献 ID（`ref-{uuid}`）。
    pub reference_id: String,
    /// 导入完成后的文献条目。
    pub entry: ReferenceEntry,
}

/// 任务失败 payload。
#[derive(Debug, Clone, Serialize)]
pub struct ImportFailedPayload {
    /// 任务 ID（`task-{uuid}`）。
    pub job_id: String,
    /// 文献 ID（`ref-{uuid}`）。
    pub reference_id: String,
    /// 错误消息（明文，供前端展示与日志记录）。
    pub error: String,
}
