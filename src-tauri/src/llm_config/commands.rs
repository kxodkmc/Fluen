//! Tauri commands——供前端调用的 LLM 配置 CRUD 接口。
//!
//! 通过 `@tauri-apps/api` 的 `invoke` 函数调用：
//!
//! ```typescript
//! // 读取配置
//! const config = await invoke<LlmConfig>('get_llm_config');
//! // 保存配置
//! await invoke('save_llm_config', { config: newConfig });
//! // 获取配置文件路径
//! const path = await invoke<string>('get_llm_config_path');
//! ```

use tauri::State;

use super::model::LlmConfig;
use super::storage::ConfigStorage;

/// 读取完整 LLM 配置。
///
/// 文件不存在时返回默认空配置。
#[tauri::command]
pub fn get_llm_config(storage: State<'_, ConfigStorage>) -> Result<LlmConfig, String> {
    storage.load().map_err(|e| e.to_string())
}

/// 保存完整 LLM 配置（原子写入）。
///
/// 保存前会校验配置完整性，校验失败时返回错误信息。
#[tauri::command]
pub fn save_llm_config(
    storage: State<'_, ConfigStorage>,
    config: LlmConfig,
) -> Result<(), String> {
    storage.save(&config).map_err(|e| e.to_string())
}

/// 返回配置文件的完整路径（供前端展示）。
#[tauri::command]
pub fn get_llm_config_path(storage: State<'_, ConfigStorage>) -> Result<String, String> {
    Ok(storage.path().to_string_lossy().to_string())
}
