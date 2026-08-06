//! Tauri commands——文献管理与导入入队。
//!
//! ## 命令一览
//!
//! | 命令 | 返回 | 说明 |
//! |------|------|------|
//! | `references_enqueue_imports` | `Vec<TaskRecord>` | 批量入队文献导入任务 |
//! | `list_references` | `Vec<ReferenceEntry>` | 列出全部文献 |
//! | `get_reference` | `ReferenceEntry` | 获取详情 |
//! | `delete_reference` | `()` | 删除（含 raw/md/resource） |
//! | `retry_import` | `TaskRecord` | 重试失败导入（入队） |
//! | `update_reference_title` | `ReferenceEntry` | 修改标题 |
//! | `check_references_consistency` | `ConsistencyReport` | 一致性校验 |
//!
//! ## 导入队列
//!
//! 文献导入由 [`crate::task_queue`] 统一调度：每个文件入队一个
//! `TaskKind::ReferenceImport` 任务，同一项目内严格串行（FIFO）。
//! 进度与结果通过 `reference:*` 事件推送（见 [`super::events`]），
//! 任务生命周期（取消 / 重试 / 查询）由 `task_queue_*` 命令提供。
//!
//! 崩溃恢复：App 重启后由 `recover_on_startup` 将 Running 任务重置为
//! Pending 并继续执行；文献导入任务重跑幂等（去重跳过自身条目）。

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::ai_services::storage::ConfigStorage as AiServicesConfigStorage;
use crate::llm_config::storage::ConfigStorage as LlmConfigStorage;
use crate::task_queue::state::TaskQueueState;
use crate::task_queue::store::TaskStore;
use crate::task_queue::types::{TaskKind, TaskRecord};

use super::consistency;
use super::error::ReferenceError;
use super::importer::ReferenceImporter;
use super::model::{
    ConsistencyReport, ReferenceEntry, ReferenceStatus,
};
use super::storage::ReferenceIndex;

/// 日志 scope 标签，与前端 `useLogger('references')` 保持一致。
const LOG_SCOPE: &str = "references";

// ===========================================================================
// 文献导入（队列入队）
// ===========================================================================

/// 批量入队文献导入任务。
///
/// 每个文件生成独立的 `TaskKind::ReferenceImport` 任务并入队，
/// 由 [`TaskQueueState`] 按 FIFO 顺序串行执行（先到先导）。
/// 立即返回全部任务记录，执行进度与结果通过 `reference:*` 事件推送。
///
/// 入队前会预校验 OCR 配置可用性，配置缺失时立即报错（快速反馈），
/// 避免任务排队后才发现无法执行。
#[tauri::command]
pub async fn references_enqueue_imports(
    project_path: String,
    file_paths: Vec<String>,
    state: State<'_, TaskQueueState>,
    llm_storage: State<'_, LlmConfigStorage>,
    ai_storage: State<'_, AiServicesConfigStorage>,
    app: AppHandle,
) -> Result<Vec<TaskRecord>, ReferenceError> {
    if file_paths.is_empty() {
        return Err(ReferenceError::Other("文件列表为空".into()));
    }

    let project_dir = PathBuf::from(&project_path);

    // 预校验 OCR 配置（失败立即返回，快速反馈）
    let ai_config = ai_storage.get().inspect_err(|e| {
        tracing::error!(scope = LOG_SCOPE, error = %e, "读取 OCR 配置失败");
    }).map_err(|e| ReferenceError::Other(e.to_string()))?;
    ReferenceImporter::new(&ai_config)?;

    let store = Arc::new(TaskStore::new(&project_dir));

    let mut records = Vec::with_capacity(file_paths.len());
    for file_path in file_paths {
        let kind = TaskKind::ReferenceImport {
            file_path,
            reference_id: ReferenceEntry::generate_id(),
            force: false,
        };
        let record = store
            .enqueue(kind)
            .map_err(|e| ReferenceError::Other(e.to_string()))?;
        tracing::info!(
            scope = LOG_SCOPE,
            task_id = %record.id,
            reference_id = %record.kind.ref_id().unwrap_or_default(),
            project = %project_dir.display(),
            "文献导入任务已入队"
        );
        records.push(record);
    }

    // 触发文献导入种类的 runner（若该项目已有该种类活跃 runner，
    // 新任务会被现有循环拾取；与知识库构建 runner 并行不互斥）
    state.try_start_runner(
        project_dir.clone(),
        "reference_import",
        store,
        Arc::new(llm_storage.inner().clone()),
        Arc::new(ai_storage.inner().clone()),
        app,
    );

    tracing::info!(
        scope = LOG_SCOPE,
        count = records.len(),
        project = %project_dir.display(),
        "文献导入任务批量入队完成"
    );

    Ok(records)
}

// ===========================================================================
// 文献 CRUD
// ===========================================================================

/// 列出项目所有文献。
#[tauri::command]
pub fn list_references(
    project_path: String,
) -> Result<Vec<ReferenceEntry>, ReferenceError> {
    let index = ReferenceIndex::new(&project_path);
    index.list()
}

