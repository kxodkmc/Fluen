//! # 学术助手（Academic Assistant）
//!
//! 右侧面板"学术助手"智能体——职责是**根据要求撰写格式规范的文章内容**。
//!
//! ## 架构
//!
//! 复用 Motis 聊天的 confluent 运行时架构，差异化在：
//!
//! | 维度 | Motis 宠物助手 | 学术助手 |
//! |------|----------------|----------|
//! | 模型来源 | MascotConfig 优先，回退全局 | 直接使用 LLM 全局激活项 |
//! | 提示词 | 宠物人格 + 学术辅助 | 学术写作 profile（fluen-markup 规范） |
//! | 工具 | MCP / Skills / 通用 ToolKit | 论文写作工具（paper_content / manuscript / project_file + 只读内置） |
//! | 审批 | 写操作弹窗确认 | 写操作弹窗确认（同一审批通道） |
//! | 事件前缀 | `motis:*` | `ai-assistant:*` |
//!
//! ## 模块
//!
//! | 文件 | 职责 |
//! |------|------|
//! | [`prompt`] | 学术写作 prompt profile + 上下文注入器 |
//! | [`runtime`] | confluent 运行时装配（工具 + 审批） |
//! | [`commands`] | Tauri commands（send / cancel）+ 事件常量 |
//! | [`error`] | 错误类型 |

pub mod commands;
pub mod error;
pub mod prompt;
pub mod runtime;

pub use commands::{AiAssistantState, EVENT_APPROVAL_REQUEST};
