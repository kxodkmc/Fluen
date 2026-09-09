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

#[cfg(test)]
mod tests {
    //! 预算口径验证：引擎按「每个内部 LLM 轮」累计预算，而调用方（如知识库
    //! pipeline）只能看到回合的最后一次响应。用脚本化 mock provider 走真实
    //! 回合循环（工具调用轮 + 收尾轮），对账两侧读数。

    use super::*;
    use async_trait::async_trait;
    use futures::stream::BoxStream;
    use referee_ai::budget::BudgetConfig;
    use referee_ai::engine::{EngineReply, EngineStartError};
    use referee_ai::provider::{
        ChatRequest, ChatResponse, FinishReason, LlmError, LLMProvider, Message, MessageContent,
        ModelSpec, MultimodalCapabilities, ProviderCapabilities, ProviderId, Role, StreamChunk,
        TokenUsage, ToolCall, ToolCallFunction,
    };
    use referee_ai::tool::{Tool, ToolContext, ToolError, ToolOutput};
    use serde_json::{json, Value};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    /// 脚本化 provider：第 1 轮发工具调用，第 2 轮纯文本收尾。
    ///
    /// 两轮 usage 固定不同（1000 / 1100），用于验证引擎把两轮都计入预算。
    struct ScriptedProvider {
        calls: Arc<AtomicU32>,
    }

    impl ScriptedProvider {
        fn new() -> (Self, Arc<AtomicU32>) {
            let calls = Arc::new(AtomicU32::new(0));
            (Self { calls: calls.clone() }, calls)
        }
    }

