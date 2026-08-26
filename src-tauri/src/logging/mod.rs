//! # logging
//!
//! 统一日志系统——基于 `tracing` + `tracing-subscriber` + 自定义滚动文件写入器。
//!
//! 提供能力：
//! - 每次运行独立日志文件（文件名含 `YYYYMMDDHHMMSS` 时间戳）
//! - 单文件条目数超阈值时自动滚动创建新文件（`-1`/`-2` 后缀）
//! - 累计文件数超阈值时自动清理最旧文件
//! - 可选的日志存放目录（默认 `fluen_cache_dir()/logs/`）
//! - 可选的控制台镜像（开发模式）
//! - 环境变量 `FLUEN_LOG` 覆盖级别（最高优先，便于临时调试）
//! - 前端日志经 [`commands::log_frontend`] 桥接到同一文件
//!
//! ## 输出格式（[`CompactFormat`]）
//!
//! 统一使用精简格式：`时间 级别 target: 消息 字段…`。**不渲染 span 上下文**，
//! 消除 referee_ai 等产生的超长 span 前缀（如 `base_turn{…}:execute_single{…}:`），
//! 降低噪声、突出级别、模块与关键结构化字段，便于定位。文件纯文本，控制台可配 ANSI。
//!
//! ## 接入约定
//!
//! - 模块内部用 `tracing::{info,debug,warn,error,trace}`（target 默认取模块路径）。
//! - 关键阶段里程碑用 `info`；过程/检索细节用 `debug`；更细粒度用 `trace`。
//! - 关键结构化字段（如 `task_id`、`ref_id`、`wiki_id`）以 `field` 形式随日志事件给出，
//!   供复盘与 `EnvFilter` 按字段/模块筛选。默认级别为 `info`，需要明细时用
//!   `FLUEN_LOG=fluen_lib::knowledge_builder=debug,info` 调级，避免全局噪声。
//!
//! ## 文件命名
//!
//! - 首个文件：`fluen_YYYYMMDDHHMMSS.log`
//! - 滚动文件：`fluen_YYYYMMDDHHMMSS-1.log`、`fluen_YYYYMMDDHHMMSS-2.log`…
//!
//! ## 初始化
//!
//! 在 [`crate::run`] 最开头调用 [`init_logging`]，返回的
//! [`WorkerGuard`] 必须在应用整个生命周期内持有（存入 Tauri manage state，
//! 见 [`LogGuardHolder`]），drop 时刷盘并关闭后台写入线程。

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{field::{Field, Visit}, Event, Level};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_core::Subscriber;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::fmt::FmtContext;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::platform;

pub mod commands;

/// 单文件最大条目数默认值。
const DEFAULT_MAX_ENTRIES_PER_FILE: u32 = 4096;
/// 单文件最大条目数下限。
pub const MIN_MAX_ENTRIES_PER_FILE: u32 = 512;
/// 单文件最大条目数上限。
pub const MAX_MAX_ENTRIES_PER_FILE: u32 = 8192;

/// 累计日志文件数默认值。
const DEFAULT_MAX_FILE_COUNT: u32 = 64;
/// 累计日志文件数下限。
pub const MIN_MAX_FILE_COUNT: u32 = 1;
/// 累计日志文件数上限。
pub const MAX_MAX_FILE_COUNT: u32 = 8192;

/// 日志文件名前缀。
const LOG_FILE_PREFIX: &str = "fluen_";

/// 日志配置。对应 `app_config.json` 中的 `logging` 段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// 全局日志级别：`trace`/`debug`/`info`/`warn`/`error`。
    #[serde(default = "default_level")]
    pub level: String,
    /// 是否同时输出到 stdout 控制台（建议仅在开发模式开启）。
    #[serde(default)]
    pub console_enabled: bool,
    /// 自定义日志存放目录。为空时使用默认缓存目录下的 `logs/` 子目录。
    #[serde(default)]
    pub log_dir: Option<String>,
    /// 单个日志文件最大条目数（超限自动滚动创建新文件）。
    #[serde(default = "default_max_entries_per_file")]
    pub max_entries_per_file: u32,
    /// 累计日志文件数上限（超限自动清理最旧文件）。
    #[serde(default = "default_max_file_count")]
    pub max_file_count: u32,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: default_level(),
            console_enabled: false,
            log_dir: None,
            max_entries_per_file: default_max_entries_per_file(),
            max_file_count: default_max_file_count(),
        }
    }
}

