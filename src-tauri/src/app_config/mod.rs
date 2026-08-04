//! # app_config
//!
//! App 用户配置模块——管理应用级别的用户偏好设置（主题、语言等）。
//!
//! 与 [`crate::llm_config`] 分离，职责单一：仅存储应用级偏好，
//! 不涉及 LLM 提供商与模型配置。
//!
//! ## 模块结构
//!
//! | 子模块 | 职责 |
//! |--------|------|
//! | [`model`] | 纯数据模型与校验逻辑（`AppConfig`、`ThemeMode`、`Language`） |
//! | [`error`] | 统一错误类型 [`AppConfigError`] |
//! | [`storage`] | 跨平台路径解析与文件读写（原子写入 + 内存缓存） |
//! | [`commands`] | Tauri commands，供前端调用 |
//!
//! ## 存储位置
//!
//! | 平台 | 路径 |
//! |------|------|
//! | macOS | `~/Library/Preferences/Fluen/app_config.json` |
//! | Windows | `%APPDATA%\Fluen\app_config.json` |
//! | Linux | `~/.config/Fluen/app_config.json` |
//!
//! ## 性能优化
//!
//! [`storage::AppConfigStorage`] 内置 `RwLock` 内存缓存，
//! 首次读取后缓存于内存，后续读取直接返回缓存副本。

pub mod commands;
pub mod error;
pub mod model;
pub mod storage;
