//! # knowledge_builder
//!
//! 知识库构建业务层——驱动 AI 从文献中提取结构化知识并写入知识库。
//!
//! ## 两阶段流水线（V2.1）
//!
//! 核心是 [`pipeline::build`]，采用两阶段设计：
//!
//! - **阶段 1（Planning）**：AI 阅读文献全文 + Index 快照（全局去重视图），
//!   产出结构化 [`ExtractionPlan`]（通过 `submit_plan` 工具捕获）。
//! - **阶段 2（Execution）**：按计划逐条创建 summary / concept / entity 条目，
//!   每条创建前做 L2 混合检索查重，每次 `runtime.run()` 后从真实 usage 更新会话预算。
//!   最后单独执行 `EstablishingRelations` 阶段。
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
//! ## V2.1 新增模块
//!
//! | 子模块 | 职责 | 状态 |
//! |--------|------|------|
//! | [`index_snapshot`] | 从 index.md 实时解析紧凑快照（屏蔽 ID） | 无状态 |
//! | [`context_budget`] | 上下文预算计算（128K 门槛 + 75% 上限） | 无状态 |
//! | [`error_classify`] | LLM 错误分类（瞬态/结构性）+ 重试策略 | 无状态 |
//! | [`session`] | 跨论文会话管理（runtime 复用 + 真实 usage 跟踪） | 有状态 |
//!
//! ## 模块结构
//!
//! | 子模块 | 职责 |
//! |--------|------|
//! | [`types`] | 核心数据类型（选项、阶段、checkpoint、计划） |
//! | [`error`] | 统一错误类型 [`KnowledgeBuilderError`] |
//! | [`config`] | 全局配置 [`KnowledgeBuildConfig`] |
//! | [`events`] | Tauri 事件常量与 payload 结构 |
//! | [`prompts`] | 两阶段 prompt 模板与渲染（V2.1 含 candidates 渲染） |
//! | [`llm_helper`] | LLM 客户端与运行时构建（含 `submit_plan` 工具 + UsageObserver） |
//! | [`chat_retry`] | 工具凭证催促重试（capture 为空时强制模型回到工具调用路径） |
//! | [`pipeline`] | 两阶段流水线主入口（V2.1 注入快照 + L2 检索 + usage 更新） |
//! | [`commands`] | Tauri commands，供前端调用 |

pub mod chat_retry;
pub mod commands;
pub mod config;
pub mod context_budget;
pub mod error;
pub mod error_classify;
pub mod events;
pub mod index_snapshot;
pub mod llm_helper;
pub mod pipeline;
pub mod prompts;
pub mod session;
pub mod types;

pub use error::KnowledgeBuilderError;
#[allow(unused_imports)]
pub use types::{
    BuildStage, ExtractionPlan, KnowledgeBuildCheckpoint, KnowledgeBuildOptions, PlannedEntry,
};
