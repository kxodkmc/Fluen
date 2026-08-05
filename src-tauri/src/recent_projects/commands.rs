//! Tauri commands——供前端调用的最近打开项目接口。
//!
//! 通过 `@tauri-apps/api` 的 `invoke` 函数调用：
//!
//! ```typescript
//! // 获取最近项目列表
//! const data = await invoke<RecentProjectsData>('recent_projects_list');
//! // 记录一次打开
//! await invoke('recent_projects_record', { entry: {...} });
//! // 移除一条
//! await invoke('recent_projects_remove', { projectPath: '/path' });
//! // 裁剪到指定数量
//! await invoke('recent_projects_trim', { maxCount: 4 });
//! ```

use tauri::State;

use super::error::RecentProjectsError;
use super::model::{RecentProjectEntry, RecentProjectsData};
use super::storage::{RecentProjectsStorage, DEFAULT_MAX_COUNT};
use crate::app_config::storage::AppConfigStorage;

/// 读取最近打开项目列表（优先从内存缓存返回）。
///
/// 文件不存在时返回默认数据（空列表）。
/// `max_count` 用于在返回前按当前设置裁剪列表，避免展示超出设置数量的条目。
#[tauri::command]
pub fn recent_projects_list(
    storage: State<'_, RecentProjectsStorage>,
    app_storage: State<'_, AppConfigStorage>,
) -> Result<RecentProjectsData, String> {
    let mut data = storage.get().map_err(|e| e.to_string())?;
    let max_count = app_storage
        .get()
        .map(|c| c.recent_projects_count as usize)
        .unwrap_or(DEFAULT_MAX_COUNT);
    data.trim(max_count);
    Ok(data)
}

/// 记录一次打开（更新或插入）。
///
/// - 若 `project_path` 已存在，移除旧记录
/// - 将新记录插入到列表头部
/// - 按 `AppConfig.recent_projects_count` 裁剪列表
#[tauri::command]
pub fn recent_projects_record(
    storage: State<'_, RecentProjectsStorage>,
    app_storage: State<'_, AppConfigStorage>,
    entry: RecentProjectEntry,
) -> Result<(), String> {
    entry.validate().map_err(|e: RecentProjectsError| e.to_string())?;

    let mut data = storage.get().map_err(|e| e.to_string())?;
    let max_count = app_storage
        .get()
        .map(|c| c.recent_projects_count as usize)
        .unwrap_or(DEFAULT_MAX_COUNT);
    data.upsert(entry, max_count);
    storage.save(&data).map_err(|e| e.to_string())
}

/// 移除一条记录（路径失效或用户主动移除时调用）。
#[tauri::command]
pub fn recent_projects_remove(
    storage: State<'_, RecentProjectsStorage>,
    project_path: String,
) -> Result<(), String> {
    let mut data = storage.get().map_err(|e| e.to_string())?;
    data.remove(&project_path);
    storage.save(&data).map_err(|e| e.to_string())
}

/// 按最大数量裁剪列表。
///
/// 用户在设置中调小 `recent_projects_count` 后调用，确保存储与展示一致。
#[tauri::command]
pub fn recent_projects_trim(
    storage: State<'_, RecentProjectsStorage>,
    max_count: usize,
) -> Result<(), String> {
    let mut data = storage.get().map_err(|e| e.to_string())?;
    data.trim(max_count);
    storage.save(&data).map_err(|e| e.to_string())
}
