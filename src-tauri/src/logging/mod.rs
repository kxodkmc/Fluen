//! # logging
//!
//! 统一日志系统——基于 `tracing` + `tracing-subscriber` + `tracing-appender`。
//!
//! 提供能力：
//! - 按天滚动的文件日志（非阻塞写入），与前端日志同文件
//! - 可选的控制台镜像（开发模式）
//! - 环境变量 `FLUEN_LOG` 覆盖级别（最高优先，便于临时调试）
//! - 前端日志经 [`commands::log_frontend`] 桥接到同一文件
//!
//! ## 日志位置
//!
//! `fluen_cache_dir()/logs/fluen.log.YYYY-MM-DD`（遵循 AGENTS.md 缓存目录规范，
//! 可随时清除，不影响功能）。
//!
//! ## 初始化
//!
//! 在 [`crate::run`] 最开头调用 [`init_logging`]，返回的
//! [`WorkerGuard`] 必须在应用整个生命周期内持有（存入 Tauri manage state，
//! 见 [`LogGuardHolder`]），drop 时刷盘并关闭后台写入线程。
//!
//! ## 级别优先级
//!
//! 1. 环境变量 `FLUEN_LOG`（如 `debug`、`info,fluen_frontend=debug`）
//! 2. [`LogConfig::level`]
//!
//! ## 稳定性
//!
//! 日志初始化失败**不影响应用启动**：返回 `Err` 时调用方应继续运行，
//! 后续 `tracing::` 调用退化为静默（无输出）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub mod commands;

/// 日志配置。对应 `app_config.json` 中的 `logging` 段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// 全局日志级别：`trace`/`debug`/`info`/`warn`/`error`。
    #[serde(default = "default_level")]
    pub level: String,
    /// 是否同时输出到 stdout 控制台（建议仅在开发模式开启）。
    #[serde(default)]
    pub console_enabled: bool,
    /// 日志文件保留天数，过期文件启动时清理。`0` 表示不清理。
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: default_level(),
            console_enabled: false,
            retention_days: default_retention_days(),
        }
    }
}

fn default_level() -> String {
    "info".to_string()
}

fn default_retention_days() -> u32 {
    7
}

/// 持有 [`WorkerGuard`]，存入 Tauri manage state 以保证应用生命周期内不 drop。
///
/// guard drop 时会刷盘并关闭 non-blocking 写入线程。
#[derive(Default)]
pub struct LogGuardHolder(pub Option<WorkerGuard>);

/// 日志初始化错误（仅用于日志，不向上传播以避免阻塞应用启动）。
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    #[error("无法创建日志目录 {dir}: {source}")]
    CreateDir { dir: String, source: std::io::Error },
}

/// 初始化全局日志系统。
///
/// 返回的 `WorkerGuard` 必须在应用整个生命周期内持有。
///
/// # 级别
///
/// 优先读取环境变量 `FLUEN_LOG`，未设置时回退到 `config.level`。
///
/// # 文件滚动
///
/// 使用 `tracing_appender::rolling::daily`，文件名 `fluen.log.YYYY-MM-DD`。
/// 启动时按文件名中的日期清理超过 `retention_days` 的文件。
pub fn init_logging(
    cache_dir: PathBuf,
    config: &LogConfig,
) -> Result<WorkerGuard, LogError> {
    let logs_dir = cache_dir.join("logs");
    std::fs::create_dir_all(&logs_dir).map_err(|e| LogError::CreateDir {
        dir: logs_dir.display().to_string(),
        source: e,
    })?;

    cleanup_old_logs(&logs_dir, config.retention_days);

    // 按天滚动文件 appender（当前文件名为 fluen.log.YYYY-MM-DD）
    let file_appender = tracing_appender::rolling::daily(&logs_dir, "fluen.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // 构建 EnvFilter：环境变量优先
    let filter = EnvFilter::try_from_env("FLUEN_LOG")
        .or_else(|_| EnvFilter::try_new(&config.level))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    // 文件 layer（非阻塞、无 ANSI 颜色码）
    let file_layer = fmt::layer()
        .with_target(true)
        .with_level(true)
        .with_ansi(false)
        .with_writer(non_blocking);

    let registry = tracing_subscriber::registry().with(filter).with(file_layer);

    if config.console_enabled {
        let console_layer = fmt::layer()
            .with_target(true)
            .with_level(true)
            .with_ansi(true)
            .with_writer(std::io::stdout);
        registry.with(console_layer).init();
    } else {
        registry.init();
    }

    tracing::info!(
        target: "fluen_logging",
        dir = %logs_dir.display(),
        level = %config.level,
        console = config.console_enabled,
        "日志系统已初始化"
    );

    Ok(guard)
}

/// 清理过期日志文件。
///
/// 按文件名中的日期判断（`fluen.log.YYYY-MM-DD`），删除早于
/// `retention_days` 前的文件。解析失败或非日志文件跳过。
/// 任何 IO 错误静默忽略，不影响启动。
fn cleanup_old_logs(logs_dir: &Path, retention_days: u32) {
    if retention_days == 0 {
        return;
    }
    let cutoff =
        chrono::Local::now().date_naive() - chrono::Duration::days(retention_days as i64);

    let entries = match std::fs::read_dir(logs_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(date_str) = name.strip_prefix("fluen.log.") else {
            continue;
        };
        let Ok(date) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") else {
            continue;
        };
        if date < cutoff {
            let _ = std::fs::remove_file(&path);
        }
    }
}

// ===========================================================================
// 测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_config_default() {
        let cfg = LogConfig::default();
        assert_eq!(cfg.level, "info");
        assert!(!cfg.console_enabled);
        assert_eq!(cfg.retention_days, 7);
    }

    #[test]
    fn log_config_serde_roundtrip() {
        let cfg = LogConfig {
            level: "debug".into(),
            console_enabled: true,
            retention_days: 14,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: LogConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.level, "debug");
        assert!(parsed.console_enabled);
        assert_eq!(parsed.retention_days, 14);
    }

    #[test]
    fn log_config_deserialize_missing_fields_uses_defaults() {
        let json = r#"{}"#;
        let cfg: LogConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.level, "info");
        assert!(!cfg.console_enabled);
        assert_eq!(cfg.retention_days, 7);
    }

    #[test]
    fn cleanup_old_logs_removes_expired_keeps_recent() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_log_test_{}_cleanup",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        // 过期文件（2020 年）
        let old_file = tmp.join("fluen.log.2020-01-01");
        std::fs::write(&old_file, "old").unwrap();

        // 今天文件
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let recent_file = tmp.join(format!("fluen.log.{today}"));
        std::fs::write(&recent_file, "recent").unwrap();

        // 非日志文件（应保留）
        let other_file = tmp.join("notes.txt");
        std::fs::write(&other_file, "notes").unwrap();

        cleanup_old_logs(&tmp, 7);

        assert!(!old_file.exists(), "过期日志应被删除");
        assert!(recent_file.exists(), "当天日志应保留");
        assert!(other_file.exists(), "非日志文件应保留");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn cleanup_zero_retention_is_noop() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_log_test_{}_zero",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let old_file = tmp.join("fluen.log.2020-01-01");
        std::fs::write(&old_file, "old").unwrap();

        cleanup_old_logs(&tmp, 0);

        assert!(old_file.exists(), "retention_days=0 应不清理");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
