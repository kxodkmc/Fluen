//! LLM 客户端与运行时构建辅助。
//!
//! 复用 `motis_chat::runtime` 中的 `build_chat_client` 逻辑（避免循环依赖，
//! 此处独立实现），并叠加知识库工具集与 `submit_plan` 工具。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use confluent::agent_runtime::{
    AgentEvent, InvocationContext, RuntimeObserver, Tool, ToolError, ToolProvider, ToolSchema,
};
use confluent::llmkit::{
    AnthropicProvider, AnthropicTransformer, ApiStyle, ChatClient, ChatClientConfig,
    DualStyleProvider, OpenAiProvider, OpenAiTransformer, RequestTransformer, ThinkingMode,
    ZhipuProvider, ZhipuTransformer,
};
use confluent::{ConfluentRuntime, ConfluentRuntimeBuilder};
use serde_json::{json, Value};

use crate::llm_config::model::{LlmConfig, ProviderConfig};
use crate::llm_config::model::SceneModelRef;

use super::error::KnowledgeBuilderError;
use super::types::ExtractionPlan;

/// 共享捕获状态：用于 SubmitPlanTool 把 plan 传回 pipeline。
pub type PlanCapture = Arc<Mutex<Option<ExtractionPlan>>>;

/// 共享捕获状态：用于 CreateEntryObserver 把 wiki_id 传回 pipeline。
///
/// 每次 `runtime.run()` 前清空，运行中观察者写入，运行后 pipeline 读取。
/// 消除了此前通过 title 反查 wiki_id 的脆弱路径（AI 标点漂移会导致反查失败）。
pub type CreateEntryCapture = Arc<Mutex<Option<String>>>;

/// 共享捕获状态：用于 UsageObserver 把 LLM usage 传回 pipeline。
///
/// V2.1：每次 `runtime.run()` 后从中读取 `prompt_tokens + completion_tokens`
/// 作为会话真实上下文占用，替代 V2.0 的累加估算（避免 Context Overflow）。
pub type UsageCapture = Arc<Mutex<Option<UsageSnapshot>>>;

/// 单次 `runtime.run()` 的 LLM usage 快照。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UsageSnapshot {
    /// 输入 tokens（含 system + 历史轮次 + 当前输入）。
    pub prompt_tokens: usize,
    /// 输出 tokens。
    pub completion_tokens: usize,
}

impl UsageSnapshot {
    /// 总占用 = 输入 + 输出。
    pub fn total(&self) -> usize {
        self.prompt_tokens + self.completion_tokens
    }

    /// 从 AgentEvent::Finish 的 usage 构造。
    pub fn from_token_usage(usage: &confluent::agent_runtime::TokenUsage) -> Self {
        Self {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
        }
    }
}

/// `knowledge_create_entry` 工具名称。
const CREATE_ENTRY_TOOL_NAME: &str = "knowledge_create_entry";
/// `knowledge_edit_entry` 工具名称（合并路径：候选命中已有条目时调用）。
const EDIT_ENTRY_TOOL_NAME: &str = "knowledge_edit_entry";

/// 观察者：捕获条目操作工具返回的 `wiki_id`。
///
/// 注册到 ConfluentRuntime 后，在每次工具调用结束时检查工具名，
/// 若为 `knowledge_create_entry`（新建）或 `knowledge_edit_entry`
/// （合并到已有条目），则从返回值中提取 `wiki_id` 写入 capture。
///
/// **必须同时监听两个工具**：Execution 阶段 AI 依据 L2 检索结果二选一——
/// 无相似条目时新建（create_entry），有相似条目时合并（edit_entry）。
/// 只监听 create_entry 会在合并路径下误报"AI 未调用 create_entry"。
pub struct CreateEntryObserver {
    capture: CreateEntryCapture,
}

impl CreateEntryObserver {
    pub fn new(capture: CreateEntryCapture) -> Self {
        Self { capture }
    }
}

