//! Tauri commands——编辑器核心模块的前端接口。
//!
//! 本模块仅做参数接收与错误转换，业务逻辑委托给 [`EditorEngine`](super::engine::EditorEngine)。

use parking_lot::Mutex;

use super::config::EditorConfig;
use super::edit::EditLabel;
use super::engine::EditorEngine;
use super::error::EditorError;

/// 编辑器状态——全局唯一的 EditorEngine 实例（可选）。
///
/// 使用 `parking_lot::Mutex` 防中毒（panic 后锁不中毒，后续 command 可继续获取锁）。
pub struct EditorState(pub Mutex<Option<EditorEngine>>);

impl EditorState {
    /// 创建空状态。
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }
}

impl Default for EditorState {
    fn default() -> Self {
        Self::new()
    }
}

/// 错误响应（可序列化给前端）。
#[derive(Debug, serde::Serialize)]
pub struct EditorErrorResponse {
    pub message: String,
}

impl From<EditorError> for EditorErrorResponse {
    fn from(e: EditorError) -> Self {
        Self { message: e.to_string() }
    }
}

// ── Commands ──

/// 加载项目 main.md 到引擎。
#[tauri::command]
pub fn editor_load(
    state: tauri::State<EditorState>,
    project_path: String,
) -> Result<(), EditorErrorResponse> {
    let mut guard = state.0.lock();
    if guard.is_none() {
        *guard = Some(EditorEngine::new(EditorConfig::default()));
    }
    let engine = guard.as_mut().unwrap();
    engine.load_project(&project_path).map_err(Into::into)
}

/// 获取当前 source_md 全文。
#[tauri::command]
pub fn editor_get_text(state: tauri::State<EditorState>) -> Result<String, EditorErrorResponse> {
    let guard = state.0.lock();
    match guard.as_ref() {
        Some(engine) => Ok(engine.get_text().to_string()),
        None => Err(EditorErrorResponse { message: "编辑器未初始化".into() }),
    }
}

/// 区间替换文本。
#[tauri::command]
pub fn editor_replace_text(
    state: tauri::State<EditorState>,
    start: usize,
    end: usize,
    new_text: String,
    label: Option<String>,
) -> Result<(), EditorErrorResponse> {
    let label = match label.as_deref() {
        Some("auto_optimize") => EditLabel::AutoOptimize,
        Some("system") => EditLabel::System,
        _ => EditLabel::User,
    };
    let mut guard = state.0.lock();
    match guard.as_mut() {
        Some(engine) => engine.replace_text(start..end, &new_text, label).map_err(Into::into),
        None => Err(EditorErrorResponse { message: "编辑器未初始化".into() }),
    }
}

/// 撤销。
#[tauri::command]
pub fn editor_undo(state: tauri::State<EditorState>) -> Result<UndoRedoResult, EditorErrorResponse> {
    let mut guard = state.0.lock();
    match guard.as_mut() {
        Some(engine) => {
            let success = engine.undo()?;
            Ok(UndoRedoResult {
                success,
                can_undo: engine.can_undo(),
                can_redo: engine.can_redo(),
            })
        }
        None => Err(EditorErrorResponse { message: "编辑器未初始化".into() }),
    }
}

/// 重做。
#[tauri::command]
pub fn editor_redo(state: tauri::State<EditorState>) -> Result<UndoRedoResult, EditorErrorResponse> {
    let mut guard = state.0.lock();
    match guard.as_mut() {
        Some(engine) => {
            let success = engine.redo()?;
            Ok(UndoRedoResult {
                success,
                can_undo: engine.can_undo(),
                can_redo: engine.can_redo(),
            })
        }
        None => Err(EditorErrorResponse { message: "编辑器未初始化".into() }),
    }
}

/// 渲染为 HTML。
///
/// 传入 `content` 时即时渲染该内容（不修改引擎 source_md，且不要求引擎已加载——
/// 预览渲染是无状态操作，使用默认渲染选项）；
/// 不传或传 `null` 时渲染引擎当前 source_md（要求引擎已加载）。
#[tauri::command]
pub fn editor_render_html(
    state: tauri::State<EditorState>,
    content: Option<String>,
) -> Result<String, EditorErrorResponse> {
    match content {
        // 有内容时无状态渲染：不依赖引擎，用默认配置
        Some(c) => super::render::render_to_html(&c, &super::config::EditorConfig::default().render_options)
            .map_err(Into::into),
        // 无内容时读引擎 source_md
        None => {
            let guard = state.0.lock();
            match guard.as_ref() {
                Some(engine) => engine.render_html().map_err(Into::into),
                None => Err(EditorErrorResponse { message: "编辑器未初始化".into() }),
            }
        }
    }
}

