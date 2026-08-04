//! Tauri commands——文献导入与管理。
//!
//! ## 命令一览
//!
//! | 命令 | 返回 | 说明 |
//! |------|------|------|
//! | `import_reference` | `ReferenceEntry` | 单文件导入（await） |
//! | `import_references` | `ImportJobHandle` | 批量导入（fire-and-forget） |
//! | `get_import_status` | `ImportJobStatus` | 查询批量任务状态 |
//! | `cancel_import` | `()` | 取消导入任务 |
//! | `cancel_all_imports` | `()` | 取消所有导入 |
//! | `list_references` | `Vec<ReferenceEntry>` | 列出全部文献 |
//! | `get_reference` | `ReferenceEntry` | 获取详情 |
//! | `delete_reference` | `()` | 删除（含 raw/md/resource） |
//! | `retry_import` | `ReferenceEntry` | 重试失败导入 |
//! | `update_reference_title` | `ReferenceEntry` | 修改标题 |
//! | `check_references_consistency` | `ConsistencyReport` | 一致性校验 |
//!
//! ## 事件
//!
//! | 事件 | Payload | 说明 |
//! |------|---------|------|
//! | `reference:import_started` | `{ job_id?, reference_id, filename }` | 开始导入 |
//! | `reference:import_progress` | `{ job_id?, reference_id, stage, ocr_progress? }` | 进度 |
//! | `reference:import_completed` | `{ job_id?, reference_id, entry }` | 完成 |
//! | `reference:import_failed` | `{ job_id?, reference_id, error }` | 失败 |
//! | `reference:job_completed` | `{ job_id, summary }` | 批量任务完成 |

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{Emitter, State, Window};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::ai_services::storage::ConfigStorage as AiServicesConfigStorage;

use super::consistency;
use super::error::ReferenceError;
use super::importer::{ImportProgress, ReferenceImporter};
use super::model::{
    ConsistencyReport, ImportJobHandle, ImportJobStatus, ReferenceEntry, ReferenceStatus,
};
use super::storage::ReferenceIndex;

// ===========================================================================
// 事件常量
// ===========================================================================

pub const EVENT_IMPORT_STARTED: &str = "reference:import_started";
pub const EVENT_IMPORT_PROGRESS: &str = "reference:import_progress";
pub const EVENT_IMPORT_COMPLETED: &str = "reference:import_completed";
pub const EVENT_IMPORT_FAILED: &str = "reference:import_failed";
pub const EVENT_JOB_COMPLETED: &str = "reference:job_completed";

// ===========================================================================
// 导入状态管理
// ===========================================================================

/// 单个导入任务句柄（内部使用）。
struct JobHandle {
    cancel_token: CancellationToken,
}

/// 导入任务状态管理——维护活跃的取消令牌。
pub struct ImportState {
    /// job_id → JobHandle
    jobs: Mutex<HashMap<String, JobHandle>>,
}

impl ImportState {
    pub fn new() -> Self {
        Self {
            jobs: Mutex::new(HashMap::new()),
        }
    }

    fn register(&self, job_id: String) -> CancellationToken {
        let token = CancellationToken::new();
        self.jobs.lock().unwrap().insert(job_id, JobHandle {
            cancel_token: token.clone(),
        });
        token
    }

    fn unregister(&self, job_id: &str) {
        self.jobs.lock().unwrap().remove(job_id);
    }

    fn cancel(&self, job_id: &str) -> bool {
        if let Some(handle) = self.jobs.lock().unwrap().remove(job_id) {
            handle.cancel_token.cancel();
            return true;
        }
        false
    }

    fn cancel_all(&self) -> usize {
        let mut map = self.jobs.lock().unwrap();
        let count = map.len();
        for (_, handle) in map.drain() {
            handle.cancel_token.cancel();
        }
        count
    }
}

impl Default for ImportState {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// 事件 payload
// ===========================================================================

#[derive(Serialize)]
struct ImportStartedPayload<'a> {
    job_id: Option<&'a str>,
    reference_id: &'a str,
    filename: &'a str,
}

#[derive(Serialize)]
struct ImportProgressPayload<'a> {
    job_id: Option<&'a str>,
    reference_id: &'a str,
    stage: &'a str,
    ocr_progress: Option<&'a crate::ai_services::paddleocr::OcrProgress>,
}

#[derive(Serialize)]
struct ImportCompletedPayload<'a> {
    job_id: Option<&'a str>,
    reference_id: &'a str,
    entry: &'a ReferenceEntry,
}

#[derive(Serialize)]
struct ImportFailedPayload<'a> {
    job_id: Option<&'a str>,
    reference_id: &'a str,
    error: &'a str,
}

#[derive(Serialize)]
struct JobCompletedPayload<'a> {
    job_id: &'a str,
    total: usize,
    completed: usize,
    failed: usize,
}

// ===========================================================================
// 单文件导入
// ===========================================================================