impl RuntimeObserver for CreateEntryObserver {
    fn on_tool_invoked(
        &self,
        tool: &str,
        _duration: Duration,
        result: &Result<Value, ToolError>,
    ) {
        if tool != CREATE_ENTRY_TOOL_NAME && tool != EDIT_ENTRY_TOOL_NAME {
            return;
        }
        if let Ok(val) = result {
            if let Some(wiki_id) = val.get("wiki_id").and_then(|v| v.as_str()) {
                tracing::debug!(tool = %tool, wiki_id = %wiki_id, "捕获条目操作结果");
                *self.capture.lock().expect("create_entry capture poisoned") =
                    Some(wiki_id.to_string());
            }
        }
    }
}

/// 观察者：捕获 LLM 调用的真实 token usage。
///
/// V2.1：通过监听 [`AgentEvent::Finish`] 提取 `prompt_tokens` 与
/// `completion_tokens`，写入共享 capture。pipeline 在 `runtime.run()`
/// 返回后读取，作为会话真实上下文占用（避免 V2.0 的累加估算误差）。
///
/// `input_tokens` 已包含完整历史（system + 所有历史轮次输入输出 + 当前轮输入），
/// 直接取最后一次调用的 `input + output` 作为 `history_used`。
pub struct UsageObserver {
    capture: UsageCapture,
}

impl UsageObserver {
    pub fn new(capture: UsageCapture) -> Self {
        Self { capture }
    }
}

impl RuntimeObserver for UsageObserver {
    fn on_agent_event(
        &self,
        event: &AgentEvent,
        _ctx: &confluent::agent_runtime::ExecutionContext,
    ) {
        if let AgentEvent::Finish { usage, .. } = event {
            let snapshot = UsageSnapshot::from_token_usage(usage);
            tracing::debug!(
                prompt_tokens = snapshot.prompt_tokens,
                completion_tokens = snapshot.completion_tokens,
                total = snapshot.total(),
                "捕获 LLM usage"
            );
            *self.capture.lock().expect("usage capture poisoned") = Some(snapshot);
        }
    }
}

/// Anthropic API 默认版本头。
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// OpenAI 风格端点的路径后缀。
const OPENAI_CHAT_PATH: &str = "/chat/completions";

/// Anthropic 风格端点的路径后缀。
const ANTHROPIC_MESSAGES_PATH: &str = "/v1/messages";

// ---------------------------------------------------------------------------
// Provider/Model 解析
// ---------------------------------------------------------------------------

/// 解析场景化模型引用为实际的 (ProviderConfig, model_id)。
///
/// 优先使用 `scene_models.knowledge_build`，回退到全局激活项。
pub fn resolve_kb_provider<'a>(
    llm: &'a LlmConfig,
    model_ref: &SceneModelRef,
) -> Result<(&'a ProviderConfig, String), KnowledgeBuilderError> {
    if let Some((provider, model_id)) = llm.resolve_scene(model_ref) {
        tracing::info!(
            provider_id = %provider.id,
            model_id = %model_id,
            style = ?provider.default_style,
            "知识库构建模型已解析"
        );
        return Ok((provider, model_id));
    }
    tracing::error!(
        provider_id = %model_ref.provider_id,
        model_id = %model_ref.model_id,
        "场景模型引用失效（provider 或 model 不存在）"
    );
    Err(KnowledgeBuilderError::Config(format!(
        "场景模型引用失效：provider={}, model={}",
        model_ref.provider_id, model_ref.model_id
    )))
}

/// 规范化 OpenAI 端点 URL（补全 `/chat/completions`）。
fn normalize_openai_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with(OPENAI_CHAT_PATH) {
        trimmed.to_string()
    } else {
        format!("{trimmed}{OPENAI_CHAT_PATH}")
    }
}

/// 规范化 Anthropic 端点 URL（补全 `/v1/messages`）。
fn normalize_anthropic_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with(ANTHROPIC_MESSAGES_PATH) {
        trimmed.to_string()
    } else {
        format!("{trimmed}{ANTHROPIC_MESSAGES_PATH}")
    }
}

/// 构建提供商的额外请求头（自动补充 `anthropic-version`）。
fn build_extra_headers(provider: &ProviderConfig) -> Vec<(String, String)> {
    let mut headers: Vec<(String, String)> = provider
        .extra_headers
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if matches!(provider.default_style, ApiStyle::Anthropic)
        && !headers.iter().any(|(k, _)| k == "anthropic-version")
    {
        headers.push(("anthropic-version".into(), ANTHROPIC_VERSION.into()));
    }
    headers
}