/// 保存到 main.md。
#[tauri::command]
pub fn editor_save(state: tauri::State<EditorState>) -> Result<(), EditorErrorResponse> {
    let mut guard = state.0.lock();
    match guard.as_mut() {
        Some(engine) => engine.save().map_err(Into::into),
        None => Err(EditorErrorResponse { message: "编辑器未初始化".into() }),
    }
}

/// 保存指定内容到 main.md 并拆分回各章节备份文件。
///
/// 若引擎未初始化或绑定的项目路径与 `project_path` 不一致，先重新加载项目。
/// 返回重新加载后的 [`OpenProjectResult`]（含最新章节列表与 main_md）。
#[tauri::command]
pub fn editor_save_content(
    state: tauri::State<EditorState>,
    project_path: String,
    content: String,
) -> Result<crate::project::model::OpenProjectResult, EditorErrorResponse> {
    let mut guard = state.0.lock();

    // 引擎未初始化时创建并加载项目
    if guard.is_none() {
        let mut engine = EditorEngine::new(EditorConfig::default());
        engine
            .load_project(&project_path)
            .map_err(EditorErrorResponse::from)?;
        *guard = Some(engine);
    }

    let engine = guard.as_mut().unwrap();

    // 绑定路径不一致时重新加载（保留 history 的前提下切换项目）
    let need_reload = engine.project_path() != Some(project_path.as_str());
    if need_reload {
        engine
            .load_project(&project_path)
            .map_err(EditorErrorResponse::from)?;
    }

    engine
        .save_content(content)
        .map_err(EditorErrorResponse::from)
}

/// 保存资源文件到项目的 `manuscript/assets/` 目录。
///
/// 纯函数命令（不依赖编辑器状态），对文件名做安全校验防止路径穿越。
/// 成功返回相对路径 `assets/{filename}`。
#[tauri::command]
pub fn editor_save_asset(
    project_path: String,
    filename: String,
    bytes: Vec<u8>,
) -> Result<String, EditorErrorResponse> {
    EditorEngine::save_asset(project_path, filename, bytes).map_err(EditorErrorResponse::from)
}

/// 历史元信息。
#[tauri::command]
pub fn editor_history_info(
    state: tauri::State<EditorState>,
    preview_count: Option<usize>,
) -> Result<HistoryInfo, EditorErrorResponse> {
    let guard = state.0.lock();
    match guard.as_ref() {
        Some(engine) => {
            let (undo_count, redo_count) = engine.history_len();
            let previews = engine.history_preview(preview_count.unwrap_or(10));
            Ok(HistoryInfo {
                undo_count,
                redo_count,
                can_undo: engine.can_undo(),
                can_redo: engine.can_redo(),
                is_dirty: engine.is_dirty(),
                previews,
            })
        }
        None => Err(EditorErrorResponse { message: "编辑器未初始化".into() }),
    }
}

/// 记录文件删除操作（不影响 source_md，仅入栈历史供撤销恢复）。
#[tauri::command]
pub fn editor_record_file_delete(
    state: tauri::State<EditorState>,
    path: String,
    content_before: String,
) -> Result<(), EditorErrorResponse> {
    let mut guard = state.0.lock();
    match guard.as_mut() {
        Some(engine) => {
            engine.record_file_delete(path, content_before);
            Ok(())
        }
        None => Err(EditorErrorResponse { message: "编辑器未初始化".into() }),
    }
}

/// 记录自动优化操作（全文替换，根据配置决定是否入栈历史）。
#[tauri::command]
pub fn editor_record_auto_optimize(
    state: tauri::State<EditorState>,
    before: String,
    after: String,
    description: String,
) -> Result<(), EditorErrorResponse> {
    let mut guard = state.0.lock();
    match guard.as_mut() {
        Some(engine) => {
            engine.record_auto_optimize(before, after, description);
            Ok(())
        }
        None => Err(EditorErrorResponse { message: "编辑器未初始化".into() }),
    }
}

/// 清空历史栈。
#[tauri::command]
pub fn editor_clear_history(state: tauri::State<EditorState>) -> Result<(), EditorErrorResponse> {
    let mut guard = state.0.lock();
    match guard.as_mut() {
        Some(engine) => {
            engine.clear_history();
            Ok(())
        }
        None => Err(EditorErrorResponse { message: "编辑器未初始化".into() }),
    }
}

