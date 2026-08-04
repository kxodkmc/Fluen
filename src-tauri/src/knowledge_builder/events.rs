//! 知识库构建 Tauri 事件常量与 payload 结构。
//!
//! 事件由 `task_queue::runner` 在执行任务时通过 `Window::emit` 推送到前端。

use serde::{Deserialize, Serialize};

/// 任务开始。
pub const EVENT_KB_BUILD_STARTED: &str = "kb-build:started";
/// 任务进度更新。
pub const EVENT_KB_BUILD_PROGRESS: &str = "kb-build:progress";
/// 任务完成。
pub const EVENT_KB_BUILD_COMPLETED: &str = "kb-build:completed";
/// 任务失败。
pub const EVENT_KB_BUILD_FAILED: &str = "kb-build:failed";
/// 任务被取消。
pub const EVENT_KB_BUILD_CANCELLED: &str = "kb-build:cancelled";

/// 任务开始 payload。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KbBuildStartedPayload {
    pub task_id: String,
    pub ref_id: String,
    /// 文献标题（用于前端展示）。
    pub title: Option<String>,
}

/// 任务进度 payload。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KbBuildProgressPayload {
    pub task_id: String,
    pub ref_id: String,
    /// 当前阶段（`BuildStage::as_str`）。
    pub stage: String,
    /// 已创建条目数。
    pub created_count: usize,
    /// 计划创建总数（AI 可能不确定，故为 Option）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_planned: Option<usize>,
    /// 详细信息（如"正在创建概念页：数智化技术"）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// 任务完成 payload。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KbBuildCompletedPayload {
    pub task_id: String,
    pub ref_id: String,
    /// 已创建的 summary 条目 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_id: Option<String>,
    /// 已创建的 concept 条目 ID 列表。
    #[serde(default)]
    pub concept_ids: Vec<String>,
    /// 已创建的 entity 条目 ID 列表。
    #[serde(default)]
    pub entity_ids: Vec<String>,
    /// 关联关系是否已建立。
    pub relations_established: bool,
}

/// 任务失败 payload。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KbBuildFailedPayload {
    pub task_id: String,
    pub ref_id: String,
    pub error: String,
}

/// 任务取消 payload。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KbBuildCancelledPayload {
    pub task_id: String,
    pub ref_id: String,
}
