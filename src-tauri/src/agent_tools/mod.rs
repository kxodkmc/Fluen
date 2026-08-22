//! # agent_tools
//!
//! 应用级智能体工具集——为 referee 智能体提供读写 Fluen 业务数据的工具。
//!
//! 区别于通用文件工具，本模块的工具与 Fluen 项目结构强耦合
//! （如读取当前论文内容），由应用层实现并注册到
//! [`ToolRegistry`](referee_ai::tool::ToolRegistry)。
//!
//! ## 模块结构
//!
//! | 子模块 | 职责 |
//! |--------|------|
//! | [`parse`] | 论文内容解析纯函数层（大纲树、章节提取） |
//! | [`paper`] | [`paper::PaperContentTool`]（论文内容读取） |
//! | [`file`] | [`file::ProjectFileTool`]（项目内文件读写/编辑） |
//! | [`manuscript`] | [`manuscript::ManuscriptEditTool`]（论文正文写入，格式校验+同步） |
//! | [`literature`] | [`literature::LiteratureSearchTool`]（文献知识库搜索，混合检索 top4） |
//!
//! ## 示例
//!
//! ```no_run
//! use std::sync::Arc;
//!
//! let tool = Arc::new(crate::agent_tools::paper::PaperContentTool::new(
//!     "/path/to/project".into(),
//! ));
//! // 注册：FluenRuntimeBuilder::new(provider).with_tool(tool).build()
//! ```

pub mod file;
pub mod literature;
pub mod manuscript;
pub mod paper;
pub mod parse;