impl LogConfig {
    /// 校验配置完整性。
    ///
    /// - `max_entries_per_file` 必须在 [`MIN_MAX_ENTRIES_PER_FILE`]..=[`MAX_MAX_ENTRIES_PER_FILE`]
    /// - `max_file_count` 必须在 [`MIN_MAX_FILE_COUNT`]..=[`MAX_MAX_FILE_COUNT`]
    /// - `log_dir` 若存在则不能为空字符串
    pub fn validate(&self) -> Result<(), String> {
        if !(MIN_MAX_ENTRIES_PER_FILE..=MAX_MAX_ENTRIES_PER_FILE).contains(&self.max_entries_per_file)
        {
            return Err(format!(
                "max_entries_per_file 必须在 {MIN_MAX_ENTRIES_PER_FILE}-{MAX_MAX_ENTRIES_PER_FILE} 范围内，当前为: {}",
                self.max_entries_per_file
            ));
        }
        if !(MIN_MAX_FILE_COUNT..=MAX_MAX_FILE_COUNT).contains(&self.max_file_count) {
            return Err(format!(
                "max_file_count 必须在 {MIN_MAX_FILE_COUNT}-{MAX_MAX_FILE_COUNT} 范围内，当前为: {}",
                self.max_file_count
            ));
        }
        if let Some(dir) = &self.log_dir {
            if dir.trim().is_empty() {
                return Err("log_dir 不能为空字符串".into());
            }
        }
        Ok(())
    }
}

fn default_level() -> String {
    "info".to_string()
}

fn default_max_entries_per_file() -> u32 {
    DEFAULT_MAX_ENTRIES_PER_FILE
}

fn default_max_file_count() -> u32 {
    DEFAULT_MAX_FILE_COUNT
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
    #[error("无法创建日志文件 {path}: {source}")]
    CreateFile { path: String, source: std::io::Error },
    #[error("无法确定平台缓存目录")]
    Platform,
}

/// 解析日志存放目录。
///
/// 优先使用 `config.log_dir`，为空时回退到 `fluen_cache_dir()/logs/`。
/// 目录不存在时自动创建。
pub fn resolve_logs_dir(config: &LogConfig) -> Result<PathBuf, LogError> {
    let logs_dir = match config.log_dir.as_ref().filter(|d| !d.trim().is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => platform::fluen_cache_dir()
            .map_err(|_| LogError::Platform)?
            .join("logs"),
    };
    std::fs::create_dir_all(&logs_dir).map_err(|e| LogError::CreateDir {
        dir: logs_dir.display().to_string(),
        source: e,
    })?;
    Ok(logs_dir)
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
/// 每次运行创建独立日志文件（`fluen_YYYYMMDDHHMMSS.log`），
/// 单文件条目数超 `max_entries_per_file` 时自动滚动（`-1`/`-2` 后缀），
/// 累计文件数超 `max_file_count` 时自动清理最旧文件。
pub fn init_logging(config: &LogConfig) -> Result<WorkerGuard, LogError> {
    let logs_dir = resolve_logs_dir(config)?;
    let max_entries = config.max_entries_per_file as u64;

    cleanup_excess_logs(&logs_dir, config.max_file_count);

    let writer = RollingFileWriter::new(&logs_dir, max_entries)?;
    let (non_blocking, guard) = tracing_appender::non_blocking(writer);

    // 构建 EnvFilter：环境变量优先
    let filter = EnvFilter::try_from_env("FLUEN_LOG")
        .or_else(|_| EnvFilter::try_new(&config.level))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    // 文件 layer（非阻塞、纯文本、精简格式）
    let file_layer = fmt::layer()
        .event_format(CompactFormat::default())
        .with_writer(non_blocking);

    let registry = tracing_subscriber::registry().with(filter).with(file_layer);

    if config.console_enabled {
        let console_layer = fmt::layer()
            .event_format(CompactFormat::default().with_ansi(true))
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
        max_entries = config.max_entries_per_file,
        max_files = config.max_file_count,
        "日志系统已初始化"
    );

    Ok(guard)
}

// ===========================================================================
// 自定义紧凑事件格式器
// ===========================================================================

/// 精简日志事件格式：`时间 级别 target: 消息 字段…`，不渲染 span 上下文。
///
/// 相比默认 `tracing_subscriber` 输出，去掉 `referee_ai` 等产生的超长 span 前缀
/// （如 `base_turn{…}:execute_single{…}:`），显著降低噪声、突出真实信息。
/// 控制台可启用 ANSI 颜色，日志文件保持纯文本。
#[derive(Clone, Copy)]
struct CompactFormat {
    ansi: bool,
}

impl Default for CompactFormat {
    fn default() -> Self {
        Self { ansi: false }
    }
}

impl CompactFormat {
    fn with_ansi(mut self, on: bool) -> Self {
        self.ansi = on;
        self
    }
}

/// 写入本地时间前缀：`YYYY-MM-DD HH:MM:SS.mmm`。
fn write_timestamp(w: &mut dyn std::fmt::Write) -> std::fmt::Result {
    let now = chrono::Local::now();
    write!(w, "{} ", now.format("%Y-%m-%d %H:%M:%S%.3f"))
}

/// 级别标签（可选 ANSI 着色）。
fn level_label(level: &Level, ansi: bool) -> String {
    let (text, code) = match *level {
        Level::ERROR => ("ERROR", "31"),
        Level::WARN => ("WARN", "33"),
        Level::INFO => ("INFO", "32"),
        Level::DEBUG => ("DEBUG", "36"),
        Level::TRACE => ("TRACE", "90"),
    };
    if ansi {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

/// 将事件字段渲染为 `key=value`（`message` 字段仅输出文本，不带键名）。
struct FieldRenderer<'w> {
    w: &'w mut dyn std::fmt::Write,
    first: bool,
}

impl FieldRenderer<'_> {
    fn put(&mut self, field: &Field, args: std::fmt::Arguments<'_>) {
        let sep = if self.first { "" } else { " " };
        if field.name() == "message" {
            let _ = write!(self.w, "{sep}{args}");
        } else {
            let _ = write!(self.w, "{sep}{}={args}", field.name());
        }
        self.first = false;
    }
}

impl Visit for FieldRenderer<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.put(field, format_args!("{value:?}"));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.put(field, format_args!("{value}"));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.put(field, format_args!("{value}"));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.put(field, format_args!("{value}"));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.put(field, format_args!("{value}"));
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.put(field, format_args!("{value}"));
    }
    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.put(field, format_args!("{value}"));
    }
}