    #[async_trait]
    impl LLMProvider for ScriptedProvider {
        fn id(&self) -> ProviderId {
            ProviderId::new("scripted")
        }
        fn capabilities(&self) -> &ProviderCapabilities {
            static CAPS: ProviderCapabilities = ProviderCapabilities {
                parallel_tool_calls: false,
                system_role: true,
                streaming: false,
                usage_reported: true,
                multimodal: MultimodalCapabilities::NONE,
            };
            &CAPS
        }
        fn model_spec(&self) -> ModelSpec {
            ModelSpec {
                context_window_tokens: 128 * 1024,
                max_output_tokens: 16 * 1024,
            }
        }
        async fn chat(&self, _req: ChatRequest) -> Result<ChatResponse, LlmError> {
            let mark = self.calls.fetch_add(1, Ordering::SeqCst);
            let text = |t: &str| MessageContent::text(t.to_string());
            Ok(match mark {
                // 轮 1：发起工具调用（usage total = 1000）
                0 => ChatResponse {
                    id: "scripted".into(),
                    model: "scripted".into(),
                    message: Message {
                        role: Role::Assistant,
                        content: text("我调用工具。"),
                        reasoning_content: None,
                        tool_calls: vec![ToolCall {
                            id: "call_1".into(),
                            function: ToolCallFunction {
                                name: "echo_tool".into(),
                                arguments: json!({"x": 1}).to_string(),
                            },
                        }],
                        tool_call_id: None,
                        usage: Some(TokenUsage {
                            prompt_tokens: 900,
                            completion_tokens: 100,
                            total_tokens: 1000,
                            ..Default::default()
                        }),
                    },
                    finish_reason: FinishReason::ToolCalls,
                    usage: Some(TokenUsage {
                        prompt_tokens: 900,
                        completion_tokens: 100,
                        total_tokens: 1000,
                        ..Default::default()
                    }),
                },
                // 轮 2：纯文本收尾（usage total = 1100）
                _ => ChatResponse {
                    id: "scripted".into(),
                    model: "scripted".into(),
                    message: Message {
                        role: Role::Assistant,
                        content: text("已完成。"),
                        reasoning_content: None,
                        tool_calls: vec![],
                        tool_call_id: None,
                        usage: Some(TokenUsage {
                            prompt_tokens: 1050,
                            completion_tokens: 50,
                            total_tokens: 1100,
                            ..Default::default()
                        }),
                    },
                    finish_reason: FinishReason::Stop,
                    usage: Some(TokenUsage {
                        prompt_tokens: 1050,
                        completion_tokens: 50,
                        total_tokens: 1100,
                        ..Default::default()
                    }),
                },
            })
        }
        async fn chat_stream(
            &self,
            _req: ChatRequest,
        ) -> Result<BoxStream<'static, Result<StreamChunk, LlmError>>, LlmError> {
            unreachable!("本测试走非流式路径")
        }
    }

    /// 固定成功返回的哑工具（等待语义，与 ForceWaitGuard 包装后的真实 KB 工具一致）。
    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo_tool"
        }
        fn description(&self) -> &str {
            "echo"
        }
        fn input_schema(&self) -> Value {
            json!({"type": "object"})
        }
        fn default_wait(&self) -> bool {
            true
        }
        async fn execute(
            &self,
            _ctx: ToolContext,
            _args: Value,
        ) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::text("ok"))
        }
    }

    fn build_runtime(session_limit: u64) -> (FluenRuntime, Arc<AtomicU32>) {
        let mut cfg = EngineConfig::default();
        cfg.budget = BudgetConfig {
            session_limit,
            global_limit: 0,
        };
        cfg.cache.enabled = false;
        let (provider, calls) = ScriptedProvider::new();
        let runtime = FluenRuntimeBuilder::new(Arc::new(provider))
            .with_config(cfg)
            .with_tool(Arc::new(EchoTool))
            .build();
        (runtime, calls)
    }

    fn payload() -> referee_ai::session::ChatPayload {
        referee_ai::session::ChatPayload {
            message: Message::user("做一件事"),
            options: ChatOptions {
                system_prompt: Some("test".into()),
                thinking: referee_ai::provider::ThinkingConfig {
                    enabled: false,
                    effort: None,
                },
                ..ChatOptions::default()
            },
            peer_depth: 0,
        }
    }

    #[tokio::test]
    async fn session_budget_counts_every_round_not_just_final() {
        let (runtime, calls) = build_runtime(0); // 无限制
        let sid = referee_ai::session::SessionId::new_v4();

        let handle = runtime.chat(sid, payload()).unwrap();
        let reply = handle.wait().await.unwrap();
        let resp = match reply {
            EngineReply::Success(resp) => resp,
            other => panic!("期望 Success，得到 {other:?}"),
        };

        // 调用方视角（pipeline 的 history_used 口径）：只有收尾轮 = 1100
        assert_eq!(resp.usage.as_ref().unwrap().total_tokens, 1100);

        // 引擎预算口径：工具调用轮(1000) + 收尾轮(1100) 全部计入
        assert_eq!(
            runtime.session_consumed_tokens(sid),
            Some(2100),
            "引擎必须把工具调用轮与收尾轮都计入会话预算"
        );
        assert_eq!(runtime.total_consumed_tokens(), 2100);

        // 一次「1 个工具调用」的回合 = 2 次 LLM 调用
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn session_budget_rejection_reports_per_round_cumulative() {
        // 限额 = 两轮之和：第 1 次回合恰好耗尽，同会话再发起即被拒
        let (runtime, _calls) = build_runtime(2100);
        let sid = referee_ai::session::SessionId::new_v4();

        let handle = runtime.chat(sid, payload()).unwrap();
        handle.wait().await.unwrap();
        assert_eq!(runtime.session_consumed_tokens(sid), Some(2100));

        // 同会话再次发起：check_budget 读到 per-round 累计 2100 ≥ 2100，拒绝
        let err = runtime.chat(sid, payload()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Session budget exceeded"),
            "应报会话预算超限，得到: {msg}"
        );
        assert!(msg.contains("2100"), "used/limit 应为累计流水 2100: {msg}");
    }
}
