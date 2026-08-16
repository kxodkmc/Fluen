//! # motis_chat
//!
//! Motis 聊天模块——驱动 Motis 宠物助手的对话流程。
//!
//! 基于 confluent 运行时，将用户消息与历史记录传入 LLM，
//! 以流式事件（思考增量、文本增量、工具调用、完成、错误）的形式
//! 通过 Tauri 事件推送到前端。
//!
//! ## 模块结构
//!
//! | 子模块 | 职责 |
//! |--------|------|
//! | [`error`] | 统一错误类型 [`MotisChatError`] |
//! | [`events`] | Tauri 事件名常量与 payload 序列化结构 |
//! | [`prompt`] | Motis 模块化提示词——人设、Profile 定义与上下文注入器 |
//! | [`runtime`] | confluent 运行时构建（provider/model 解析 + 提示词装配 + 能力装配） |
//! | [`commands`] | Tauri commands（`motis_chat_send` / `motis_chat_cancel`） |
//!
//! ## 能力扩展
//!
//! 不在本模块硬编码应用操作工具，所有能力扩展通过 confluent 适配器装配：
//!
//! | 能力 | 适配器 | 装配条件 |
//! |------|--------|----------|
//! | MCP | `confluent::adapters::mcp` | `MascotConfig.mcp_enabled = true` |
//! | Skills | `confluent::adapters::skills` | `MascotConfig.skills_enabled = true` |
//! | 函数调用 | `confluent::ToolKit` | `MascotConfig.function_calling_enabled = true` |

pub mod approval;
pub mod commands;
pub mod error;
pub mod events;
pub mod prompt;
pub mod runtime;

pub use commands::MotisChatState;
