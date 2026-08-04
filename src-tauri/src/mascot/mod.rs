//! # mascot
//!
//! 宠物助手模块——管理标题栏宠物助手的全局配置与运行时数据。
//!
//! 职责单一：仅存储宠物助手相关的配置（启用状态、人格、能力开关等）
//! 与运行时数据（心情、好感度），不涉及 LLM 提供商配置或应用级偏好。
//!
//! ## 模块结构
//!
//! | 子模块 | 职责 |
//! |--------|------|
//! | [`model`] | 纯数据模型与校验逻辑（`MascotConfig`、`MascotData`、`Mood`） |
//! | [`error`] | 统一错误类型 [`MascotError`] |
//! | [`storage`] | 跨平台路径解析与文件读写（原子写入 + 内存缓存） |
//! | [`commands`] | Tauri commands，供前端调用 |
//!
//! ## 存储位置
//!
//! | 文件 | 路径 |
//! |------|------|
//! | `mascot_config.json` | `fluen_config_dir`（配置目录） |
//! | `mascot_data.json` | `fluen_data_dir`（数据目录） |
//!
//! ## 性能优化
//!
//! [`storage::MascotConfigStorage`] 与 [`storage::MascotDataStorage`] 均内置
//! `RwLock` 内存缓存，首次读取后缓存于内存，后续读取直接返回缓存副本。

pub mod commands;
pub mod error;
pub mod model;
pub mod storage;
