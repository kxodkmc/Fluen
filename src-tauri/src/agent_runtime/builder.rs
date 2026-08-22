//! FluenRuntime 构建器。
//!
//! 提供链式 API 组装引擎所需的全部组件：
//! `LLMProvider` → `EngineConfig` → 可选工具 → `FluenRuntime`
//!
//! ## 使用示例
//!
//! ```no_run
//! use std::sync::Arc;
//! use fluen_lib::agent_runtime::FluenRuntimeBuilder;
//! use fluen_lib::llm_chat;
//! use fluen_lib::llm_config::model::LlmConfig;
//!
//! fn build(llm: &LlmConfig) -> anyhow::Result<()> {
//!     let provider = llm_chat::resolve_and_build(None, None, llm)?;
//!     let runtime = FluenRuntimeBuilder::new(provider).build();
//!     Ok(())
//! }
//! ```

use referee_ai::engine::EngineConfig;
use referee_ai::provider::LLMProvider;
use referee_ai::tool::{Tool, ToolExecutor, ToolRegistry};

use super::FluenRuntime;

/// FluenRuntime 构建器。
///
/// 最少必须提供 `LLMProvider`；其余取安全默认（引擎配置 / 工具能力）。
pub struct FluenRuntimeBuilder {
    provider: Option<std::sync::Arc<dyn LLMProvider>>,
    config: EngineConfig,
    tools: Option<ToolRegistry>,
    executor: Option<ToolExecutor>,
}

impl std::fmt::Debug for FluenRuntimeBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FluenRuntimeBuilder")
            .field("has_provider", &self.provider.is_some())
            .field("config", &self.config)
            .field("has_tools", &self.tools.is_some())
            .finish()
    }
}

impl Default for FluenRuntimeBuilder {
    fn default() -> Self {
        Self {
            provider: None,
            config: EngineConfig::default(),
            tools: None,
            executor: None,
        }
    }
}

impl FluenRuntimeBuilder {
    /// 创建空构建器。
    pub fn new(provider: std::sync::Arc<dyn LLMProvider>) -> Self {
        Self {
            provider: Some(provider),
            ..Default::default()
        }
    }

    /// 覆盖引擎配置（会话 / 预算 / 缓存 / 并发上限等）。
    pub fn with_config(mut self, config: EngineConfig) -> Self {
        self.config = config;
        self
    }

    /// 启用工具能力（工具注册表 + 执行器）。
    ///
    /// 未调用此方法时引擎不具备工具调用能力。
    pub fn with_tools(mut self, registry: ToolRegistry, executor: ToolExecutor) -> Self {
        self.tools = Some(registry);
        self.executor = Some(executor);
        self
    }

    /// 注册单个工具（链式，可多次调用；首次调用自动创建默认注册表与执行器）。
    ///
    /// 需自定义执行器配置（如审批场景调大 `tool_timeout`）时，
    /// 先调用 [`with_tools`](Self::with_tools) 再逐个注册。
    ///
    /// 注册失败（重名 / 超出注册表上限）视为装配期编程错误，直接 panic。
    pub fn with_tool(mut self, tool: std::sync::Arc<dyn Tool>) -> Self {
        let registry = self
            .tools
            .take()
            .unwrap_or_else(ToolRegistry::with_defaults);
        if self.executor.is_none() {
            self.executor = Some(ToolExecutor::with_defaults());
        }
        registry
            .register(tool)
            .expect("FluenRuntimeBuilder: tool registration failed");
        self.tools = Some(registry);
        self
    }

    /// 构建运行时。
    ///
    /// 未设置 provider 时 panic（编程错误，非运行时错误）。
    pub fn build(self) -> FluenRuntime {
        let provider = self.provider.expect("FluenRuntimeBuilder: provider is required");

        let mut engine = referee_ai::Engine::new(provider, self.config);

        if let (Some(registry), Some(executor)) = (self.tools, self.executor) {
            engine = engine.with_tools(registry, executor);
        }

        FluenRuntime::from_engine(engine)
    }
}