/// 根据 ProviderConfig 构造 ChatClient。
///
/// 按提供商的 base_url 情况选择 Provider：
/// - 同时有 openai/anthropic base_url → DualStyleProvider
/// - 仅 openai → OpenAiProvider
/// - 仅 anthropic → AnthropicProvider
pub fn build_chat_client(
    provider: &ProviderConfig,
) -> Result<ChatClient, KnowledgeBuilderError> {
    let api_key = provider.api_key.as_ref().ok_or_else(|| {
        tracing::error!(provider_id = %provider.id, "提供商未配置 api_key");
        KnowledgeBuilderError::Config(format!("提供商 {} 未配置 api_key", provider.id))
    })?;

    let http_client = reqwest::Client::new();
    let config = ChatClientConfig {
        default_style: provider.default_style,
        // 非流式请求（如文献 AI 校正）输入大、输出长，默认 120s 总超时
        // （含重试）不足，放宽到 300s。流式调用不受此字段约束。
        request_timeout: Duration::from_secs(300),
        ..Default::default()
    };

    let has_openai = provider.openai_base_url.is_some();
    let has_anthropic = provider.anthropic_base_url.is_some();
    let extra_headers = build_extra_headers(provider);

    // 深度适配提供商：按 provider.id 路由到专用 Provider，注入厂商特有请求字段。
    // 智谱（GLM）为 OpenAI 兼容协议，但深度思考参数（thinking.type / reasoning_effort）
    // 需要专用转换器注入；流式 reasoning_content 由 OpenAiTransformer 通用解析。
    if provider.id == "zhipu" {
        let endpoint = normalize_openai_endpoint(
            provider
                .openai_base_url
                .as_ref()
                .ok_or_else(|| {
                    KnowledgeBuilderError::Config(format!(
                        "提供商 {} 未配置 openai_base_url",
                        provider.id
                    ))
                })?,
        );
        tracing::debug!(
            provider_id = %provider.id,
            endpoint = %endpoint,
            "构建 Zhipu ChatClient"
        );
        let p = ZhipuProvider::new(http_client, api_key.clone()).with_endpoint(endpoint);
        return Ok(ChatClient::new(
            Arc::new(p),
            Arc::new(ZhipuTransformer::new()),
            config,
        ));
    }

    if has_openai && has_anthropic {
        let openai_endpoint =
            normalize_openai_endpoint(provider.openai_base_url.as_ref().unwrap());
        let anthropic_endpoint =
            normalize_anthropic_endpoint(provider.anthropic_base_url.as_ref().unwrap());
        tracing::debug!(
            provider_id = %provider.id,
            openai_endpoint = %openai_endpoint,
            anthropic_endpoint = %anthropic_endpoint,
            style = ?provider.default_style,
            "构建 DualStyle ChatClient"
        );
        let dual = DualStyleProvider::new(
            http_client,
            api_key.clone(),
            openai_endpoint,
            anthropic_endpoint,
        )
        .with_extra_headers(extra_headers);
        let transformer: Arc<dyn RequestTransformer> = match provider.default_style {
            ApiStyle::OpenAI => Arc::new(OpenAiTransformer::new()),
            ApiStyle::Anthropic => Arc::new(AnthropicTransformer::new()),
        };
        Ok(ChatClient::new(Arc::new(dual), transformer, config))
    } else if has_openai {
        let endpoint =
            normalize_openai_endpoint(provider.openai_base_url.as_ref().unwrap());
        tracing::debug!(
            provider_id = %provider.id,
            endpoint = %endpoint,
            "构建 OpenAI ChatClient"
        );
        let p = OpenAiProvider::new(http_client, api_key.clone()).with_endpoint(endpoint);
        Ok(ChatClient::new(
            Arc::new(p),
            Arc::new(OpenAiTransformer::new()),
            config,
        ))
    } else {
        let endpoint =
            normalize_anthropic_endpoint(provider.anthropic_base_url.as_ref().unwrap());
        tracing::debug!(
            provider_id = %provider.id,
            endpoint = %endpoint,
            "构建 Anthropic ChatClient"
        );
        let p = AnthropicProvider::new(http_client, api_key.clone())
            .with_endpoint(endpoint)
            .with_extra_headers(extra_headers);
        Ok(ChatClient::new(
            Arc::new(p),
            Arc::new(AnthropicTransformer::new()),
            config,
        ))
    }
}

