//! # agent_tools
//!
//! 应用级智能体工具集——为 referee 智能体提供读写 Fluen 业务数据的工具。
//!
//! 区别于通用文件工具，本模块的工具与 Fluen 项目结构强耦合
//! （如读取当前论文内容），由应用层实现并注册到
//! [`ToolRegistry`](referee_ai::tool::ToolRegistry)。
//!
//! ## 工具分层（检索通道的唯一性约定）
//!
//! ```text
//! 语义层（懂论文结构）   paper_outline / paper_section / manuscript
//! 业务层（Fluen 领域）   literature_search / delegate_agent（见 motis_chat::delegate）
//! 原语层（通用文件门面） project_read / project_write / project_edit
//! 知识库管线专用         knowledge_* 前缀工具族（fluen-knowledge，仅用于后台
//!                        知识库构建智能体与 MCP server，不进入用户侧聊天运行时；
//!                        literature_search 是聊天运行时的唯一检索入口）
//! ```
//!
//! ## 模块结构
//!
//! | 子模块 | 职责 |
//! |--------|------|
//! | [`parse`] | 论文内容解析纯函数层（大纲树、章节提取） |
//! | [`paper`] | [`paper::outline::PaperOutlineTool`] / [`paper::section::PaperSectionTool`]（论文大纲与章节读取，经 referee `read`） |
//! | [`project`] | 项目内通用文件读写：[`project::read::ProjectReadTool`] / [`project::write::ProjectWriteTool`] / [`project::edit::ProjectEditTool`]（经 referee 原语，写需审批；正文 `main.md` 写保护） |
//! | [`manuscript`] | [`manuscript::ManuscriptEditTool`]（论文正文唯一写入通道，格式校验+同步） |
//! | [`literature`] | [`literature::LiteratureSearchTool`]（文献知识库搜索，混合检索 top4；用户侧唯一检索入口） |
//! | [`assemble`] | 运行时装配辅助：各运行时共享的工具注册组合（含审批包装） |
//!
//! ## 示例
//!
//! ```no_run
//! use std::sync::Arc;
//!
//! let tool = Arc::new(crate::agent_tools::paper::outline::PaperOutlineTool::new(
//!     "/path/to/project".into(),
//! ));
//! // 注册：FluenRuntimeBuilder::new(provider).with_tool(tool).build()
//! ```

pub mod assemble;
pub mod literature;
pub mod manuscript;
pub mod paper;
pub mod parse;
pub mod project;
