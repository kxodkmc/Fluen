//! Tauri commands——供前端调用的宠物助手 CRUD 接口。
//!
//! 通过 `@tauri-apps/api` 的 `invoke` 函数调用：
//!
//! ```typescript
//! // 读取配置
//! const config = await invoke<MascotConfig>('get_mascot_config');
//! // 保存配置
//! await invoke('save_mascot_config', { config: newConfig });
//! // 获取配置文件路径
//! const path = await invoke<string>('get_mascot_config_path');
//! // 读取数据
//! const data = await invoke<MascotData>('get_mascot_data');
//! // 保存数据
//! await invoke('save_mascot_data', { data: newData });
//! // 获取数据文件路径
//! const dataPath = await invoke<string>('get_mascot_data_path');
//! ```

use tauri::State;

use super::model::{MascotConfig, MascotData};
use super::storage::{MascotConfigStorage, MascotDataStorage};

/// 读取宠物配置（优先从内存缓存返回）。
///
/// 文件不存在时返回默认配置。
#[tauri::command]
pub fn get_mascot_config(storage: State<'_, MascotConfigStorage>) -> Result<MascotConfig, String> {
    storage.get().map_err(|e| e.to_string())
}

/// 保存宠物配置（原子写入 + 更新缓存）。
///
/// 保存前会校验配置完整性，校验失败时返回错误信息。
#[tauri::command]
pub fn save_mascot_config(
    storage: State<'_, MascotConfigStorage>,
    config: MascotConfig,
) -> Result<(), String> {
    storage.save(&config).map_err(|e| e.to_string())
}

/// 返回配置文件的完整路径（供前端展示）。
#[tauri::command]
pub fn get_mascot_config_path(storage: State<'_, MascotConfigStorage>) -> Result<String, String> {
    Ok(storage.path().to_string_lossy().to_string())
}

/// 读取宠物数据（优先从内存缓存返回）。
///
/// 文件不存在时返回默认数据。
#[tauri::command]
pub fn get_mascot_data(storage: State<'_, MascotDataStorage>) -> Result<MascotData, String> {
    storage.get().map_err(|e| e.to_string())
}

/// 保存宠物数据（原子写入 + 更新缓存）。
///
/// 保存前会校验数据完整性，校验失败时返回错误信息。
#[tauri::command]
pub fn save_mascot_data(
    storage: State<'_, MascotDataStorage>,
    data: MascotData,
) -> Result<(), String> {
    storage.save(&data).map_err(|e| e.to_string())
}

/// 返回数据文件的完整路径（供前端展示）。
#[tauri::command]
pub fn get_mascot_data_path(storage: State<'_, MascotDataStorage>) -> Result<String, String> {
    Ok(storage.path().to_string_lossy().to_string())
}