// ---------------------------------------------------------------------------
// SubmitPlan 工具（Planning 阶段专用）
// ---------------------------------------------------------------------------

/// `submit_plan` 工具的 schema 名称。
pub const SUBMIT_PLAN_TOOL_NAME: &str = "submit_plan";

/// Planning 阶段用于接收 AI 产出的 ExtractionPlan 的工具。
///
/// AI 在 Planning 阶段必须调用此工具提交计划，后端从 tool_call 中解析出
/// 结构化的 ExtractionPlan（不依赖自由文本）。
///
/// 调用 `invoke` 时，plan 会被存入 `capture` 共享状态，pipeline 在
/// `runtime.run` 返回后从中读取。
pub struct SubmitPlanTool {
    schema: ToolSchema,
    capture: PlanCapture,
}

impl SubmitPlanTool {
    pub fn new(capture: PlanCapture) -> Self {
        let parameters = json!({
            "type": "object",
            "properties": {
                "summary_points": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "文献综述要点（3-5 个）"
                },
                "concepts": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "title": {"type": "string"},
                            "brief": {"type": "string"},
                            "existing_id": {"type": "string", "description": "已存在条目的 wiki_id（去重）"},
                            "merge_supplement": {"type": "string", "description": "合并到已存在条目的补充内容"}
                        },
                        "required": ["title", "brief"]
                    }
                },
                "entities": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "title": {"type": "string"},
                            "brief": {"type": "string"},
                            "existing_id": {"type": "string"},
                            "merge_supplement": {"type": "string"}
                        },
                        "required": ["title", "brief"]
                    }
                }
            },
            "required": ["summary_points", "concepts", "entities"]
        });
        Self {
            schema: ToolSchema {
                name: SUBMIT_PLAN_TOOL_NAME.into(),
                description: "提交文献提取计划（Planning 阶段必须调用）".into(),
                parameters,
            },
            capture,
        }
    }
}

#[async_trait]
impl Tool for SubmitPlanTool {
    fn schema(&self) -> &ToolSchema {
        &self.schema
    }

    async fn invoke(
        &self,
        input: Value,
        _ctx: &InvocationContext,
    ) -> Result<Value, ToolError> {
        let plan: ExtractionPlan = serde_json::from_value(input.clone())
            .map_err(|e| ToolError::InvalidParams(format!("plan 解析失败: {e}")))?;

        // 校验计划非空：至少要有 summary_points 或 concepts 或 entities
        if plan.summary_points.is_empty()
            && plan.concepts.is_empty()
            && plan.entities.is_empty()
        {
            tracing::warn!("AI 提交了空的 ExtractionPlan");
            return Err(ToolError::InvalidParams(
                "plan 不能为空：summary_points / concepts / entities 至少需有一项非空".into(),
            ));
        }

        tracing::info!(
            summary_points = plan.summary_points.len(),
            concepts = plan.concepts.len(),
            entities = plan.entities.len(),
            "AI 提交 ExtractionPlan"
        );
        *self.capture.lock().expect("plan capture poisoned") = Some(plan);
        Ok(json!({"success": true, "message": "plan received"}))
    }
}

/// 包装 `SubmitPlanTool` 为 ToolProvider（便于注入 ConfluentRuntimeBuilder）。
pub struct SubmitPlanProvider {
    tool: Arc<SubmitPlanTool>,
}

impl SubmitPlanProvider {
    pub fn new(capture: PlanCapture) -> Self {
        Self {
            tool: Arc::new(SubmitPlanTool::new(capture)),
        }
    }
}

#[async_trait]
impl ToolProvider for SubmitPlanProvider {
    async fn list_tools(&self) -> Vec<Arc<dyn Tool>> {
        vec![self.tool.clone()]
    }
}