impl<S, N> FormatEvent<S, N> for CompactFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> std::fmt::Result {
        let meta = event.metadata();

        write_timestamp(&mut writer)?;
        write!(writer, "{} ", level_label(meta.level(), self.ansi))?;
        write!(writer, "{}: ", meta.target())?;

        {
            let mut renderer = FieldRenderer {
                w: &mut writer,
                first: true,
            };
            event.record(&mut renderer);
        }
        writeln!(writer)?;
        Ok(())
    }
}

// ===========================================================================
// 滚动文件写入器
// ===========================================================================

/// 基于条目数的滚动文件写入器。
///
/// 每次运行以 `fluen_YYYYMMDDHHMMSS.log` 起始，按换行符统计已写入条目数，
/// 达到 `max_entries` 后自动滚动到 `fluen_YYYYMMDDHHMMSS-1.log` 等后续文件。
/// 适用于 `tracing_appender::non_blocking` 的后台线程模型——单线程独占访问。
pub struct RollingFileWriter {
    logs_dir: PathBuf,
    session_ts: String,
    file_index: u32,
    current_file: Option<std::io::BufWriter<File>>,
    entry_count: u64,
    max_entries: u64,
}

impl RollingFileWriter {
    /// 创建写入器并打开首个会话文件。
    pub fn new(logs_dir: &Path, max_entries: u64) -> Result<Self, LogError> {
        let session_ts = chrono::Local::now().format("%Y%m%d%H%M%S").to_string();
        let mut writer = Self {
            logs_dir: logs_dir.to_path_buf(),
            session_ts,
            file_index: 0,
            current_file: None,
            entry_count: 0,
            max_entries,
        };
        writer.open_session_file()?;
        Ok(writer)
    }