/// 获取单个文献详情。
#[tauri::command]
pub fn get_reference(
    project_path: String,
    reference_id: String,
) -> Result<ReferenceEntry, ReferenceError> {
    let index = ReferenceIndex::new(&project_path);
    index
        .find(&reference_id)?
        .ok_or_else(|| ReferenceError::NotFound(reference_id))
}

/// 删除文献（同时删除 raw/md/resource 文件）。
#[tauri::command]
pub fn delete_reference(
    project_path: String,
    reference_id: String,
) -> Result<(), ReferenceError> {
    let project_dir = PathBuf::from(&project_path);
    let index = ReferenceIndex::new(&project_dir);

    let entry = index
        .remove(&reference_id)?
        .ok_or_else(|| ReferenceError::NotFound(reference_id.clone()))?;

    // 删除 raw 文件
    let raw_path = project_dir.join(&entry.file_path);
    let _ = std::fs::remove_file(&raw_path);

    // 删除 md 文件
    let md_path = project_dir.join(&entry.md_path);
    let _ = std::fs::remove_file(&md_path);

    // 删除 resource 目录
    let resource_path = project_dir.join(&entry.resource_dir);
    let _ = std::fs::remove_dir_all(&resource_path);

    Ok(())
}

/// 重试失败的导入——将文献重新入队（`force=true` 跳过文件去重）。
///
/// 使用 raw 备份文件作为源，复用原文献 ID 保证索引幂等覆盖。
/// 返回入队的任务记录，执行进度通过 `reference:*` 事件推送。
#[tauri::command]
pub async fn retry_import(
    project_path: String,
    reference_id: String,
    state: State<'_, TaskQueueState>,
    llm_storage: State<'_, LlmConfigStorage>,
    ai_storage: State<'_, AiServicesConfigStorage>,
    app: AppHandle,
) -> Result<TaskRecord, ReferenceError> {
    let project_dir = PathBuf::from(&project_path);
    let index = ReferenceIndex::new(&project_dir);

    let existing = index
        .find(&reference_id)?
        .ok_or_else(|| ReferenceError::NotFound(reference_id.clone()))?;

    // 仅 failed 状态可重试（防御直接 invoke 误用，避免将正常条目重置）
    if existing.status != ReferenceStatus::Failed {
        return Err(ReferenceError::Other(format!(
            "文献状态 {:?} 不支持重试（仅 failed 状态可重试）",
            existing.status
        )));
    }

    // 重置索引状态为 Pending（任务执行时再置 Processing）
    index.update(|entries| {
        if let Some(e) = entries.iter_mut().find(|e| e.id == reference_id) {
            e.status = ReferenceStatus::Pending;
            e.error = None;
        }
        Ok(())
    })?;

    // 预校验 OCR 配置
    let ai_config = ai_storage.get().inspect_err(|e| {
        tracing::error!(scope = LOG_SCOPE, error = %e, "读取 OCR 配置失败");
    }).map_err(|e| ReferenceError::Other(e.to_string()))?;
    ReferenceImporter::new(&ai_config)?;

    let raw_abs = project_dir.join(&existing.file_path);
    let kind = TaskKind::ReferenceImport {
        file_path: raw_abs.to_string_lossy().to_string(),
        reference_id,
        force: true,
    };

    let store = Arc::new(TaskStore::new(&project_dir));
    let record = store
        .enqueue(kind)
        .map_err(|e| ReferenceError::Other(e.to_string()))?;

    tracing::info!(
        scope = LOG_SCOPE,
        task_id = %record.id,
        reference_id = %record.kind.ref_id().unwrap_or_default(),
        project = %project_dir.display(),
        "重试导入任务已入队"
    );

    state.try_start_runner(
        project_dir,
        "reference_import",
        store,
        Arc::new(llm_storage.inner().clone()),
        Arc::new(ai_storage.inner().clone()),
        app,
    );

    Ok(record)
}

/// 更新文献标题（手动修正）。
#[tauri::command]
pub fn update_reference_title(
    project_path: String,
    reference_id: String,
    title: String,
) -> Result<ReferenceEntry, ReferenceError> {
    let index = ReferenceIndex::new(&project_path);
    let mut updated_entry = None;
    index.update(|entries| {
        if let Some(entry) = entries.iter_mut().find(|e| e.id == reference_id) {
            entry.title = title;
            updated_entry = Some(entry.clone());
        }
        Ok(())
    })?;
    updated_entry.ok_or_else(|| ReferenceError::NotFound(reference_id))
}

/// 一致性校验——扫描孤儿文件与缺失条目。
#[tauri::command]
pub fn check_references_consistency(
    project_path: String,
) -> Result<ConsistencyReport, ReferenceError> {
    let project_dir = PathBuf::from(&project_path);
    let index = ReferenceIndex::new(&project_dir);
    consistency::check_and_repair(&project_dir, &index)
}
