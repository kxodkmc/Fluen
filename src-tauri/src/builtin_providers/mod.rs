//! 内置免费模型提供商（带优先级降级）。
//!
//! 设计目标：
//! - 开箱即用：无需用户配置即可提供 Embedding 等基础能力
//! - 高可用：同一类型（Embedding/LLM/OCR）可注册多个提供商，按优先级逐个尝试，
//!   单个失败自动降级到下一个
//! - 易扩展：新增提供商只需实现对应 trait 并加入路由器列表
//!
//! 当前内置：
//! - Embedding：ModelScope Qwen3-Embedding-4B（OpenAI 兼容 API）
//!
//! # 架构
//!
//! ```text
//! BuiltinEmbeddingProvider (trait)          ← 各提供商实现
//!     └── OpenAiEmbeddingProvider           ← 通用 OpenAI 兼容提供商
//!         ├── modelscope() 预设             ← 内置免费
//!         └── new() 自定义                  ← 用户配置的第三方
//!
//! EmbeddingRouter                           ← 路由器，实现 KnowledgeEmbedding
//!     └── providers: Vec<Arc<dyn BuiltinEmbeddingProvider>>  (按 priority 排序)
//!         调用时逐个尝试，失败降级
//! ```

pub mod embedding;
pub mod obfuscation;
pub mod openai_provider;

pub use embedding::{BuiltinEmbeddingProvider, EmbeddingRouter};
pub use openai_provider::OpenAiEmbeddingProvider;