// ── 响应类型 ──

/// 撤销/重做结果。
#[derive(Debug, serde::Serialize)]
pub struct UndoRedoResult {
    pub success: bool,
    pub can_undo: bool,
    pub can_redo: bool,
}

/// 历史信息。
#[derive(Debug, serde::Serialize)]
pub struct HistoryInfo {
    pub undo_count: usize,
    pub redo_count: usize,
    pub can_undo: bool,
    pub can_redo: bool,
    pub is_dirty: bool,
    pub previews: Vec<super::edit::EditSummary>,
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::creator;
    use crate::project::model::{sanitize_project_name, CreateProjectRequest};
    use std::fs;
    use std::path::{Path, PathBuf};

    /// 创建唯一临时目录（遵循项目测试约定：temp_dir + pid + nanos）。
    fn temp_project_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluen_cmd_test_{}_{:?}_{}",
            std::process::id(),
            std::thread::current().id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 通过 creator 创建测试项目，返回项目根路径。
    fn create_test_project(storage: &Path) -> PathBuf {
        let request = CreateProjectRequest {
            title: "测试文章".into(),
            author: "张三".into(),
            project_name: sanitize_project_name("测试文章"),
            storage_path: storage.to_string_lossy().to_string(),
            description: None,
        };
        PathBuf::from(creator::create_project(request).unwrap())
    }

    // save_asset 写入文件并返回相对路径
    #[test]
    fn save_asset_writes_file_and_returns_relative_path() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        let project_path = project_dir.to_str().unwrap().to_string();

        let bytes = vec![1u8, 2, 3];
        let relative =
            EditorEngine::save_asset(project_path, "test.png".into(), bytes.clone()).unwrap();
        assert_eq!(relative, "assets/test.png");

        let file_path = project_dir.join("manuscript").join("assets").join("test.png");
        assert!(file_path.exists());
        assert_eq!(fs::read(&file_path).unwrap(), bytes);

        let _ = fs::remove_dir_all(&storage);
    }

    // save_asset 拒绝路径穿越文件名
    #[test]
    fn save_asset_rejects_path_traversal() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        let project_path = project_dir.to_str().unwrap().to_string();

        let result = EditorEngine::save_asset(project_path, "../evil.png".into(), vec![]);
        assert!(matches!(result, Err(EditorError::InvalidAssetFilename(_))));
        // 确认 evil.png 未被写到项目上级目录
        assert!(!storage.join("evil.png").exists());

        let _ = fs::remove_dir_all(&storage);
    }

    // save_asset 拒绝空文件名
    #[test]
    fn save_asset_rejects_empty_filename() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        let project_path = project_dir.to_str().unwrap().to_string();

        let result = EditorEngine::save_asset(project_path, "".into(), vec![1]);
        assert!(matches!(result, Err(EditorError::InvalidAssetFilename(_))));

        let _ = fs::remove_dir_all(&storage);
    }

    // 锁不中毒模拟：panic 后再次获取 EditorState 锁成功（parking_lot::Mutex 不中毒）
    #[test]
    fn parking_lot_mutex_does_not_poison_after_panic() {
        let state = EditorState::new();
        // 模拟 command 内部 panic（被 catch_unwind 捕获，但锁已被持有）。
        // AssertUnwindSafe 包装：parking_lot::Mutex 含 UnsafeCell，非 RefUnwindSafe，
        // 但本测试仅验证锁不中毒特性，不依赖 UnwindSafe 的语义保证。
        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = state.0.lock();
            panic!("模拟 command 内部 panic");
        }));
        assert!(panic_result.is_err(), "panic 应被 catch_unwind 捕获");
        // 锁应可再次获取（parking_lot::Mutex 不中毒）
        let guard = state.0.lock();
        assert!(guard.is_none(), "panic 后锁仍可获取，且内容不变");
    }

    // 锁不中毒：连续 panic 多次后锁仍可用
    #[test]
    fn parking_lot_mutex_survives_multiple_panics() {
        let state = EditorState::new();
        for _ in 0..3 {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _guard = state.0.lock();
                panic!("再次 panic");
            }));
        }
        // 多次 panic 后锁仍可获取
        let mut guard = state.0.lock();
        *guard = Some(EditorEngine::new(EditorConfig::default()));
        assert!(guard.is_some(), "锁可用且能正常写入");
    }
}
