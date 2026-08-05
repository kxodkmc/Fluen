//! 日志相关 Tauri 命令。
//!
//! 前端通过 [`log_frontend`] 批量上报日志到后端，由后端以 `tracing` 事件
//! 重发并写入同一份日志文件，实现全应用统一日志视图。
//!
//! ## target 约定
//!
//! 前端日志的 `target` 为 `fluen_frontend::{scope}`，后端日志为模块路径
//! （如 `fluen_lib::task_queue::runner`），便于在 EnvFilter 中按来源过滤。
//!
//! ## 级别过滤
//!
//! 前端日志受全局 EnvFilter 约束：若全局级别为 `info`，则前端 `trace`/`debug`
//! 不会落盘。这与后端行为一致，避免噪音。

use serde::{Deserialize, Serialize};
use tauri::State;
use tauri_plugin_opener::OpenerExt;
use tracing::{self, Level};

use crate::app_config::storage::AppConfigStorage;
use super::{resolve_logs_dir};

/// 前端日志级别。与前端 `logger.ts` 的级别对齐（小写序列化）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrontendLogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl FrontendLogLevel {
    /// 转换为 `tracing::Level`。
    fn to_tracing(self) -> Level {
        match self {
            Self::Trace => Level::TRACE,
            Self::Debug => Level::DEBUG,
            Self::Info => Level::INFO,
            Self::Warn => Level::WARN,
            Self::Error => Level::ERROR,
        }
    }
}

/// 前端上报的单条日志。
#[derive(Debug, Clone, Deserialize)]
pub struct FrontendLogEntry {
    /// 级别。
    pub level: FrontendLogLevel,
    /// 作用域标签（如 `editor`、`kb`、`references`）。
    pub scope: String,
    /// 日志消息。
    pub message: String,
    /// 可选结构化上下文（已由前端序列化为字符串，原样记录）。
    #[serde(default)]
    pub context: Option<String>,
}

/// 接收前端批量日志并写入统一日志文件。
///
/// 前端 `logger.ts` 会缓冲并批量调用此命令。每条日志以
/// `target = fluen_frontend`、`scope` 为结构化字段重发为 `tracing` 事件，
/// 与后端日志共享同一文件输出与级别过滤。
#[tauri::command]
pub async fn log_frontend(entries: Vec<FrontendLogEntry>) {
    for entry in entries {
        let FrontendLogEntry { level, scope, message, context } = entry;
        let msg = match context {
            Some(ref ctx) if !ctx.is_empty() => format!("{message} {ctx}"),
            _ => message,
        };
        // target 必须为 &'static str；scope 作为结构化字段记录，
        // 可用 EnvFilter `fluen_frontend=debug` 整体过滤前端日志。
        match level.to_tracing() {
            Level::TRACE => tracing::trace!(target: "fluen_frontend", scope = %scope, "{}", msg),
            Level::DEBUG => tracing::debug!(target: "fluen_frontend", scope = %scope, "{}", msg),
            Level::INFO => tracing::info!(target: "fluen_frontend", scope = %scope, "{}", msg),
            Level::WARN => tracing::warn!(target: "fluen_frontend", scope = %scope, "{}", msg),
            Level::ERROR => tracing::error!(target: "fluen_frontend", scope = %scope, "{}", msg),
        }
    }
}

/// 打开日志存放目录（在系统文件管理器中打开）。
///
/// 解析当前配置的日志目录（自定义或默认 `cache_dir/logs`），
/// 目录不存在时自动创建，随后通过 `tauri-plugin-opener` 打开。
#[tauri::command]
pub fn open_logs_dir(
    app: tauri::AppHandle,
    storage: State<'_, AppConfigStorage>,
) -> Result<String, String> {
    let config = storage.get().map_err(|e| e.to_string())?;
    let log_config = &config.logging;
    let logs_dir = resolve_logs_dir(log_config).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(logs_dir.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())?;
    Ok(logs_dir.to_string_lossy().to_string())
}

// ===========================================================================
// 测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontend_log_level_serde_lowercase() {
        let json = serde_json::to_string(&FrontendLogLevel::Warn).unwrap();
        assert_eq!(json, "\"warn\"");
        let lvl: FrontendLogLevel = serde_json::from_str("\"debug\"").unwrap();
        assert!(matches!(lvl, FrontendLogLevel::Debug));
    }

    #[test]
    fn frontend_log_level_to_tracing() {
        assert_eq!(FrontendLogLevel::Trace.to_tracing(), Level::TRACE);
        assert_eq!(FrontendLogLevel::Error.to_tracing(), Level::ERROR);
    }

    #[test]
    fn frontend_log_entry_deserialize_minimal() {
        let json = r#"{"level":"info","scope":"editor","message":"saved"}"#;
        let entry: FrontendLogEntry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry.level, FrontendLogLevel::Info));
        assert_eq!(entry.scope, "editor");
        assert!(entry.context.is_none());
    }

    #[test]
    fn frontend_log_entry_deserialize_with_context() {
        let json = r#"{"level":"error","scope":"kb","message":"build failed","context":"{\"task\":\"abc\"}"}"#;
        let entry: FrontendLogEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.context.as_deref(), Some("{\"task\":\"abc\"}"));
    }
}
