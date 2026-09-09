//! 引擎交互重试层 — 工具凭证（capture）驱动的催促重试闭环。
//!
//! ## 解决的问题
//!
//! 部分模型在多轮工具循环后倾向以纯文本"作答"而非调用工具：
//! chat 正常返回（`EngineReply::Success`）但 capture 为空（如未调用
//! `submit_plan` / `knowledge_create_entry`），流水线随即判定失败。
//! 本模块在 capture 为空时向**同一会话**追加一条强约束催促消息再试，
//! 用最新的用户消息把模型拉回工具调用路径。
//!
//! ## 与 [`super::llm_helper::run_chat`] 的分工
//!
//! `run_chat` 负责单次「发起 + 等待」，本模块负责「capture 检查 + 催促重试」。
//! 会话（`session_id`）由调用方创建，循环结束后由调用方负责 `remove_session`。

use referee_ai::session::SessionId;

use crate::agent_runtime::FluenRuntime;

use super::error::KnowledgeBuilderError;
use super::llm_helper::{run_chat, UsageSnapshot};

/// 知识库构建系统提示词（Planning + Execution 通用）。
///
/// 告知 AI 可用工具及其用途，确保 AI 知道需要通过 `submit_plan` 提交计划
/// 或通过 `knowledge_create_entry` / `knowledge_edit_entry` 创建条目。
pub const KB_BUILD_SYSTEM_PROMPT: &str = r#"你是学术文献知识库构建助手。你可以使用以下工具来完成任务：

- `knowledge_query`：搜索知识库已有条目（keyword / semantic / hybrid）
- `knowledge_query_batch`：批量搜索知识库已有条目（**慎用**：返回含条目全量内容，
  单次即注入数万 token 永久占用会话历史；单条确认一律用 `knowledge_query`）
- `knowledge_create_entry`：创建新条目，参数：type（summary/concept/entity）、title、body、source（仅 summary，格式 `ref-xxxxxxxxxxxxxxxx`）
- `knowledge_edit_entry`：编辑已有条目正文，参数：id、ops（kind=search_replace/insert_after 的操作列表）
- `knowledge_get_entry`：按 wikiID 获取条目详情
- `submit_plan`：提交文献提取计划（Planning 阶段必须调用）
- `submit_relations`：提交条目间关联关系（EstablishingRelations 阶段必须调用）

**重要规则**：
1. 所有交付物（提取计划 / 条目 / 关联）只能通过工具调用提交；纯文本回复视为未完成，系统会要求你重新以工具调用提交
2. 关联关系必须使用 wikiID（格式 `wiki-xxxxxxxxxxxxxxxx`），严禁使用标题；关联统一在最后通过 `submit_relations` 提交，不要在正文里手写
3. **溯源（强制）**：summary 的 body 全文用 `<ref-xxxxxxxxxxxxxxxx>…</ref-xxxxxxxxxxxxxxxx>` 包裹（标签即 source 的 refID）；concept / entity 的 body 用 `<ref-…>` 标注来源段落
4. 禁止在正文中手写 `## 关联页面` 区，关联关系由工具参数与 `submit_relations` 建立
"#;

/// 催促重试上限（首次调用 + 2 次催促，共最多 3 次模型调用）。
pub const TOOL_NUDGE_MAX_ATTEMPTS: usize = 3;

/// 催促消息模板（capture 为空时向同一会话追加发送）。
const TOOL_NUDGE_PROMPT_TEMPLATE: &str = "你上一轮的回复没有调用必需的工具：纯文本回复不会被系统接受，本轮任务尚未完成。{hint}。不要把结果写在文本里，必须以工具调用提交；再次未调用工具将被判定为任务失败。";

/// 渲染催促消息。
pub fn render_nudge(hint: &str) -> String {
    TOOL_NUDGE_PROMPT_TEMPLATE.replace("{hint}", hint)
}

