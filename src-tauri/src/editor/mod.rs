//! # editor
//!
//! 编辑器核心模块——驱动论文章节文本的编辑、历史、渲染与命令暴露。
//!
//! 基于 `fluen-markup` 提供 MD→HTML 渲染能力，维护双栈历史与编辑引擎，
//! 并通过 Tauri commands 向前端暴露最小接口。
//!
//! ## 模块结构
//!
//! | 子模块 | 职责 |
//! |--------|------|
//! | [`config`] | 编辑器配置 [`EditorConfig`] |
//! | [`error`] | 统一错误类型 [`EditorError`] |
//! | [`edit`] | Edit 枚举（编辑操作描述） |
//! | [`engine`] | 编辑引擎 [`EditorEngine`]（待实现） |
//! | [`history`] | 双栈历史记录（待实现） |
//! | [`render`] | `fluen-markup` 渲染封装（待实现） |
//! | [`commands`] | Tauri commands 薄封装（待实现） |
//!
//! [`EditorConfig`]: config::EditorConfig
//! [`EditorError`]: error::EditorError

pub mod commands;
pub mod config;
pub mod edit;
pub mod engine;
pub mod error;
pub mod history;
pub mod render;
