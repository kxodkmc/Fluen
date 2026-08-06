//! Tauri commands——任务队列的前端调用接口。
//!
//! 通过 `@tauri-apps/api` 的 `invoke` 函数调用：
//!
//! ```typescript
//! // 入队任务
//! await invoke('task_queue_enqueue', {
//!   project_path: '/path/to/project',
//!   kind: { kind: 'knowledge_build', ref_id: 'ref-abc', model_ref: {...}, options: {...} }
//! });
//!
//! // 列出项目下所有任务
//! await invoke('task_queue_list', { project_path: '/path/to/project' });
//!
//! // 取消任务
//! await invoke('task_queue_cancel', { task_id: 'task-xxx' });
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::ai_services::storage::ConfigStorage as AiServicesConfigStorage;
use crate::llm_config::storage::ConfigStorage as LlmConfigStorage;

use super::error::TaskQueueError;
use super::state::TaskQueueState;
use super::store::TaskStore;
use super::types::{TaskKind, TaskRecord, TaskStatus};

/// 入队任务。
///
/// 创建 Pending 状态的 TaskRecord 并持久化到 `{project}/data/task-queue.json`。
/// 入队后自动触发该项目的 runner（若未运行）。
#[tauri::command]
pub async fn task_queue_enqueue(
    project_path: String,
    kind: TaskKind,
    state: State<'_, TaskQueueState>,
    llm_storage: State<'_, LlmConfigStorage>,
    ai_storage: State<'_, AiServicesConfigStorage>,
    app: AppHandle,
) -> Result<TaskRecord, TaskQueueError> {
    let project_path_buf = PathBuf::from(&project_path);
    let store = Arc::new(TaskStore::new(&project_path_buf));

    // 入队
    let record = store.enqueue(kind)?;

    // 触发对应种类的 runner（若该项目的该种类 runner 已活跃，则不重复启动；
    // 当前任务会在现有 runner 的循环中被拾取）
    state.try_start_runner(
        project_path_buf,
        record.kind.kind_name(),
        store,
        Arc::new(llm_storage.inner().clone()),
        Arc::new(ai_storage.inner().clone()),
        app,
    );

    Ok(record)
}

/// 查询项目下所有任务（按创建时间倒序）。
///
/// `status_filter` 为 `None` 时返回全部任务；
/// 为 `Some(vec)` 时仅返回状态在 vec 中的任务。
#[tauri::command]
pub async fn task_queue_list(
    project_path: String,
    status_filter: Option<Vec<TaskStatus>>,
) -> Result<Vec<TaskRecord>, TaskQueueError> {
    let store = TaskStore::new(&project_path);
    match status_filter {
        Some(statuses) if !statuses.is_empty() => store.list_with_status(&statuses),
        _ => store.list(),
    }
}

/// 取消任务。
///
/// 若任务正在执行，调用对应 CancellationToken 触发取消（runner 检测后
/// 自行更新状态并推送事件）；若任务未在执行（Pending），直接标记为
/// Cancelled。
///
/// 已终态（Completed/Failed/Cancelled）的任务返回 `InvalidState` 错误。
///
/// 先尝试取消令牌再读状态，避免"读状态 → 决策"窗口内 runner 抢先
/// 置 Running 导致的 TOCTOU（取消令牌存在即视为执行中）。
#[tauri::command]
pub async fn task_queue_cancel(
    task_id: String,
    project_path: String,
    state: State<'_, TaskQueueState>,
) -> Result<(), TaskQueueError> {
    let store = TaskStore::new(&project_path);

    // 执行中的任务：取消令牌生效，runner 负责更新终态
    if state.cancel_task(&task_id) {
        return Ok(());
    }

    // 未在执行（Pending，或终态后令牌已移除）：读状态处理
    let task = store.find(&task_id)?;
    if task.status.is_terminal() {
        return Err(TaskQueueError::InvalidState(format!(
            "任务 {task_id} 已是终态 {:?}，无法取消",
            task.status
        )));
    }
    store.update_status(&task_id, TaskStatus::Cancelled)?;

    Ok(())
}

/// 重试失败任务。
///
/// 清除 error 信息，重置为 Pending，保留 checkpoint。
/// 触发该项目的 runner 继续执行。
#[tauri::command]
pub async fn task_queue_retry(
    task_id: String,
    project_path: String,
    state: State<'_, TaskQueueState>,
    llm_storage: State<'_, LlmConfigStorage>,
    ai_storage: State<'_, AiServicesConfigStorage>,
    app: AppHandle,
) -> Result<(), TaskQueueError> {
    let project_path_buf = PathBuf::from(&project_path);
    let store = Arc::new(TaskStore::new(&project_path_buf));

    let kind_name = {
        let s = TaskStore::new(&project_path_buf);
        let task = s.find(&task_id)?;
        if !task.status.can_resume() {
            return Err(TaskQueueError::InvalidState(format!(
                "任务 {task_id} 状态 {:?} 不支持重试（仅 Failed/Cancelled 可重试）",
                task.status
            )));
        }
        s.update_status(&task_id, TaskStatus::Pending)?;
        task.kind.kind_name()
    };

    // 触发对应种类的 runner
    state.try_start_runner(
        project_path_buf,
        kind_name,
        store,
        Arc::new(llm_storage.inner().clone()),
        Arc::new(ai_storage.inner().clone()),
        app,
    );

    Ok(())
}

/// 继续已取消任务。
///
/// 与 retry 的区别：retry 用于 Failed 任务，resume 用于 Cancelled 任务。
/// 实现上相同：重置为 Pending，保留 checkpoint，触发 runner。
#[tauri::command]
pub async fn task_queue_resume(
    task_id: String,
    project_path: String,
    state: State<'_, TaskQueueState>,
    llm_storage: State<'_, LlmConfigStorage>,
    ai_storage: State<'_, AiServicesConfigStorage>,
    app: AppHandle,
) -> Result<(), TaskQueueError> {
    task_queue_retry(task_id, project_path, state, llm_storage, ai_storage, app).await
}

/// 删除任务记录。
///
/// Running 状态的任务不允许删除（需先取消）。
#[tauri::command]
pub async fn task_queue_delete(
    task_id: String,
    project_path: String,
) -> Result<(), TaskQueueError> {
    let store = TaskStore::new(&project_path);
    store.delete(&task_id)
}

/// 清除项目下所有已完成 / 失败 / 取消的任务。
///
/// 返回清除的任务数。
#[tauri::command]
pub async fn task_queue_clear_finished(
    project_path: String,
) -> Result<usize, TaskQueueError> {
    let store = TaskStore::new(&project_path);
    store.clear_finished()
}