/// 导入单个文献（await 语义）。
///
/// 进度通过 [`EVENT_IMPORT_PROGRESS`] 事件推送。
#[tauri::command]
pub async fn import_reference(
    file_path: String,
    project_path: String,
    force: bool,
    window: Window,
    ai_storage: State<'_, AiServicesConfigStorage>,
    import_state: State<'_, ImportState>,
) -> Result<ReferenceEntry, ReferenceError> {
    let project_dir = PathBuf::from(&project_path);
    let index = ReferenceIndex::new(&project_dir);

    let ai_config = ai_storage.get().map_err(|e| ReferenceError::Other(e.to_string()))?;
    let importer = ReferenceImporter::new(&ai_config)?;

    // 单文件导入使用临时 job_id（用于事件关联，不注册到 ImportState）
    let job_id = Uuid::new_v4().to_string();
    let reference_id = ReferenceEntry::generate_id();
    let cancel_token = import_state.register(job_id.clone());

    let filename = Path::new(&file_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".into());

    // 推送 started 事件
    let _ = window.emit(
        EVENT_IMPORT_STARTED,
        &ImportStartedPayload {
            job_id: Some(&job_id),
            reference_id: &reference_id,
            filename: &filename,
        },
    );

    let window_ref = window.clone();
    let job_id_for_closure = job_id.clone();
    let ref_id_for_closure = reference_id.clone();
    let result = importer
        .import_single(
            &file_path,
            &project_dir,
            &index,
            &reference_id,
            force,
            move |progress: ImportProgress| {
                let _ = window_ref.emit(
                    EVENT_IMPORT_PROGRESS,
                    &ImportProgressPayload {
                        job_id: Some(&job_id_for_closure),
                        reference_id: &ref_id_for_closure,
                        stage: &progress.stage,
                        ocr_progress: progress.ocr_progress.as_ref(),
                    },
                );
            },
            &cancel_token,
        )
        .await;

    import_state.unregister(&job_id);

    match result {
        Ok(entry) => {
            let _ = window.emit(
                EVENT_IMPORT_COMPLETED,
                &ImportCompletedPayload {
                    job_id: Some(&job_id),
                    reference_id: &entry.id,
                    entry: &entry,
                },
            );
            Ok(entry)
        }
        Err(e) => {
            let err_str = e.to_string();
            let _ = window.emit(
                EVENT_IMPORT_FAILED,
                &ImportFailedPayload {
                    job_id: Some(&job_id),
                    reference_id: &reference_id,
                    error: &err_str,
                },
            );
            Err(e)
        }
    }
}

// ===========================================================================
// 批量导入（fire-and-forget）
// ===========================================================================

/// 批量导入——立即返回任务句柄，后台串行处理。
///
/// 进度和结果通过事件推送。
#[tauri::command]
pub async fn import_references(
    file_paths: Vec<String>,
    project_path: String,
    window: Window,
    ai_storage: State<'_, AiServicesConfigStorage>,
    import_state: State<'_, ImportState>,
) -> Result<ImportJobHandle, ReferenceError> {
    if file_paths.is_empty() {
        return Err(ReferenceError::Other("文件列表为空".into()));
    }

    let project_dir = PathBuf::from(&project_path);
    let index = ReferenceIndex::new(&project_dir);

    let ai_config = ai_storage.get().map_err(|e| ReferenceError::Other(e.to_string()))?;
    let importer = ReferenceImporter::new(&ai_config)?;

    let job_id = Uuid::new_v4().to_string();
    let reference_ids: Vec<String> = file_paths
        .iter()
        .map(|_| ReferenceEntry::generate_id())
        .collect();

    let cancel_token = import_state.register(job_id.clone());

    let handle = ImportJobHandle {
        job_id: job_id.clone(),
        reference_ids: reference_ids.clone(),
    };

    // 后台串行处理
    let window_clone = window.clone();
    let import_state_job_id = job_id.clone();
    tokio::spawn(async move {
        let total = file_paths.len();
        let mut completed = 0usize;
        let mut failed = 0usize;
        let mut results = Vec::new();

        for (i, file_path) in file_paths.iter().enumerate() {
            if cancel_token.is_cancelled() {
                break;
            }

            let reference_id = &reference_ids[i];
            let filename = Path::new(file_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".into());

            let _ = window_clone.emit(
                EVENT_IMPORT_STARTED,
                &ImportStartedPayload {
                    job_id: Some(&import_state_job_id),
                    reference_id,
                    filename: &filename,
                },
            );

            let window_inner = window_clone.clone();
            let job_id_inner = import_state_job_id.clone();
            let ref_id_inner = reference_id.clone();

            let result = importer
                .import_single(
                    file_path,
                    &project_dir,
                    &index,
                    reference_id,
                    false,
                    move |progress: ImportProgress| {
                        let _ = window_inner.emit(
                            EVENT_IMPORT_PROGRESS,
                            &ImportProgressPayload {
                                job_id: Some(&job_id_inner),
                                reference_id: &ref_id_inner,
                                stage: &progress.stage,
                                ocr_progress: progress.ocr_progress.as_ref(),
                            },
                        );
                    },
                    &cancel_token,
                )
                .await;

            match result {
                Ok(entry) => {
                    completed += 1;
                    results.push(entry.clone());
                    let _ = window_clone.emit(
                        EVENT_IMPORT_COMPLETED,
                        &ImportCompletedPayload {
                            job_id: Some(&import_state_job_id),
                            reference_id: &entry.id,
                            entry: &entry,
                        },
                    );
                }
                Err(ReferenceError::Cancelled) => {
                    failed += 1;
                    let _ = window_clone.emit(
                        EVENT_IMPORT_FAILED,
                        &ImportFailedPayload {
                            job_id: Some(&import_state_job_id),
                            reference_id,
                            error: "已取消",
                        },
                    );
                    break;
                }
                Err(e) => {
                    failed += 1;
                    let err_str = e.to_string();
                    let _ = window_clone.emit(
                        EVENT_IMPORT_FAILED,
                        &ImportFailedPayload {
                            job_id: Some(&import_state_job_id),
                            reference_id,
                            error: &err_str,
                        },
                    );
                }
            }
        }

        let _ = window_clone.emit(
            EVENT_JOB_COMPLETED,
            &JobCompletedPayload {
                job_id: &import_state_job_id,
                total,
                completed,
                failed,
            },
        );
    });

    Ok(handle)
}

// ===========================================================================
// 导入任务管理
// ===========================================================================

/// 查询批量导入任务状态。
///
/// 由于后台任务状态由事件流维护，此命令提供轻量查询能力。
#[tauri::command]
pub fn get_import_status(
    _project_path: String,
    job_id: String,
    import_state: State<'_, ImportState>,
) -> Result<ImportJobStatus, ReferenceError> {
    let exists = import_state.jobs.lock().unwrap().contains_key(&job_id);
    if exists {
        Ok(ImportJobStatus::Running {
            total: 0,
            completed: 0,
            failed: 0,
            current: None,
        })
    } else {
        Ok(ImportJobStatus::Finished {
            total: 0,
            completed: 0,
            failed: 0,
            results: Vec::new(),
        })
    }
}

/// 取消指定导入任务。
#[tauri::command]
pub fn cancel_import(
    _project_path: String,
    job_id: String,
    import_state: State<'_, ImportState>,
) -> Result<(), ReferenceError> {
    import_state.cancel(&job_id);
    Ok(())
}

/// 取消所有进行中的导入。
#[tauri::command]
pub fn cancel_all_imports(
    _project_path: String,
    import_state: State<'_, ImportState>,
) -> Result<usize, ReferenceError> {
    Ok(import_state.cancel_all())
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

/// 重试失败的导入。
#[tauri::command]
pub async fn retry_import(
    project_path: String,
    reference_id: String,
    window: Window,
    ai_storage: State<'_, AiServicesConfigStorage>,
    import_state: State<'_, ImportState>,
) -> Result<ReferenceEntry, ReferenceError> {
    let project_dir = PathBuf::from(&project_path);
    let index = ReferenceIndex::new(&project_dir);

    let existing = index
        .find(&reference_id)?
        .ok_or_else(|| ReferenceError::NotFound(reference_id.clone()))?;

    // 重置状态为 Pending
    index.update(|entries| {
        if let Some(e) = entries.iter_mut().find(|e| e.id == reference_id) {
            e.status = ReferenceStatus::Pending;
            e.error = None;
        }
        Ok(())
    })?;

    let ai_config = ai_storage.get().map_err(|e| ReferenceError::Other(e.to_string()))?;
    let importer = ReferenceImporter::new(&ai_config)?;

    let job_id = Uuid::new_v4().to_string();
    let cancel_token = import_state.register(job_id.clone());

    let raw_abs = project_dir.join(&existing.file_path);

    let window_ref = window.clone();
    let job_id_for_closure = job_id.clone();
    let ref_id_for_closure = reference_id.clone();
    let result = importer
        .import_single(
            raw_abs.to_str().unwrap(),
            &project_dir,
            &index,
            &reference_id,
            true, // force=true 跳过去重（已存在）
            move |progress: ImportProgress| {
                let _ = window_ref.emit(
                    EVENT_IMPORT_PROGRESS,
                    &ImportProgressPayload {
                        job_id: Some(&job_id_for_closure),
                        reference_id: &ref_id_for_closure,
                        stage: &progress.stage,
                        ocr_progress: progress.ocr_progress.as_ref(),
                    },
                );
            },
            &cancel_token,
        )
        .await;

    import_state.unregister(&job_id);

    match result {
        Ok(entry) => {
            let _ = window.emit(
                EVENT_IMPORT_COMPLETED,
                &ImportCompletedPayload {
                    job_id: Some(&job_id),
                    reference_id: &entry.id,
                    entry: &entry,
                },
            );
            Ok(entry)
        }
        Err(e) => {
            let err_str = e.to_string();
            let _ = window.emit(
                EVENT_IMPORT_FAILED,
                &ImportFailedPayload {
                    job_id: Some(&job_id),
                    reference_id: &reference_id,
                    error: &err_str,
                },
            );
            Err(e)
        }
    }
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
