//! fluen-knowledge：文献知识库基础设施模块。
//!
//! 严格依据 wiki.md 规范，提供三类条目（summary / concept / entity）、
//! 双索引（index.md + index.db）、三种检索（keyword / semantic / hybrid）。
//!
//! 本模块不依赖 server / agent，可被任意上层调用。
//!
//! ## 快速上手
//! ```no_run
//! use fluen_knowledge::wiki;
//! use std::path::Path;
//!
//! # fn main() -> anyhow::Result<()> {
//! // 初始化
//! wiki::init_wiki(Path::new("project/references"))?;
//!
//! // 打开 DB 连接
//! let conn = wiki::open_wiki(Path::new("project/references"))?;
//!
//! // 列出所有条目
//! let entries = wiki::list_entries(&conn)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## 可选特性（feature flags）
//!
//! | Feature | 说明 |
//! |---------|------|
//! | `async` | 异步知识库句柄 [`async_kb::AsyncKnowledgeBase`] |
//! | `mcp-server` | MCP server（JSON-RPC 2.0 over stdio）[`mcp_server::KnowledgeMcpServer`] |
//! | `tools` | confluent agent_runtime 工具适配 [`tools::KnowledgeToolProvider`] |
//!
//! ### MCP server 示例
//!
//! ```no_run
//! # #[cfg(feature = "mcp-server")]
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! use fluen_knowledge::async_kb::AsyncKnowledgeBase;
//! use fluen_knowledge::config::KnowledgeConfig;
//! use fluen_knowledge::mcp_server::KnowledgeMcpServer;
//!
//! let kb = AsyncKnowledgeBase::open("references")?;
//! let server = KnowledgeMcpServer::new(kb, KnowledgeConfig::default());
//! server.run_stdio().await?;
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "mcp-server"))]
//! # fn main() {}
//! ```
//!
//! ### confluent agent_runtime 工具注入示例
//!
//! ```no_run
//! # #[cfg(feature = "tools")]
//! # async fn example() -> anyhow::Result<()> {
//! use fluen_knowledge::async_kb::AsyncKnowledgeBase;
//! use fluen_knowledge::tools::KnowledgeToolProvider;
//! use confluent::agent_runtime::ToolRegistry;
//!
//! let kb = AsyncKnowledgeBase::open("references")?;
//! let provider = KnowledgeToolProvider::with_defaults(kb);
//! let registry = ToolRegistry::new();
//! registry.register_provider(&provider).await;
//! # Ok(())
//! # }
//! ```

pub mod db;
pub mod error;
pub mod frontmatter;
pub mod id;
pub mod index_md;
pub mod indexer;
pub mod markdown;
pub mod search;
pub mod types;
pub mod util;
pub mod wiki;

/// 配置系统：MCP server 与工具层的共享配置。
pub mod config;

pub use error::{KnowledgeError, Result as KnowledgeResult};
pub use types::*;

/// 重导出 SQLite 连接类型（供上层工具/处理器使用，避免直接依赖 rusqlite）。
pub use rusqlite::Connection;

// ── Feature-gated 模块 ──

/// 异步知识库句柄（`async` feature）。
///
/// 提供 `Arc<Mutex<Connection>>` + `spawn_blocking` 的异步访问层，
/// 支持 embedding provider 注入、并发安全。
#[cfg(feature = "async")]
pub mod async_kb;

/// MCP server（`mcp-server` feature）。
///
/// 将知识库操作暴露为标准 MCP 工具，通过 JSON-RPC 2.0 over stdio
/// 与 MCP 客户端通信。
#[cfg(feature = "mcp-server")]
pub mod mcp_server;

/// confluent agent_runtime 工具适配（`tools` feature）。
///
/// 将知识库操作适配为 `Tool` / `ToolProvider`，可直接注册到
/// 智能体的 `ToolRegistry`。
#[cfg(feature = "tools")]
pub mod tools;
