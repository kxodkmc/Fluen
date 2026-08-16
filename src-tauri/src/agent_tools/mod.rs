//! # agent_tools
//!
//! 应用级智能体工具集——为 confluent 智能体提供读写 Fluen 业务数据的工具。
//!
//! 区别于 confluent 内置 ToolKit（通用文件/命令工具），本模块的工具与
//! Fluen 项目结构强耦合（如读取当前论文内容），由应用层实现并注册。
//!
//! ## 模块结构
//!
//! | 子模块 | 职责 |
//! |--------|------|
//! | [`parse`] | 论文内容解析纯函数层（大纲树、章节提取），无 confluent 依赖 |
//! | [`paper`] | [`paper::PaperContentTool`] 及其 Provider |
//! | [`file`] | [`file::ProjectFileTool`] 及其 Provider（项目内文件读写/编辑） |
//! | [`manuscript`] | [`manuscript::ManuscriptEditTool`] 及其 Provider（论文正文写入，格式校验+同步） |
//! | [`literature`] | [`literature::LiteratureSearchTool`] 及其 Provider（文献知识库搜索，混合检索 top4） |
//!
//! ## 示例
//!
//! ```no_run
//! use std::sync::Arc;
//! use confluent::agent_runtime::ToolProvider;
//!
//! let provider = Arc::new(crate::agent_tools::paper::PaperContentToolProvider::new(
//!     "/path/to/project".into(),
//! ));
//! // provider 注册到 ConfluentRuntimeBuilder::with_tool_provider
//! ```

pub mod file;
pub mod literature;
pub mod manuscript;
pub mod paper;
pub mod parse;
