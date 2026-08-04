//! Tauri commands——供前端调用的 App 配置 CRUD 接口。
//!
//! 通过 `@tauri-apps/api` 的 `invoke` 函数调用：
//!
//! ```typescript
//! // 读取配置
//! const config = await invoke<AppConfig>('get_app_config');
//! // 保存配置
//! await invoke('save_app_config', { config: newConfig });
//! // 获取配置文件路径
//! const path = await invoke<string>('get_app_config_path');
//! ```

use tauri::State;

use super::model::AppConfig;
use super::storage::AppConfigStorage;

/// 读取 App 配置（优先从内存缓存返回）。
///
/// 文件不存在时返回默认配置。
#[tauri::command]
pub fn get_app_config(storage: State<'_, AppConfigStorage>) -> Result<AppConfig, String> {
    storage.get().map_err(|e| e.to_string())
}

/// 保存 App 配置（原子写入 + 更新缓存）。
///
/// 保存前会校验配置完整性，校验失败时返回错误信息。
#[tauri::command]
pub fn save_app_config(
    storage: State<'_, AppConfigStorage>,
    config: AppConfig,
) -> Result<(), String> {
    storage.save(&config).map_err(|e| e.to_string())
}

/// 返回配置文件的完整路径（供前端展示）。
#[tauri::command]
pub fn get_app_config_path(storage: State<'_, AppConfigStorage>) -> Result<String, String> {
    Ok(storage.path().to_string_lossy().to_string())
}