/// 发起 chat 并等待工具捕获；capture 为空时催促重试。
///
/// 每次调用后通过 `try_take` 检查共享 capture（如 [`super::llm_helper::PlanCapture`]、
/// [`super::llm_helper::CreateEntryCapture`]）：
///
/// - 有值 → 返回 `(捕获值, 最后一次调用的 usage)`
/// - 为空 → 向同一会话追加催促消息（`nudge_hint` 描述本轮必须调用的工具）
///   再试，最多 [`TOOL_NUDGE_MAX_ATTEMPTS`] 次
///
/// `usage` 取最后一次成功调用的快照（其 prompt_tokens 含完整历史，
/// 是会话真实上下文占用的最准确读数）。
pub async fn chat_until_captured<T>(
    runtime: &FluenRuntime,
    thinking_enabled: bool,
    session_id: SessionId,
    first_prompt: &str,
    nudge_hint: &str,
    mut try_take: impl FnMut() -> Option<T>,
) -> Result<(T, UsageSnapshot), KnowledgeBuilderError> {
    let mut prompt = first_prompt.to_string();
    for attempt in 1..=TOOL_NUDGE_MAX_ATTEMPTS {
        // 回显 LLM 本轮原始回复（截断），用于核对「模型实际产出」是否偏离注入的文献原文。
        let (text, usage) = run_chat(runtime, session_id, &prompt, KB_BUILD_SYSTEM_PROMPT, thinking_enabled)
            .await?;
        let reply_prefix: String = text.chars().take(240).collect();
        tracing::debug!(
            attempt,
            reply_chars = text.chars().count(),
            reply_prefix = %reply_prefix,
            "LLM 原始回复（前缀）"
        );
        if let Some(captured) = try_take() {
            return Ok((captured, usage));
        }
        tracing::warn!(
            attempt,
            max_attempts = TOOL_NUDGE_MAX_ATTEMPTS,
            "AI 未调用必需工具（capture 为空），发送催促消息重试"
        );
        prompt = render_nudge(nudge_hint);
    }
    Err(KnowledgeBuilderError::AiOutput(format!(
        "AI 未调用必需工具（已催促 {} 次仍失败）：{nudge_hint}",
        TOOL_NUDGE_MAX_ATTEMPTS - 1
    )))
}