// ---------------------------------------------------------------------------
// Runtime 构建
// ---------------------------------------------------------------------------

/// 构建知识库构建专用的 ConfluentRuntime。
///
/// 装配：
/// - LlmAgent（基于场景模型）
/// - KnowledgeToolProvider（5 个工具：query/query_batch/create/edit/get_entry）
/// - SubmitPlanProvider（Planning 阶段专用，捕获 ExtractionPlan）
/// - CreateEntryObserver（Execution 阶段捕获 wiki_id，消除 title 反查）
/// - UsageObserver（V2.1：捕获 LLM 真实 usage，供会话预算跟踪）
///
/// `plan_capture` / `entry_capture` / `usage_capture` 由调用方创建并传入，
/// pipeline 在对应阶段执行后从中读取结果。
pub async fn build_kb_runtime(
    llm: &LlmConfig,
    model_ref: &SceneModelRef,
    kb: fluen_knowledge::async_kb::AsyncKnowledgeBase,
    plan_capture: PlanCapture,
    entry_capture: CreateEntryCapture,
    usage_capture: UsageCapture,
) -> Result<ConfluentRuntime, KnowledgeBuilderError> {
    let (provider, model_id) = resolve_kb_provider(llm, model_ref)?;
    let chat_client = build_chat_client(provider)?;

    // 知识库工具：仅启用提取所需的 5 个
    let kb_config = fluen_knowledge::config::KnowledgeConfig::builder()
        .enabled_tools(vec![
            "query".into(),
            "query_batch".into(),
            "create_entry".into(),
            "edit_entry".into(),
            "get_entry".into(),
        ])
        .build();
    let kb_provider = Arc::new(fluen_knowledge::tools::KnowledgeToolProvider::new(
        kb,
        kb_config,
    ));
    let plan_provider = Arc::new(SubmitPlanProvider::new(plan_capture));
    let entry_observer = Arc::new(CreateEntryObserver::new(entry_capture));
    let usage_observer = Arc::new(UsageObserver::new(usage_capture));

    tracing::debug!(model_id = %model_ref.model_id, agent_id = "knowledge-builder", "装配 ConfluentRuntime");
    // 模型支持思考时启用思考模式（如 DeepSeek / 智谱深度思考）。
    let supports_thinking = provider.model_supports_thinking(&model_id);
    let mut builder = ConfluentRuntimeBuilder::new()
        .with_agent_id("knowledge-builder")
        .with_model(model_id)
        .with_chat_client(Arc::new(chat_client))
        .with_tool_provider(kb_provider as Arc<dyn ToolProvider>)
        .with_tool_provider(plan_provider as Arc<dyn ToolProvider>)
        .with_observer(entry_observer as Arc<dyn RuntimeObserver>)
        .with_observer(usage_observer as Arc<dyn RuntimeObserver>);
    if supports_thinking {
        builder = builder.with_thinking(ThinkingMode::Enabled);
    }

    let runtime = builder
        .build()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "ConfluentRuntime 构建失败");
            KnowledgeBuilderError::Llm(e.to_string())
        })?;

    tracing::info!(model_id = %model_ref.model_id, "知识库构建 Runtime 构建完成");
    Ok(runtime)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_config::model::{
        LlmConfig, ModelCapabilities, ModelConfig, ProviderConfig, ProviderType, SceneModelRef,
        SceneModels,
    };
    use std::collections::HashMap;

    fn sample_provider() -> ProviderConfig {
        ProviderConfig {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            provider_type: ProviderType::Custom,
            openai_base_url: Some("https://api.deepseek.com/v1".into()),
            anthropic_base_url: None,
            api_key: Some("sk-test".into()),
            default_style: ApiStyle::OpenAI,
            extra_headers: HashMap::new(),
            enabled: true,
            models: vec![ModelConfig {
                id: "deepseek-chat".into(),
                name: "DeepSeek Chat".into(),
                capabilities: ModelCapabilities::default(),
                max_output_tokens: Some(8192),
                context_window: Some(64000),
                description: None,
                enabled: true,
            }],
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn resolve_kb_provider_uses_scene_ref() {
        let llm = LlmConfig {
            providers: vec![sample_provider()],
            scene_models: Some(SceneModels {
                knowledge_build: Some(SceneModelRef {
                    provider_id: "deepseek".into(),
                    model_id: "deepseek-chat".into(),
                }),
                ..SceneModels::default()
            }),
            ..LlmConfig::default()
        };
        let scene = SceneModelRef {
            provider_id: "deepseek".into(),
            model_id: "deepseek-chat".into(),
        };
        let (provider, model_id) = resolve_kb_provider(&llm, &scene).unwrap();
        assert_eq!(provider.id, "deepseek");
        assert_eq!(model_id, "deepseek-chat");
    }

    #[test]
    fn resolve_kb_provider_invalid_ref() {
        let llm = LlmConfig {
            providers: vec![sample_provider()],
            ..LlmConfig::default()
        };
        let scene = SceneModelRef {
            provider_id: "nonexistent".into(),
            model_id: "gpt-4o".into(),
        };
        let err = resolve_kb_provider(&llm, &scene).unwrap_err();
        assert!(matches!(err, KnowledgeBuilderError::Config(_)));
    }

    #[test]
    fn build_chat_client_openai_style() {
        let provider = sample_provider();
        let client = build_chat_client(&provider).unwrap();
        // 仅验证不 panic（ChatClient 内部未暴露 URL 字段）
        let _ = client;
    }

    #[test]
    fn build_chat_client_anthropic_style() {
        let mut provider = sample_provider();
        provider.openai_base_url = None;
        provider.anthropic_base_url = Some("https://api.anthropic.com".into());
        provider.default_style = ApiStyle::Anthropic;
        let client = build_chat_client(&provider).unwrap();
        let _ = client;
    }

    #[test]
    fn normalize_openai_completes_endpoint() {
        assert_eq!(
            normalize_openai_endpoint("https://api.deepseek.com/v1"),
            "https://api.deepseek.com/v1/chat/completions"
        );
        assert_eq!(
            normalize_openai_endpoint("https://api.deepseek.com/v1/chat/completions"),
            "https://api.deepseek.com/v1/chat/completions"
        );
    }

    /// 构造测试用的 PlanCapture。
    fn test_capture() -> PlanCapture {
        Arc::new(Mutex::new(None))
    }

    #[test]
    fn submit_plan_tool_schema_name() {
        let tool = SubmitPlanTool::new(test_capture());
        assert_eq!(tool.schema().name, SUBMIT_PLAN_TOOL_NAME);
    }

    #[tokio::test]
    async fn submit_plan_provider_lists_tool() {
        let provider = SubmitPlanProvider::new(test_capture());
        let tools = provider.list_tools().await;
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].schema().name, SUBMIT_PLAN_TOOL_NAME);
    }

    #[tokio::test]
    async fn submit_plan_tool_accepts_valid_plan() {
        let capture = test_capture();
        let tool = SubmitPlanTool::new(capture.clone());
        let ctx = InvocationContext {
            run_id: "test".into(),
            agent_id: "test".into(),
            tool_call_id: "test".into(),
            cancel_token: tokio_util::sync::CancellationToken::new(),
            timeout: std::time::Duration::from_secs(30),
        };
        let input = json!({
            "summary_points": ["要点1"],
            "concepts": [{"title": "机器学习", "brief": "数据驱动方法"}],
            "entities": []
        });
        let result = tool.invoke(input, &ctx).await.unwrap();
        assert!(result["success"].as_bool().unwrap_or(false));
        // capture 应已存入 plan
        let captured = capture.lock().unwrap();
        assert!(captured.is_some());
        assert_eq!(captured.as_ref().unwrap().concepts.len(), 1);
    }

    #[tokio::test]
    async fn submit_plan_tool_rejects_invalid_plan() {
        let tool = SubmitPlanTool::new(test_capture());
        let ctx = InvocationContext {
            run_id: "test".into(),
            agent_id: "test".into(),
            tool_call_id: "test".into(),
            cancel_token: tokio_util::sync::CancellationToken::new(),
            timeout: std::time::Duration::from_secs(30),
        };
        let input = json!({"foo": "bar"});
        let result = tool.invoke(input, &ctx).await;
        assert!(result.is_err());
    }
}
