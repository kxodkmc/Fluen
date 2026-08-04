//! # task_queue
//!
//! 项目级持久化串行任务队列。
//!
//! 用于异步长任务（如知识库构建、翻译、OCR 纠错）的串行执行、
//! 中断接续与取消恢复。每个项目独立一份 `data/task-queue.json`，
//! 单项目内任务严格串行，跨项目隔离。
//!
//! ## 模块结构
//!
//! | 子模块 | 职责 |
//! |--------|------|
//! | [`error`] | 统一错误类型 [`TaskQueueError`] |
//! | [`types`] | 任务记录、状态、种类等核心数据类型 |
//! | [`store`] | JSON 持久化层 [`TaskStore`] |
//! | [`runner`] | 单 worker 串行执行器 [`TaskRunner`] |
//! | [`state`] | 全局状态 [`TaskQueueState`]（管理 runner 生命周期） |
//! | [`recovery`] | 启动时中断接续 [`recover_on_startup`] |
//! | [`commands`] | Tauri commands，供前端调用 |
//!
//! ## 持久化
//!
//! 每个项目的任务队列独立持久化到 `{project}/data/task-queue.json`，
//! 采用原子写入（先写 `.tmp` 再 `rename`）。
//!
//! ## 中断接续
//!
//! - 任务执行中持续更新 `checkpoint` 字段，记录进度。
//! - App 异常退出后重启，[`recovery::recover_on_startup`] 将 Running 任务重置为 Pending，
//!   并触发 [`runner::TaskRunner`] 从 checkpoint 继续。
//! - 状态机驱动的 checkpoint 保证 relations 等关键阶段不会被跳过（杜绝孤儿条目）。

pub mod commands;
pub mod error;
pub mod recovery;
pub mod runner;
pub mod state;
pub mod store;
pub mod types;

pub use error::TaskQueueError;
pub use state::TaskQueueState;
pub use store::TaskStore;
pub use types::{TaskKind, TaskQueueFile, TaskRecord, TaskStatus};
