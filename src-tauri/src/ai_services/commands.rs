//! Tauri commands——AI 服务配置 CRUD 与 OCR 执行。
//!
//! ## 配置管理
//!
//! ```typescript
//! // 读取配置
//! const config = await invoke<AiServicesConfig>('get_ai_services_config');
//! // 保存配置
//! await invoke('save_ai_services_config', { config: newConfig });
//! // 获取配置文件路径
//! const path = await invoke<string>('get_ai_services_config_path');
//! ```
//!
//! ## OCR 执行
//!
//! ```typescript
//! // 开始 OCR 识别（监听进度事件）
//! const unlisten = await listen<OcrProgress>('ocr:progress', (e) => {
//!   console.log(e.payload.state, e.payload.extracted_pages);
//! });
//! const result = await invoke<OcrResult>('ocr_recognize', { filePath: '/path/to/file.pdf' });
//! unlisten();
//!
//! // 取消当前 OCR 任务
//! await invoke('ocr_cancel');
//! ```
//!
//! ## 事件流
//!
//! | 事件 | payload | 说明 |
//! |------|---------|------|
//! | `ocr:progress` | `OcrProgress` | 任务进度（pending / running / done / failed） |

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{Emitter, State, Window};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::error::AiServiceError;
use super::model::{AiServicesConfig, ServiceCategory};
use super::paddleocr::{OcrProgress, OcrResult, PaddleOcrClient};
use super::storage::ConfigStorage;

// ===========================================================================
// 事件常量
// ===========================================================================

/// OCR 进度事件——任务状态变化时推送。
pub const EVENT_OCR_PROGRESS: &str = "ocr:progress";

// ===========================================================================
// OCR 运行状态
// ===========================================================================

/// OCR 运行状态：维护活跃的取消令牌。
///
/// 通过 `Mutex<HashMap<run_id, CancellationToken>>` 管理当前活跃的 OCR 任务，
/// 供 `ocr_cancel` 取消。
pub struct OcrState {
    /// 活跃的取消令牌映射（key: run_id）。
    tokens: Mutex<HashMap<String, CancellationToken>>,
}

impl OcrState {
    /// 创建新的 OCR 状态实例。
    pub fn new() -> Self {
        Self {
            tokens: Mutex::new(HashMap::new()),
        }
    }

    /// 注册取消令牌。
    fn register(&self, run_id: String, token: CancellationToken) {
        let mut map = self.tokens.lock().unwrap();
        map.insert(run_id, token);
    }

    /// 注销取消令牌。
    fn unregister(&self, run_id: &str) {
        let mut map = self.tokens.lock().unwrap();
        map.remove(run_id);
    }

    /// 取消当前活跃的 OCR 任务。
    ///
    /// 取消第一个找到的活跃令牌并移除。返回是否成功取消。
    fn cancel_active(&self) -> bool {
        let mut map = self.tokens.lock().unwrap();
        if let Some(key) = map.keys().next().cloned() {
            if let Some(token) = map.remove(&key) {
                token.cancel();
                return true;
            }
        }
        false
    }
}

impl Default for OcrState {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// 配置 CRUD commands
// ===========================================================================

/// 读取完整 AI 服务配置。
///
/// 文件不存在时返回默认空配置。
#[tauri::command]
pub fn get_ai_services_config(
    storage: State<'_, ConfigStorage>,
) -> Result<AiServicesConfig, String> {
    storage.get().map_err(|e| e.to_string())
}

/// 保存完整 AI 服务配置（原子写入）。
///
/// 保存前会校验配置完整性，校验失败时返回错误信息。
#[tauri::command]
pub fn save_ai_services_config(
    storage: State<'_, ConfigStorage>,
    config: AiServicesConfig,
) -> Result<(), String> {
    storage.save(&config).map_err(|e| e.to_string())
}

/// 返回配置文件的完整路径（供前端展示）。
#[tauri::command]
pub fn get_ai_services_config_path(storage: State<'_, ConfigStorage>) -> Result<String, String> {
    Ok(storage.path().to_string_lossy().to_string())
}

// ===========================================================================
// OCR 执行 commands
// ===========================================================================

/// 执行 OCR 识别。
///
/// 使用当前激活的 OCR 提供商对指定文件进行文字识别。
/// 任务进度通过 [`EVENT_OCR_PROGRESS`] 事件推送到前端。
///
/// # 参数
///
/// - `file_path`: 本地文件路径或以 `http` 开头的 URL
///
/// # 返回
///
/// 返回 [`OcrResult`]，包含每页的 markdown 文本与下载到本地的图片路径。
///
/// # 错误
///
/// - 未配置 OCR 提供商
/// - 提供商缺少必要配置（`api_base_url`、`api_key`）
/// - API 调用失败（网络错误、Token 错误等）
/// - 任务被取消
#[tauri::command]
pub async fn ocr_recognize(
    file_path: String,
    window: Window,
    storage: State<'_, ConfigStorage>,
    ocr_state: State<'_, OcrState>,
) -> Result<OcrResult, AiServiceError> {
    // 1. 加载配置并查找活跃的 OCR 提供商
    let config = storage.get()?;

    let provider = config.active_provider(ServiceCategory::Ocr).ok_or_else(|| {
        AiServiceError::Other("未配置 OCR 提供商，请在设置中添加".into())
    })?;

    if !provider.enabled {
        return Err(AiServiceError::Other(
            "当前 OCR 提供商已禁用，请在设置中启用".into(),
        ));
    }

    // 2. 创建客户端
    let client = PaddleOcrClient::from_provider(provider)?;

    // 3. 注册取消令牌
    let run_id = Uuid::new_v4().to_string();
    let cancel_token = CancellationToken::new();
    ocr_state.register(run_id.clone(), cancel_token.clone());

    // 4. 执行识别
    let window_ref = window.clone();
    let result = client
        .recognize(
            &file_path,
            move |progress: OcrProgress| {
                let _ = window_ref.emit(EVENT_OCR_PROGRESS, &progress);
            },
            &cancel_token,
        )
        .await;

    // 5. 清理取消令牌
    ocr_state.unregister(&run_id);

    result
}

/// 取消当前活跃的 OCR 任务。
#[tauri::command]
pub fn ocr_cancel(ocr_state: State<'_, OcrState>) -> Result<(), AiServiceError> {
    ocr_state.cancel_active();
    Ok(())
}
