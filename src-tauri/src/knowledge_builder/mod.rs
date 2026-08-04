//! # knowledge_builder
//!
//! 知识库构建业务层——驱动 AI 从文献中提取结构化知识并写入知识库。
//!
//! ## 两阶段流水线
//!
//! 核心是 [`pipeline::build`]，采用两阶段设计：
//!
//! - **阶段 1（Planning）**：AI 阅读文献全文，先 `query` 已有知识库做去重，
//!   产出结构化 [`ExtractionPlan`]（通过 `submit_plan` 工具捕获）。
//! - **阶段 2（Execution）**：按计划逐条创建 summary / concept / entity 条目，
//!   每条独立 LLM 调用，可中断恢复。最后单独执行 `EstablishingRelations` 阶段。
//!
//! ## 状态机
//!
//! 通过 [`BuildStage`] 状态机驱动恢复，保证 relations 阶段不被跳过（杜绝孤儿条目）：
//!
//! ```text
//! Planning → CreatingSummary → CreatingConcepts
//!         → CreatingEntities → EstablishingRelations → Done
//! ```
//!
//! ## 模块结构
//!
//! | 子模块 | 职责 |
//! |--------|------|
//! | [`types`] | 核心数据类型（选项、阶段、checkpoint、计划） |
//! | [`error`] | 统一错误类型 [`KnowledgeBuilderError`] |
//! | [`config`] | 全局配置 [`KnowledgeBuildConfig`] |
//! | [`events`] | Tauri 事件常量与 payload 结构 |
//! | [`prompts`] | 两阶段 prompt 模板与渲染 |
//! | [`llm_helper`] | LLM 客户端与运行时构建（含 `submit_plan` 工具） |
//! | [`pipeline`] | 两阶段流水线主入口 |
//! | [`commands`] | Tauri commands，供前端调用 |

pub mod commands;
pub mod config;
pub mod error;
pub mod events;
pub mod llm_helper;
pub mod pipeline;
pub mod prompts;
pub mod types;

pub use error::KnowledgeBuilderError;
pub use types::{
    BuildStage, ExtractionPlan, KnowledgeBuildCheckpoint, KnowledgeBuildOptions, PlannedEntry,
};
