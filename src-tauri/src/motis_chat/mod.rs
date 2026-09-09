//! # motis_chat
//!
//! Motis 聊天模块——驱动 Motis **总督角色**的对话与任务编排流程。
//!
//! 基于 referee [`FluenRuntime`](crate::agent_runtime::FluenRuntime)，
//! 将用户消息与历史记录传入 LLM，以流式事件（思考增量、文本增量、
//! 工具调用、完成、错误）的形式通过 Tauri 事件推送到前端。
//!
//! ## 总督角色
//!
//! Motis 是总督角色——他不直接负责编写、计算等具体任务，而是：
//! 1. **理解任务**：接收用户需求，分析意图
//! 2. **派发任务**：通过 `delegate_agent` 工具调用合适的子智能体执行
//! 3. **汇总结果**：收集子智能体返回结果，汇总后回复用户
//! 4. **助手操作**：直接处理简单操作（主题切换、语言切换等）
//!
//! ## 模块结构
//!
//! | 子模块 | 职责 |
//! |--------|------|
//! | [`error`] | 统一错误类型 [`MotisChatError`] |
//! | [`events`] | Tauri 事件名常量与 payload 序列化结构 |
//! | [`context_usage`] | 上下文用量分类估算（`motis:context-usage` 数据源） |
//! | [`timeouts`] | 超时分层单一事实来源（引擎 / HTTP / RPC / 执行器） |
//! | [`prompt`] | Motis 系统提示词（总督角色文案，纯函数组装） |
//! | [`approval`] | 工具审批器 [`MotisApprover`]（实现 `Approver` trait） |
//! | [`approval_diff`] | 审批前预演算行级 diff（弹窗展示真实变更） |
//! | [`runtime`] | referee 运行时构建（provider 解析 + 工具装配 + 内核注入） |
//! | [`commands`] | Tauri commands（`motis_chat_send` / `motis_chat_cancel`） |
//! | [`agents`] | 子智能体注册表——定义可调度的子智能体清单与构建逻辑 |
//! | [`federation`] | 子智能体联邦——referee Kernel + AgentRuntime 拓扑，运行时按指纹复用 |
//! | [`artifact_store`] | 项目级持久化成果板——`ArtifactStore` 自研实现（项目作用域 + 落盘） |
//! | [`delegate`] | 子智能体委派工具——经内核 RPC 把任务派发为全新子会话 |
//! | [`agent_reporter`] | 子智能体事件上报器——委派生命周期与内部工具调用透传前端 |
//!
//! 流式消费与事件映射由 [`crate::chat_bridge`] 共享层提供。

pub mod agent_reporter;
pub mod agents;
pub mod approval;
pub mod approval_diff;
pub mod artifact_store;
pub mod commands;
pub mod context_usage;
pub mod delegate;
pub mod error;
pub mod events;
pub mod federation;
pub mod prompt;
pub mod runtime;
pub mod timeouts;

pub use commands::MotisChatState;
pub use federation::FederationPool;