    /// 打开当前 `file_index` 对应的会话文件。
    ///
    /// - index 0 → `fluen_YYYYMMDDHHMMSS.log`
    /// - index N → `fluen_YYYYMMDDHHMMSS-N.log`
    ///
    /// 若文件已存在（同秒内重复启动），递增后缀直到找到可用文件名。
    fn open_session_file(&mut self) -> Result<(), LogError> {
        // 先关闭旧文件（BufWriter drop 时自动 flush）
        self.current_file = None;

        let filename = if self.file_index == 0 {
            format!("{LOG_FILE_PREFIX}{}.log", self.session_ts)
        } else {
            format!("{LOG_FILE_PREFIX}{}-{}.log", self.session_ts, self.file_index)
        };
        let mut path = self.logs_dir.join(&filename);

        // 同秒冲突时递增后缀
        while path.exists() && self.file_index == 0 {
            self.file_index += 1;
            path = self.logs_dir.join(format!(
                "{LOG_FILE_PREFIX}{}-{}.log",
                self.session_ts, self.file_index
            ));
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| LogError::CreateFile {
                path: path.display().to_string(),
                source: e,
            })?;
        self.current_file = Some(std::io::BufWriter::new(file));
        self.entry_count = 0;
        Ok(())
    }

    /// 滚动到下一个文件。
    fn rotate(&mut self) -> Result<(), LogError> {
        self.file_index += 1;
        self.open_session_file()
    }
}

impl Write for RollingFileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let file = self
            .current_file
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "日志文件未打开"))?;
        let written = file.write(buf)?;
        // 按换行符统计条目数
        let newlines = buf.iter().filter(|&&b| b == b'\n').count() as u64;
        self.entry_count += newlines;
        // 达到阈值时滚动（当前条目已完整写入旧文件）
        if self.entry_count >= self.max_entries {
            if let Some(f) = self.current_file.as_mut() {
                f.flush()?;
            }
            let _ = self.rotate();
        }
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(f) = self.current_file.as_mut() {
            f.flush()?;
        }
        Ok(())
    }
}

// ===========================================================================
// 旧文件清理
// ===========================================================================

