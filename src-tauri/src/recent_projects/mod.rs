//! # recent_projects
//!
//! 最近打开文章项目模块——记录用户打开过的文章项目，
//! 用于应用启动时在欢迎页展示快捷入口。
//!
//! ## 模块结构
//!
//! | 子模块 | 职责 |
//! |--------|------|
//! | [`model`] | 纯数据模型与校验逻辑（[`RecentProjectsData`]、[`RecentProjectEntry`]） |
//! | [`error`] | 统一错误类型 [`RecentProjectsError`] |
//! | [`storage`] | 跨平台路径解析与文件读写（原子写入 + 内存缓存） |
//! | [`commands`] | Tauri commands，供前端调用 |
//!
//! ## 存储位置
//!
//! | 平台 | 路径 |
//! |------|------|
//! | macOS | `~/Library/Application Support/com.wppcp.fluen/recent_projects.json` |
//! | Windows | `%LOCALAPPDATA%\Fluen\recent_projects.json` |
//! | Linux | `~/.local/share/Fluen/recent_projects.json`（或 `$XDG_DATA_HOME/Fluen/`） |
//!
//! ## 性能优化
//!
//! [`storage::RecentProjectsStorage`] 内置 `RwLock` 内存缓存，
//! 首次读取后缓存于内存，后续读取直接返回缓存副本，写入时同步更新缓存。

pub mod commands;
pub mod error;
pub mod model;
pub mod storage;