// ---------------------------------------------------------------------------
// 单元测试（脚本化 mock provider 驱动完整引擎闭环）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use futures::stream::BoxStream;
    use referee_ai::provider::{
        ChatRequest, ChatResponse, FinishReason, LlmError, LLMProvider, Message, MessageContent,
        ModelSpec, MultimodalCapabilities, ProviderCapabilities, ProviderId, Role, StreamChunk,
        ToolCall, ToolCallFunction,
    };
    use serde_json::json;

    use super::super::llm_helper::{new_session_id, PlanCapture, SubmitPlanTool};
    use crate::agent_runtime::FluenRuntimeBuilder;

    /// 脚本化 Provider：按序弹出预设响应；耗尽后返回纯文本（保证回合收敛）。
    struct ScriptedProvider {
        scripted: Mutex<Vec<ChatResponse>>,
        calls: Mutex<usize>,
    }

    impl ScriptedProvider {
        fn new(scripted: Vec<ChatResponse>) -> Self {
            Self {
                scripted: Mutex::new(scripted),
                calls: Mutex::new(0),
            }
        }

        fn call_count(&self) -> usize {
            *self.calls.lock().unwrap()
        }
    }

    /// 纯文本响应（模拟模型"只作答不调工具"）。
    fn text_response(text: &str) -> ChatResponse {
        ChatResponse {
            id: "scripted".into(),
            model: "scripted".into(),
            message: Message {
                role: Role::Assistant,
                content: MessageContent::text(text),
                reasoning_content: None,
                tool_calls: Vec::new(),
                tool_call_id: None,
                usage: None,
            },
            finish_reason: FinishReason::Stop,
            usage: None,
        }
    }

    /// 携带合法 submit_plan 调用的响应。
    fn submit_plan_response() -> ChatResponse {
        let args = json!({
            "summary_points": ["要点1"],
            "concepts": [{"title": "机器学习", "brief": "数据驱动方法"}],
            "entities": []
        })
        .to_string();
        ChatResponse {
            id: "scripted".into(),
            model: "scripted".into(),
            message: Message {
                role: Role::Assistant,
                content: MessageContent::text("我来提交计划。"),
                reasoning_content: None,
                tool_calls: vec![ToolCall {
                    id: "call_submit_1".into(),
                    function: ToolCallFunction {
                        name: "submit_plan".into(),
                        arguments: args,
                    },
                }],
                tool_call_id: None,
                usage: None,
            },
            finish_reason: FinishReason::ToolCalls,
            usage: None,
        }
    }

    #[async_trait]
    impl LLMProvider for ScriptedProvider {
        fn id(&self) -> ProviderId {
            ProviderId::new("scripted")
        }

        fn capabilities(&self) -> &ProviderCapabilities {
            static CAPS: ProviderCapabilities = ProviderCapabilities {
                parallel_tool_calls: true,
                system_role: true,
                streaming: false,
                usage_reported: true,
                multimodal: MultimodalCapabilities::NONE,
            };
            &CAPS
        }

        fn model_spec(&self) -> ModelSpec {
            ModelSpec {
                context_window_tokens: 8192,
                max_output_tokens: 4096,
            }
        }

        async fn chat(&self, _req: ChatRequest) -> Result<ChatResponse, LlmError> {
            *self.calls.lock().unwrap() += 1;
            let mut scripted = self.scripted.lock().unwrap();
            let resp = if scripted.is_empty() {
                text_response("（脚本已耗尽）")
            } else {
                scripted.remove(0)
            };
            Ok(resp)
        }

        async fn chat_stream(
            &self,
            _req: ChatRequest,
        ) -> Result<BoxStream<'static, Result<StreamChunk, LlmError>>, LlmError> {
            Err(LlmError::Protocol(
                "scripted provider: streaming unsupported".into(),
            ))
        }
    }

    #[test]
    fn render_nudge_embeds_hint() {
        let nudge = render_nudge("请立即调用 submit_plan");
        assert!(nudge.contains("请立即调用 submit_plan"));
        assert!(nudge.contains("纯文本回复不会被系统接受"));
    }

    #[tokio::test]
    async fn chat_until_captured_nudges_model_into_tool_call() {
        let capture: PlanCapture = Arc::new(Mutex::new(None));
        // 脚本：首轮纯文本（触发催促）→ 催促轮调用 submit_plan → 工具结果回填后的收敛轮文本
        let provider = Arc::new(ScriptedProvider::new(vec![
            text_response("这是我的提取计划：……（纯文本）"),
            submit_plan_response(),
            text_response("计划已提交。"),
        ]));
        let runtime = FluenRuntimeBuilder::new(provider.clone())
            .with_tool(Arc::new(SubmitPlanTool::new(capture.clone())))
            .build();

        let session_id = new_session_id();
        let (plan, _usage) = chat_until_captured(
            &runtime,
            false,
            session_id,
            "请提交提取计划",
            "请立即调用 submit_plan 工具提交提取计划",
            || capture.lock().unwrap().take(),
        )
        .await
        .unwrap();
        runtime.remove_session(session_id);

        assert_eq!(plan.concepts.len(), 1);
        assert_eq!(plan.concepts[0].title, "机器学习");
        // 首轮文本 + 催促轮（工具调用 + 收敛轮）= 3 次 LLM 调用
        assert_eq!(provider.call_count(), 3);
    }

    #[tokio::test]
    async fn chat_until_captured_fails_after_max_nudges() {
        let capture: PlanCapture = Arc::new(Mutex::new(None));
        let provider = Arc::new(ScriptedProvider::new(vec![text_response(
            "我把计划直接写在这里……（纯文本）",
        )]));
        let runtime = FluenRuntimeBuilder::new(provider.clone())
            .with_tool(Arc::new(SubmitPlanTool::new(capture.clone())))
            .build();

        let session_id = new_session_id();
        let err = chat_until_captured(
            &runtime,
            false,
            session_id,
            "请提交提取计划",
            "请立即调用 submit_plan 工具提交提取计划",
            || capture.lock().unwrap().take(),
        )
        .await
        .unwrap_err();
        runtime.remove_session(session_id);

        assert!(matches!(err, KnowledgeBuilderError::AiOutput(_)));
        assert_eq!(provider.call_count(), TOOL_NUDGE_MAX_ATTEMPTS);
    }

    #[tokio::test]
    async fn chat_until_captured_returns_immediately_when_tool_called_first_try() {
        let capture: PlanCapture = Arc::new(Mutex::new(None));
        // 脚本：首轮直接调用 submit_plan + 收敛轮文本（无催促）
        let provider = Arc::new(ScriptedProvider::new(vec![
            submit_plan_response(),
            text_response("计划已提交。"),
        ]));
        let runtime = FluenRuntimeBuilder::new(provider.clone())
            .with_tool(Arc::new(SubmitPlanTool::new(capture.clone())))
            .build();

        let session_id = new_session_id();
        let (plan, _usage) = chat_until_captured(
            &runtime,
            false,
            session_id,
            "请提交提取计划",
            "请立即调用 submit_plan 工具提交提取计划",
            || capture.lock().unwrap().take(),
        )
        .await
        .unwrap();
        runtime.remove_session(session_id);

        assert_eq!(plan.summary_points.len(), 1);
        // 工具调用轮 + 收敛轮 = 2 次，无催促开销
        assert_eq!(provider.call_count(), 2);
    }
}
