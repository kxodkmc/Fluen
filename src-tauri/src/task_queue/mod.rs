//! # task_queue
//!
//! 项目级持久化任务队列。
//!
//! 用于异步长任务（如知识库构建、文献导入、翻译）的排队执行、
//! 中断接续与取消恢复。每个项目独立一份 `data/task-queue.json`。
//!
//! ## 队列模型
//!
//! - **按种类分队列并行**：同一项目内，不同任务种类（如
//!   `knowledge_build` 与 `reference_import`）各有独立的串行 runner，
//!   并行执行互不阻塞；同一种类内部严格 FIFO 串行（先到先导）。
//! - **跨项目隔离**：不同项目的队列互不影响。
//! - **写并发安全**：所有"读-改-写"操作经项目级锁串行化
//!   （见 [`store::project_lock`]），多 runner 并行写同一队列文件安全。
//!
//! ## 模块结构
//!
//! | 子模块 | 职责 |
//! |--------|------|
//! | [`error`] | 统一错误类型 [`TaskQueueError`] |
//! | [`types`] | 任务记录、状态、种类等核心数据类型 |
//! | [`store`] | JSON 持久化层 [`TaskStore`]（含项目级写锁） |
//! | [`runner`] | 单种类串行执行器 [`TaskRunner`] |
//! | [`state`] | 全局状态 [`TaskQueueState`]（管理各项目各种类 runner 生命周期） |
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
//!   并按种类触发对应 runner 从 checkpoint 继续。
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