/// 清理超额日志文件。
///
/// 扫描目录下所有 `fluen_*.log` 文件，按文件名排序（时间戳即年代序），
/// 保留最新的 `max_file_count` 个，删除其余。
fn cleanup_excess_logs(logs_dir: &Path, max_file_count: u32) {
    if max_file_count == 0 {
        return;
    }
    let entries = match std::fs::read_dir(logs_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with(LOG_FILE_PREFIX) && n.ends_with(".log"))
                .unwrap_or(false)
        })
        .collect();

    if files.len() <= max_file_count as usize {
        return;
    }

    // 按文件名升序（时间戳即年代序），删除最旧的
    files.sort();
    let to_remove = files.len().saturating_sub(max_file_count as usize);
    for path in files.iter().take(to_remove) {
        let _ = std::fs::remove_file(path);
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
        assert!(cfg.log_dir.is_none());
        assert_eq!(cfg.max_entries_per_file, DEFAULT_MAX_ENTRIES_PER_FILE);
        assert_eq!(cfg.max_file_count, DEFAULT_MAX_FILE_COUNT);
    }

    #[test]
    fn level_label_plain_and_ansi() {
        assert_eq!(level_label(&Level::INFO, false), "INFO");
        assert_eq!(level_label(&Level::ERROR, true), "\x1b[31mERROR\x1b[0m");
    }

    #[test]
    fn compact_format_ansi_flag() {
        assert!(!CompactFormat::default().ansi);
        assert!(CompactFormat::default().with_ansi(true).ansi);
    }

    #[test]
    fn write_timestamp_emits_datetime_prefix() {
        let mut out = String::new();
        write_timestamp(&mut out).unwrap();
        assert!(out.len() >= 20);
        // 形如 YYYY-MM-DD HH:MM:SS.mmm
        assert_eq!(out.chars().nth(4), Some('-'));
        assert_eq!(out.chars().nth(10), Some(' '));
    }

    #[test]
    fn log_config_serde_roundtrip() {
        let cfg = LogConfig {
            level: "debug".into(),
            console_enabled: true,
            log_dir: Some("/tmp/fluen-logs".into()),
            max_entries_per_file: 2048,
            max_file_count: 32,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: LogConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.level, "debug");
        assert!(parsed.console_enabled);
        assert_eq!(parsed.log_dir.as_deref(), Some("/tmp/fluen-logs"));
        assert_eq!(parsed.max_entries_per_file, 2048);
        assert_eq!(parsed.max_file_count, 32);
    }

    #[test]
    fn log_config_deserialize_missing_fields_uses_defaults() {
        let json = r#"{}"#;
        let cfg: LogConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.level, "info");
        assert!(!cfg.console_enabled);
        assert!(cfg.log_dir.is_none());
        assert_eq!(cfg.max_entries_per_file, DEFAULT_MAX_ENTRIES_PER_FILE);
        assert_eq!(cfg.max_file_count, DEFAULT_MAX_FILE_COUNT);
    }

    #[test]
    fn log_config_validate_ok() {
        assert!(LogConfig::default().validate().is_ok());
    }

    #[test]
    fn log_config_validate_entries_out_of_range() {
        let mut cfg = LogConfig::default();
        cfg.max_entries_per_file = MIN_MAX_ENTRIES_PER_FILE - 1;
        assert!(cfg.validate().is_err());

        cfg.max_entries_per_file = MAX_MAX_ENTRIES_PER_FILE + 1;
        assert!(cfg.validate().is_err());

        cfg.max_entries_per_file = MIN_MAX_ENTRIES_PER_FILE;
        assert!(cfg.validate().is_ok());

        cfg.max_entries_per_file = MAX_MAX_ENTRIES_PER_FILE;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn log_config_validate_file_count_out_of_range() {
        let mut cfg = LogConfig::default();
        cfg.max_file_count = 0;
        assert!(cfg.validate().is_err());

        cfg.max_file_count = MAX_MAX_FILE_COUNT + 1;
        assert!(cfg.validate().is_err());

        cfg.max_file_count = MIN_MAX_FILE_COUNT;
        assert!(cfg.validate().is_ok());

        cfg.max_file_count = MAX_MAX_FILE_COUNT;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn log_config_validate_empty_log_dir() {
        let mut cfg = LogConfig::default();
        cfg.log_dir = Some("   ".into());
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn cleanup_excess_logs_removes_oldest() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_log_test_{}_excess",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        // 创建 5 个日志文件（时间戳递增）
        for i in 0..5 {
            let name = format!("{LOG_FILE_PREFIX}2026010100000{}.log", i);
            std::fs::write(tmp.join(name), "x").unwrap();
        }
        // 非日志文件（应保留）
        std::fs::write(tmp.join("notes.txt"), "notes").unwrap();

        cleanup_excess_logs(&tmp, 3);

        let remaining: Vec<String> = std::fs::read_dir(&tmp)
            .unwrap()
            .flatten()
            .filter_map(|e| e.file_name().to_string_lossy().into_owned().into())
            .collect();
        let log_count = remaining
            .iter()
            .filter(|n| n.starts_with(LOG_FILE_PREFIX) && n.ends_with(".log"))
            .count();
        assert_eq!(log_count, 3, "应保留 3 个日志文件");
        assert!(
            remaining.contains(&"notes.txt".to_string()),
            "非日志文件应保留"
        );
        // 最旧的应被删除
        assert!(
            !remaining.contains(&format!("{LOG_FILE_PREFIX}20260101000000.log")),
            "最旧日志应被删除"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn cleanup_excess_logs_noop_when_under_limit() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_log_test_{}_noop",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        std::fs::write(tmp.join(format!("{LOG_FILE_PREFIX}20260101000000.log")), "x").unwrap();
        cleanup_excess_logs(&tmp, 64);
        let count = std::fs::read_dir(&tmp).unwrap().count();
        assert_eq!(count, 1);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rolling_writer_creates_session_file() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_log_test_{}_rolling",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let mut writer = RollingFileWriter::new(&tmp, 3).unwrap();
        writer.write_all(b"a\n").unwrap();
        writer.write_all(b"b\n").unwrap();
        writer.write_all(b"c\n").unwrap();
        // 第 3 条后应滚动，第 4 条写入新文件
        writer.write_all(b"d\n").unwrap();
        writer.flush().unwrap();

        let files: Vec<String> = std::fs::read_dir(&tmp)
            .unwrap()
            .flatten()
            .filter_map(|e| e.file_name().to_string_lossy().into_owned().into())
            .collect();
        assert_eq!(files.len(), 2, "应创建 2 个文件（滚动一次）");
        assert!(
            files.iter().any(|f| f.ends_with("-1.log")),
            "应存在滚动后缀 -1 的文件: {:?}",
            files
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rolling_writer_handles_no_newline() {
        let tmp = std::env::temp_dir().join(format!(
            "fluen_log_test_{}_nonl",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let mut writer = RollingFileWriter::new(&tmp, 2).unwrap();
        // 无换行符的写入不计为条目
        writer.write_all(b"partial").unwrap();
        writer.write_all(b" line\n").unwrap();
        writer.flush().unwrap();

        let count = std::fs::read_dir(&tmp).unwrap().count();
        assert_eq!(count, 1, "1 条记录不应滚动");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
