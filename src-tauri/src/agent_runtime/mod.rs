//! # FluenRuntime — 基于 referee Engine 的统一运行时封装
//!
//! 在 P0 阶段提供最小可用的引擎创建 + Session 管理 + 流式接口，
//! 供 Motis 聊天 / 学术助手 / 知识库构建等模块统一使用。
//!
//! ## 设计目标
//! - **薄封装**：不引入额外抽象层，直接转发 Engine API
//! - **零业务耦合**：不预设 prompt 策略、工具集；审批机制以通用装饰器提供
//!   （[`approval::ApprovalGuard`]），策略由业务侧实现
//! - **类型安全**：对外暴露 referee 原生类型（StreamChunk / ChatResponse 等），
//!   上层映射到 Tauri 事件时做转换
//!
//! ## 模块结构
//! | 子模块 | 职责 |
//! |--------|------|
//! | [`builder`] | FluenRuntime 构建器（provider + config + 工具注册） |
//! | [`approval`] | 工具审批装饰器（[`approval::ApprovalGuard`] + [`approval::Approver`]） |
//! | [`observability`] | 工具观测装饰器（[`observability::ObservedTool`] + [`observability::ToolEventSink`]） |
//! | [`error`] | 运行时错误类型 |

pub mod approval;
pub mod builder;
pub mod error;
pub mod observability;

#[allow(unused)]
pub use builder::FluenRuntimeBuilder;
#[allow(unused)]
pub use error::RuntimeError;

use referee_ai::engine::{ChatHandle, EngineStartError, SessionSnapshot};
use referee_ai::session::{ChatPayload, SessionId};

/// Fluen 统一运行时 — 封装 referee [`Engine`]。
///
/// `Clone` 语义与 `Engine` 一致（内部全 `Arc`），可在多 task 间共享。
/// 创建通过 [`FluenRuntimeBuilder`] 进行，不直接 `new`。
#[derive(Clone)]
pub struct FluenRuntime {
    engine: referee_ai::Engine,
}

impl std::fmt::Debug for FluenRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FluenRuntime")
            .field("engine", &self.engine)
            .finish()
    }
}

impl FluenRuntime {
    /// 从 builder 内部构造（仅 [`FluenRuntimeBuilder`] 调用）。
    pub(crate) fn from_engine(engine: referee_ai::Engine) -> Self {
        Self { engine }
    }

    // ── 引擎能力 ──────────────────────────────

    /// 非流式发起一轮 Chat。
    ///
    /// 返回句柄，`wait()` 得到 [`EngineReply::Success`] / [`EngineReply::Error`] 等。
    /// 适合不需要边生成边消费的调用方（如知识库构建 pipeline）。
    pub fn chat(
        &self,
        session_id: SessionId,
        payload: ChatPayload,
    ) -> Result<ChatHandle, EngineStartError> {
        self.engine.chat(session_id, payload)
    }

    /// 流式发起一轮 Chat。
    ///
    /// 返回句柄，`wait()` 得到 [`EngineReply::Streaming`]，调用方消费 chunk 流。
    /// 引擎内部累积收敛与非流式一致。
    pub fn chat_stream(
        &self,
        session_id: SessionId,
        payload: ChatPayload,
    ) -> Result<ChatHandle, EngineStartError> {
        self.engine.chat_stream(session_id, payload)
    }

    /// 中断指定会话的当前回合（幂等；有活动回合才返回 `true`）。
    pub fn interrupt(&self, session_id: SessionId) -> bool {
        self.engine.interrupt(session_id)
    }

    // ── 会话生命周期 ──────────────────────────

    /// 当前会话数。
    pub fn session_count(&self) -> usize {
        self.engine.session_count()
    }

    /// 枚举全部会话 ID。
    pub fn list_sessions(&self) -> Vec<SessionId> {
        self.engine.list_sessions()
    }

    /// 查询单个会话的运行快照。
    pub fn session_info(&self, session_id: SessionId) -> Option<SessionSnapshot> {
        self.engine.session_info(session_id)
    }

    /// 移除指定会话，返回是否确有会话被移除。
    pub fn remove_session(&self, session_id: SessionId) -> bool {
        self.engine.remove_session(session_id)
    }

    // ── 观测 ──────────────────────────────────

    /// 全局已消耗 Token 数。
    pub fn total_consumed_tokens(&self) -> u64 {
        self.engine.total_consumed_tokens()
    }

    /// 指定会话已消耗 Token 数。
    pub fn session_consumed_tokens(&self, session_id: SessionId) -> Option<u64> {
        self.engine.session_consumed_tokens(session_id)
    }

    /// 当前缓存条目数。
    pub fn cache_len(&self) -> usize {
        self.engine.cache_len()
    }

    /// 恢复已确认的会话事实到指定会话历史（崩溃恢复用，不触发 LLM）。
    pub fn restore_session_history(
        &self,
        session_id: SessionId,
        messages: Vec<referee_ai::provider::Message>,
    ) -> Result<usize, referee_ai::engine::EngineError> {
        self.engine.restore_session_history(session_id, messages)
    }

    // ── 引擎句柄访问 ──────────────────────────

    /// 获取内部 Engine 引用（供上层需要直接操作引擎的场景使用，如注册工具）。
    pub fn engine(&self) -> &referee_ai::Engine {
        &self.engine
    }
}

/// 便捷重导出：上层模块常用类型
#[allow(unused_imports)]
pub use referee_ai::engine::EngineConfig;
#[allow(unused_imports)]
pub use referee_ai::session::{ChatOptions, SessionConfig};
