//! LLM 客户端与运行时构建辅助。
//!
//! 基于 referee [`FluenRuntime`] 构建知识库构建专用的运行时，
//! 装配知识库工具集与 `submit_plan` 工具。
//!
//! ## 与 confluent 时代的差异
//!
//! - `ConfluentRuntime` → [`FluenRuntime`]（封装 referee `Engine`）
//! - `ChatClient` + `ChatProvider` + `RequestTransformer` → `LLMProvider`（`GenericProvider`）
//! - `Tool` / `ToolProvider` / `ToolSchema` → referee `Tool` trait（`name()` + `execute()`）
//! - `RuntimeObserver` → **装饰器捕获**：`EntryCaptureGuard` 包装知识库工具，
//!   `execute` 后从返回值提取 `wiki_id` 写入 capture（替代 confluent 的 observer 模式）
//! - `AgentEvent::Finish` usage → `ChatResponse.usage`（`ChatHandle::wait()` 返回）

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use referee_ai::provider::{Message, ThinkingConfig, TokenUsage};
use referee_ai::session::{ChatOptions, ChatPayload, SessionId};
use referee_ai::tool::{Tool, ToolCategory, ToolContext, ToolError, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::agent_runtime::observability::ToolEventSink;
use crate::agent_runtime::{FluenRuntime, FluenRuntimeBuilder};
use crate::knowledge_mcp_bridge::{KbMcpBridge, McpKnowledgeTool, KB_BUILD_TOOL_NAMES};
use crate::llm_chat;
use crate::llm_config::model::{LlmConfig, SceneModelRef};

use super::error::KnowledgeBuilderError;
use super::reporter::KbReporter;
use super::types::ExtractionPlan;

/// 共享捕获状态：用于 SubmitPlanTool 把 plan 传回 pipeline。
pub type PlanCapture = Arc<Mutex<Option<ExtractionPlan>>>;

/// 共享捕获状态：用于 EntryCaptureGuard 把 wiki_id 传回 pipeline。
///
/// 每次 `engine.chat()` 前清空，运行中工具执行后写入，运行后 pipeline 读取。
/// 消除了此前通过 title 反查 wiki_id 的脆弱路径（AI 标点漂移会导致反查失败）。
pub type CreateEntryCapture = Arc<Mutex<Option<String>>>;

/// 共享捕获状态：用于 pipeline 读取 LLM usage。
///
/// 每次 `engine.chat()` 后从中读取 `prompt_tokens + completion_tokens`
/// 作为会话真实上下文占用，替代累加估算（避免 Context Overflow）。
pub type UsageCapture = Arc<Mutex<Option<UsageSnapshot>>>;

/// 关联关系对（`submit_relations` 提交的条目间关联）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationPair {
    /// 起始条目 wikiID。
    pub from: String,
    /// 目标条目 wikiID。
    pub to: String,
    /// 关联谓词（缺省 `related`；仅允许字母数字/下划线，≤32 字符）。
    #[serde(default)]
    pub predicate: Option<String>,
}

/// 共享捕获状态：用于 SubmitRelationsTool 把关联对传回 pipeline。
pub type RelationsCapture = Arc<Mutex<Option<Vec<RelationPair>>>>;

/// `submit_relations` 顶层入参（与 `input_schema` 声明一致：对象包裹）。
#[derive(Debug, Deserialize)]
struct RelationsArgs {
    relations: Vec<RelationPair>,
}

/// 单次 `engine.chat()` 的 LLM usage 快照。
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

    /// 从 referee `TokenUsage` 构造。
    pub fn from_token_usage(usage: &TokenUsage) -> Self {
        Self {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
        }
    }
}

/// `knowledge_create_entry` 工具名称（MCP 桥接同名工具）。
pub const CREATE_ENTRY_TOOL_NAME: &str = "knowledge_create_entry";
/// `knowledge_edit_entry` 工具名称（MCP 桥接同名工具；合并路径：候选命中已有条目时调用）。
pub const EDIT_ENTRY_TOOL_NAME: &str = "knowledge_edit_entry";

// ---------------------------------------------------------------------------
// Provider/Model 解析
// ---------------------------------------------------------------------------

/// 解析场景化模型引用为实际的 (ProviderConfig, model_id)。
///
/// 优先使用 `scene_models.knowledge_build`，回退到全局激活项。
pub fn resolve_kb_provider<'a>(
    llm: &'a LlmConfig,
    model_ref: &SceneModelRef,
) -> Result<(&'a crate::llm_config::model::ProviderConfig, String), KnowledgeBuilderError> {
    if let Some((provider, model_id)) = llm.resolve_scene(model_ref) {
        tracing::info!(
            provider_id = %provider.id,
            model_id = %model_id,
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

// ---------------------------------------------------------------------------
// SubmitPlan 工具（Planning 阶段专用）
// ---------------------------------------------------------------------------

/// `submit_plan` 工具的名称。
pub const SUBMIT_PLAN_TOOL_NAME: &str = "submit_plan";

/// 占位/待补充标记——计划文本中命中任一即判定无效，触发模型重新提取。
///
/// 目的：杜绝"AI 产出占位型计划仍被当成功"的假成功综述/条目（历史上曾出现
/// 综述页与实体名"文献作者/机构（待补充）"这类占位结果）。命中后返回
/// `InvalidArguments`，`chat_until_captured` 会催促模型基于文献正文重提。
const PLAN_PLACEHOLDER_MARKERS: [&str; 5] = ["待补全", "待补充", "占位", "占位性", "TBD"];

/// 检测计划是否含占位式内容；返回命中的第一个标记（`None` 表示内容有效）。
fn plan_has_placeholder(plan: &ExtractionPlan) -> Option<&'static str> {
    let marker_in = |s: &str| {
        PLAN_PLACEHOLDER_MARKERS
            .iter()
            .copied()
            .find(|m| s.contains(*m))
    };
    for point in &plan.summary_points {
        if let Some(m) = marker_in(point) {
            return Some(m);
        }
    }
    for entry in plan.concepts.iter().chain(plan.entities.iter()) {
        if let Some(m) = marker_in(&entry.title) {
            return Some(m);
        }
        if let Some(m) = marker_in(&entry.brief) {
            return Some(m);
        }
    }
    None
}

/// Planning 阶段用于接收 AI 产出的 ExtractionPlan 的工具。
///
/// AI 在 Planning 阶段必须调用此工具提交计划，后端从 tool_call 中解析出
/// 结构化的 ExtractionPlan（不依赖自由文本）。
///
/// 调用 `execute` 时，plan 会被存入 `capture` 共享状态，pipeline 在
/// `engine.chat()` 返回后从中读取。
pub struct SubmitPlanTool {
    name: String,
    description: String,
    parameters: Value,
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
            name: SUBMIT_PLAN_TOOL_NAME.into(),
            description: "提交文献提取计划（Planning 阶段必须调用）".into(),
            parameters,
            capture,
        }
    }
}

#[async_trait]
impl Tool for SubmitPlanTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> Value {
        self.parameters.clone()
    }

    fn default_wait(&self) -> bool {
        true
    }

    async fn execute(&self, _ctx: ToolContext, args: Value) -> Result<ToolOutput, ToolError> {
        let plan: ExtractionPlan = serde_json::from_value(args.clone())
            .map_err(|e| ToolError::InvalidArguments(format!("plan 解析失败: {e}")))?;

        // 校验计划非空：至少要有 summary_points 或 concepts 或 entities
        if plan.summary_points.is_empty()
            && plan.concepts.is_empty()
            && plan.entities.is_empty()
        {
            tracing::warn!("AI 提交了空的 ExtractionPlan");
            return Err(ToolError::InvalidArguments(
                "plan 不能为空：summary_points / concepts / entities 至少需有一项非空".into(),
            ));
        }

        // 校验计划不含占位/待补充内容：占位即假成功，须拒绝并催促重新提取
        if let Some(marker) = plan_has_placeholder(&plan) {
            tracing::warn!(marker, "AI 提交的 ExtractionPlan 含占位内容");
            return Err(ToolError::InvalidArguments(format!(
                "plan 含占位内容「{marker}」：不得使用占位/待补充措辞，请基于文献正文重新提取具体要点"
            )));
        }

        tracing::info!(
            summary_points = plan.summary_points.len(),
            concepts = plan.concepts.len(),
            entities = plan.entities.len(),
            "AI 提交 ExtractionPlan"
        );
        *self.capture.lock().expect("plan capture poisoned") = Some(plan);
        let output = json!({"success": true, "message": "plan received"});
        Ok(ToolOutput::from_json(&output))
    }
}

// ---------------------------------------------------------------------------
// SubmitRelationsTool — EstablishingRelations 阶段提交关联对
// ---------------------------------------------------------------------------

/// `submit_relations` 工具名称。
pub const SUBMIT_RELATIONS_TOOL_NAME: &str = "submit_relations";

/// EstablishingRelations 阶段用于接收 AI 判定关联的工具。
///
/// 新库 MCP `knowledge_edit_entry` 不含 add_relations 语义，关联建立改为
/// AI 结构化提交（wikiID 对 + 可选谓词），Rust 端经 `AsyncKb.merge` 确定性
/// 落库并自动补双向。
pub struct SubmitRelationsTool {
    name: String,
    description: String,
    parameters: Value,
    capture: RelationsCapture,
}

impl SubmitRelationsTool {
    pub fn new(capture: RelationsCapture) -> Self {
        let parameters = json!({
            "type": "object",
            "properties": {
                "relations": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "from": {"type": "string", "description": "起始条目 wikiID（wiki-xxxxxxxxxxxxxxxx）"},
                            "to": {"type": "string", "description": "目标条目 wikiID（wiki-xxxxxxxxxxxxxxxx）"},
                            "predicate": {"type": "string", "description": "可选谓词（默认 related；仅字母/数字/下划线，≤32 字符，可用中文）"}
                        },
                        "required": ["from", "to"]
                    },
                    "description": "关联关系对列表；无需双向提交（系统自动补反向）"
                }
            },
            "required": ["relations"]
        });
        Self {
            name: SUBMIT_RELATIONS_TOOL_NAME.into(),
            description: "提交条目间关联关系（EstablishingRelations 阶段必须调用）".into(),
            parameters,
            capture,
        }
    }
}

#[async_trait]
impl Tool for SubmitRelationsTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> Value {
        self.parameters.clone()
    }

    fn default_wait(&self) -> bool {
        true
    }

    async fn execute(&self, _ctx: ToolContext, args: Value) -> Result<ToolOutput, ToolError> {
        // 按 input_schema 声明解析：顶层为对象，关联对在 `relations` 字段
        let args: RelationsArgs = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArguments(format!("relations 解析失败: {e}")))?;
        let pairs = args.relations;

        if pairs.is_empty() {
            return Err(ToolError::InvalidArguments(
                "relations 不能为空：无关联时请提交空业务判断说明并至少保留一对关联；确实无任何关联时应提交空数组吗？——空数组将被拒绝，请基于条目语义尽量给出关联".into(),
            ));
        }

        for (i, pair) in pairs.iter().enumerate() {
            if fluen_kb::WikiId::parse(&pair.from).is_none() {
                return Err(ToolError::InvalidArguments(format!(
                    "relations[{i}].from 非法 wikiID: {}（必须形如 wiki-xxxxxxxxxxxxxxxx）",
                    pair.from
                )));
            }
            if fluen_kb::WikiId::parse(&pair.to).is_none() {
                return Err(ToolError::InvalidArguments(format!(
                    "relations[{i}].to 非法 wikiID: {}（必须形如 wiki-xxxxxxxxxxxxxxxx）",
                    pair.to
                )));
            }
            if let Some(p) = &pair.predicate {
                if fluen_kb::Predicate::new(p).is_err() {
                    return Err(ToolError::InvalidArguments(format!(
                        "relations[{i}].predicate 非法: {p}（仅允许 1..=32 个字母/数字/下划线字符）"
                    )));
                }
            }
        }

        tracing::info!(count = pairs.len(), "AI 提交关联关系");
        *self.capture.lock().expect("relations capture poisoned") = Some(pairs);
        let output = json!({"success": true, "message": "relations received"});
        Ok(ToolOutput::from_json(&output))
    }
}

// ---------------------------------------------------------------------------
// EntryCaptureGuard — 捕获知识库工具返回的 wiki_id
// ---------------------------------------------------------------------------

/// 装饰器：包装 `knowledge_create_entry` / `knowledge_edit_entry` 工具，
/// `execute` 后捕获 `wiki_id` 写入 capture。
///
/// 替代 confluent 时代的 `CreateEntryObserver`（referee 无 observer 机制，
/// 改用装饰器在工具执行后直接捕获）。
///
/// **必须同时包装两个工具**：Execution 阶段 AI 依据 L2 检索结果二选一——
/// 无相似条目时新建（create_entry），有相似条目时合并（edit_entry）。
///
/// id 提取：
/// - create 路径：从输出提取 `created` / `merged_into`（兼容旧库 `wiki_id`）
/// - edit 路径：MCP 输出为 op 结果数组（如 `[true]`），不含 id，
///   从请求参数 `id` 提取（工具成功返回即条目必然存在）
pub struct EntryCaptureGuard {
    inner: Arc<dyn Tool>,
    capture: CreateEntryCapture,
}

impl EntryCaptureGuard {
    /// 构造装饰器。
    pub fn new(inner: Arc<dyn Tool>, capture: CreateEntryCapture) -> Self {
        Self { inner, capture }
    }

    /// 从工具输出中提取 `wiki_id`。
    ///
    /// 兼容两代输出键：旧库 `wiki_id`；MCP `created`（新建）/
    /// `merged_into`（去重合并）。
    fn extract_wiki_id(output: &ToolOutput) -> Option<String> {
        let value: Value = serde_json::from_str(&output.content).ok()?;
        ["wiki_id", "created", "merged_into"]
            .iter()
            .find_map(|key| value.get(*key).and_then(|v| v.as_str()).map(|s| s.to_string()))
    }
}

#[async_trait]
impl Tool for EntryCaptureGuard {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn input_schema(&self) -> Value {
        self.inner.input_schema()
    }

    fn category(&self) -> ToolCategory {
        self.inner.category()
    }

    fn default_wait(&self) -> bool {
        self.inner.default_wait()
    }

    async fn execute(&self, ctx: ToolContext, args: Value) -> Result<ToolOutput, ToolError> {
        let result = self.inner.execute(ctx, args.clone()).await?;

        // create 路径从输出提取；edit 路径输出不含 id，从请求参数提取
        let wiki_id = Self::extract_wiki_id(&result).or_else(|| {
            if self.inner.name() == EDIT_ENTRY_TOOL_NAME {
                args.get("id").and_then(Value::as_str).map(str::to_string)
            } else {
                None
            }
        });
        if let Some(wiki_id) = wiki_id {
            tracing::debug!(tool = %self.inner.name(), wiki_id = %wiki_id, "捕获条目操作结果");
            *self.capture.lock().expect("create_entry capture poisoned") = Some(wiki_id);
        }

        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// ForceWaitGuard — 强制工具为同步等待语义
// ---------------------------------------------------------------------------

/// 装饰器：覆写 `default_wait() → true`，强制工具按「等待类」执行。
///
/// referee 引擎按 wait 语义分流工具调用：**纯派发轮（本轮全部为不等待工具）
/// 会立即结束回合并返回模型原文，绝不主动发起下一轮 LLM 调用**。
/// 知识库工具未覆写 `default_wait`（trait 默认 false = 派发），若模型先调用
/// `knowledge_query` 查重再提交计划，回合在查询派发后即被终结，模型永远没有
/// 机会调用 `submit_plan` / `create_entry`，流水线随即报"AI 未调用 xxx 工具"。
///
/// 知识库构建是严格顺序的单发智能体循环（每次 `run_chat` 后立即读 capture），
/// 所有工具必须同步收敛结果，故注册时统一包装强制等待。
pub struct ForceWaitGuard {
    inner: Arc<dyn Tool>,
}

impl ForceWaitGuard {
    /// 构造装饰器。
    pub fn new(inner: Arc<dyn Tool>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl Tool for ForceWaitGuard {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn input_schema(&self) -> Value {
        self.inner.input_schema()
    }

    fn category(&self) -> ToolCategory {
        self.inner.category()
    }

    fn default_wait(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: ToolContext, args: Value) -> Result<ToolOutput, ToolError> {
        self.inner.execute(ctx, args).await
    }
}

// ---------------------------------------------------------------------------
// Runtime 构建
// ---------------------------------------------------------------------------

/// 为知识库构建运行时创建一个新的 SessionId。
///
/// 每次构建任务创建独立会话，任务完成后由 pipeline 移除。
pub fn new_session_id() -> SessionId {
    SessionId::new_v4()
}

/// 构建知识库构建专用的 [`FluenRuntime`]。
///
/// 装配：
/// - LLMProvider（基于场景模型，通过 `llm_chat::build_llm_provider`）
/// - 知识库工具（经 [`KbMcpBridge`] 转发 MCP：query / query_batch / create /
///   edit / get_entry，schema 动态取自 MCP `list_tools`）
///   - 全部经 [`ForceWaitGuard`] 强制同步等待（避免纯派发轮被引擎立即终结回合）
///   - `create_entry` / `edit_entry` 另经 [`EntryCaptureGuard`] 包装捕获 `wiki_id`
/// - `submit_plan` 工具（Planning 阶段捕获 ExtractionPlan，自带 `default_wait=true`）
/// - `submit_relations` 工具（EstablishingRelations 阶段捕获关联对，自带 `default_wait=true`）
/// - [`KbReporter`]：工具观测装饰器（`observe_registry`）+ 引擎观察者
///   （`with_observer`），把工具调用与 LLM 增量透传为 kbchat:* 事件
///   （引擎注入观察者后，非流式 `chat()` 在厂商支持流式时自动改走
///   内部流式收敛以产生 delta，厂商不支持时保持直调、零回归）
///
/// `plan_capture` / `entry_capture` / `relations_capture` 由调用方创建并传入，
/// pipeline 在对应阶段执行后从中读取结果。
pub fn build_kb_runtime(
    llm: &LlmConfig,
    model_ref: &SceneModelRef,
    bridge: Arc<KbMcpBridge>,
    plan_capture: PlanCapture,
    entry_capture: CreateEntryCapture,
    relations_capture: RelationsCapture,
    reporter: KbReporter,
) -> Result<(bool, FluenRuntime), KnowledgeBuilderError> {
    let (provider, model_id) = resolve_kb_provider(llm, model_ref)?;
    let llm_provider = llm_chat::build_llm_provider(provider, &model_id)
        .map_err(|e| KnowledgeBuilderError::Config(e.to_string()))?;

    // 模型支持思考时启用思考模式
    let thinking_enabled = provider.model_supports_thinking(&model_id);

    // 装配工具注册表
    let registry = referee_ai::tool::ToolRegistry::with_defaults();

    // 注册 MCP 桥接知识库工具：统一经 ForceWaitGuard 强制同步等待；
    // create_entry / edit_entry 另经 EntryCaptureGuard 捕获 wiki_id
    for tool in McpKnowledgeTool::list_from(&bridge, &KB_BUILD_TOOL_NAMES) {
        let tool_name = tool.name().to_string();
        let tool: Arc<dyn Tool> = if tool_name == "knowledge_create_entry"
            || tool_name == "knowledge_edit_entry"
        {
            Arc::new(EntryCaptureGuard::new(tool, entry_capture.clone()))
        } else {
            tool
        };
        registry
            .register(Arc::new(ForceWaitGuard::new(tool)))
            .map_err(|e| KnowledgeBuilderError::Config(format!("工具注册失败: {e}")))?;
    }

    // 注册 submit_plan 工具
    registry
        .register(Arc::new(SubmitPlanTool::new(plan_capture)))
        .map_err(|e| KnowledgeBuilderError::Config(format!("工具注册失败: {e}")))?;

    // 注册 submit_relations 工具
    registry
        .register(Arc::new(SubmitRelationsTool::new(relations_capture)))
        .map_err(|e| KnowledgeBuilderError::Config(format!("工具注册失败: {e}")))?;

    // 工具观测装饰器：工具执行开始/结束经 KbReporter 透传为 kbchat:* 事件
    // （透明透传，EntryCaptureGuard 等捕获守卫保持内层、捕获语义不变）
    let registry = crate::agent_runtime::observability::observe_registry(
        &registry,
        Arc::new(reporter.clone()) as Arc<dyn ToolEventSink>,
    );

    tracing::debug!(model_id = %model_ref.model_id, "装配 FluenRuntime（知识库构建）");

    let executor = referee_ai::tool::ToolExecutor::with_defaults();

    // 会话预算（runaway 兜底，非成本核算）。
    // 口径实测（2026-09-05，t4 构建 12:04 日志 + agent_runtime::tests 引擎模拟）：
    // referee session_limit 按「每个内部 LLM 轮的 total_tokens」累计——每个等待类
    // 工具调用回合 = 工具调用轮 + 收尾轮（各按全历史计费一次），故实际流水 ≈
    // pipeline 可见 usage（每阶段最后一轮）的 ~2.2×。单篇 13 条目论文实测烧到
    // 1,012,063（limit 1M 触顶），22 条目论文推算 ~2.2M，故取 3M 覆盖全量构建。
    // 该口径把缓存命中部分也按 1× 计（实际成本 ~0.1×），仅作失控护栏使用；
    // 主流框架（MS Agent Framework / AgentBudget / LangChain）的硬门槛分别为
    // 当前上下文体积、美元成本、轮数上限，均不以裸 token 流水为闸门——KB 构建
    // 已有轮数防护（nudge ≤3 次、阶段顺序固定、条目数上限），此处只防极端跑飞。
    // global_limit 置 0：全局计数器挂在 Engine 上，会话池跨论文复用 runtime 时
    // 会累计已销毁会话的消耗（每篇论文都是新会话），该口径无守护意义，
    // 单会话上限才是有效护栏。
    let mut engine_config = referee_ai::engine::EngineConfig::default();
    engine_config.budget.session_limit = 3_000_000;
    engine_config.budget.global_limit = 0;
    // 会话 prompt 预算采用 referee 默认（128K token）：上游已保证"当前轮核心输入恒完整交付、
    // 超预算仅告警"，规划阶段的完整文献不会再被截断，无需在此覆写。
    // KB 构建的 planning 需注入整篇文献并做深度思考，单回合远超默认 30s 的 thinking 超时；
    // 抬高到与传输层 llm_chat::DEFAULT_REQUEST_TIMEOUT(240s) 一致，避免规划回合被过早掐断。
    engine_config.session.timeout.thinking_timeout = std::time::Duration::from_secs(240);
    engine_config.session.timeout.awaiting_calls_timeout = std::time::Duration::from_secs(120);
    let runtime = FluenRuntimeBuilder::new(llm_provider)
        .with_config(engine_config)
        .with_tools(registry, executor)
        .with_observer(Arc::new(reporter.clone()))
        .build();

    tracing::info!(model_id = %model_ref.model_id, "知识库构建 Runtime 构建完成");
    Ok((thinking_enabled, runtime))
}

// ---------------------------------------------------------------------------
// 引擎交互辅助
// ---------------------------------------------------------------------------

/// 发起一轮非流式 Chat 并等待结果，返回 (回复文本, usage)。
///
/// 封装 `engine.chat()` + `handle.wait()` 的典型流程：
/// 1. 构造 `ChatPayload`（system_prompt + 用户消息 + thinking 配置）
/// 2. 发起 `chat()`，等待 `EngineReply`
/// 3. 提取回复文本与 usage
///
/// 取消通过 `cancel.is_cancelled()` 检查（引擎内部中断由 `interrupt` 驱动，
/// 知识库构建为顺序执行，取消在阶段间检查）。
pub async fn run_chat(
    runtime: &FluenRuntime,
    session_id: SessionId,
    prompt: &str,
    system_prompt: &str,
    thinking_enabled: bool,
) -> Result<(String, UsageSnapshot), KnowledgeBuilderError> {
    let payload = ChatPayload {
        message: Message::user(prompt.to_string()),
        options: ChatOptions {
            system_prompt: Some(system_prompt.to_string()),
            thinking: ThinkingConfig {
                enabled: thinking_enabled,
                effort: None,
            },
            ..ChatOptions::default()
        },
        peer_depth: 0,
    };

    let handle = runtime
        .chat(session_id, payload)
        .map_err(|e| KnowledgeBuilderError::Llm(format!("会话启动失败: {e}")))?;

    let reply = handle
        .wait()
        .await
        .ok_or_else(|| KnowledgeBuilderError::Llm("会话通道关闭".into()))?;

    use referee_ai::engine::EngineReply;
    match reply {
        EngineReply::Success(resp) => {
            let text = resp.message.content.as_text().unwrap_or_default().to_string();
            let usage = resp
                .usage
                .as_ref()
                .map(UsageSnapshot::from_token_usage)
                .unwrap_or_default();
            Ok((text, usage))
        }
        EngineReply::Error(err) => {
            tracing::error!(error = %err, "LLM 调用失败");
            Err(KnowledgeBuilderError::Llm(err.to_string()))
        }
        EngineReply::Cancelled => Err(KnowledgeBuilderError::Cancelled),
        EngineReply::Timeout => Err(KnowledgeBuilderError::Llm("会话超时".into())),
        EngineReply::Busy { .. } => Err(KnowledgeBuilderError::Llm("会话忙碌".into())),
        EngineReply::Streaming(_) => {
            // 非流式调用不应返回 Streaming，稳健处理
            Err(KnowledgeBuilderError::Llm("意外的流式响应".into()))
        }
    }
}

/// 从 usage capture 中取出 usage 快照（`engine.chat()` 后调用）。
///
/// 若 capture 为空（如 LLM 未返回 usage 字段），返回零值。
pub fn take_usage(capture: &UsageCapture) -> UsageSnapshot {
    capture
        .lock()
        .expect("usage capture poisoned")
        .take()
        .unwrap_or_default()
}

/// 清空 usage capture（`engine.chat()` 前调用）。
pub fn clear_usage(capture: &UsageCapture) {
    *capture.lock().expect("usage capture poisoned") = None;
}

/// 更新 usage capture（`run_chat` 返回后调用）。
pub fn set_usage(capture: &UsageCapture, usage: UsageSnapshot) {
    *capture.lock().expect("usage capture poisoned") = Some(usage);
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_config::model::{
        ApiStyle, LlmConfig, ModelCapabilities, ModelConfig, ProviderConfig, ProviderType,
        SceneModelRef, SceneModels,
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

    /// 构造测试用的 PlanCapture。
    fn test_capture() -> PlanCapture {
        Arc::new(Mutex::new(None))
    }

    #[test]
    fn submit_plan_tool_name() {
        let tool = SubmitPlanTool::new(test_capture());
        assert_eq!(tool.name(), SUBMIT_PLAN_TOOL_NAME);
    }

    #[tokio::test]
    async fn submit_plan_tool_accepts_valid_plan() {
        let capture = test_capture();
        let tool = SubmitPlanTool::new(capture.clone());
        let ctx = ToolContext {
            tool_call_id: "test".into(),
            session_id: uuid::Uuid::new_v4(),
            turn_id: 0,
            kernel: None,
            store: None,
            wait: true,
            peer_depth: 0,
        };
        let input = json!({
            "summary_points": ["要点1"],
            "concepts": [{"title": "机器学习", "brief": "数据驱动方法"}],
            "entities": []
        });
        let result = tool.execute(ctx, input).await.unwrap();
        assert!(result.content.contains("success"));
        assert!(result.content.contains("true"));
        // capture 应已存入 plan
        let captured = capture.lock().unwrap();
        assert!(captured.is_some());
        assert_eq!(captured.as_ref().unwrap().concepts.len(), 1);
    }

    #[tokio::test]
    async fn submit_plan_tool_rejects_invalid_plan() {
        let tool = SubmitPlanTool::new(test_capture());
        let ctx = ToolContext {
            tool_call_id: "test".into(),
            session_id: uuid::Uuid::new_v4(),
            turn_id: 0,
            kernel: None,
            store: None,
            wait: true,
            peer_depth: 0,
        };
        let input = json!({"foo": "bar"});
        let result = tool.execute(ctx, input).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn submit_plan_tool_rejects_placeholder_points() {
        // 占位要点虽"非空"，也必须被拒绝（防假成功占位综述）
        let tool = SubmitPlanTool::new(test_capture());
        let ctx = ToolContext {
            tool_call_id: "test".into(),
            session_id: uuid::Uuid::new_v4(),
            turn_id: 0,
            kernel: None,
            store: None,
            wait: true,
            peer_depth: 0,
        };
        let input = json!({
            "summary_points": ["该文献研究通用人工智能驱动的多智能体仿真，具体方法与结论待补全"],
            "concepts": [],
            "entities": []
        });
        let err = tool.execute(ctx, input).await.unwrap_err().to_string();
        assert!(err.contains("占位"), "应为占位错误，得到: {err}");
        assert!(err.contains("待补全"), "应指出命中标记，得到: {err}");
    }

    #[tokio::test]
    async fn submit_plan_tool_rejects_placeholder_entry_title() {
        // 实体/概念标题也不允许占位（如"文献作者/机构（待补充）"）
        let tool = SubmitPlanTool::new(test_capture());
        let ctx = ToolContext {
            tool_call_id: "test".into(),
            session_id: uuid::Uuid::new_v4(),
            turn_id: 0,
            kernel: None,
            store: None,
            wait: true,
            peer_depth: 0,
        };
        let input = json!({
            "summary_points": ["要点1"],
            "concepts": [],
            "entities": [{"title": "文献作者/机构（待补充）", "brief": "作者信息"}]
        });
        let err = tool.execute(ctx, input).await.unwrap_err().to_string();
        assert!(err.contains("待补充"), "实体占位标题应被拒绝，得到: {err}");
    }

    #[tokio::test]
    async fn submit_relations_tool_accepts_schema_shaped_args() {
        // 回归：input_schema 声明顶层为 {"relations": [...]}，
        // execute 必须按此解析（此前按裸数组解析，schema 合规调用全被拒）
        let capture: RelationsCapture = Arc::new(Mutex::new(None));
        let tool = SubmitRelationsTool::new(capture.clone());
        let ctx = ToolContext {
            tool_call_id: "test".into(),
            session_id: uuid::Uuid::new_v4(),
            turn_id: 0,
            kernel: None,
            store: None,
            wait: true,
            peer_depth: 0,
        };
        let input = json!({
            "relations": [
                {"from": "wiki-0123456789abcdef", "to": "wiki-fedcba9876543210", "predicate": "related"}
            ]
        });
        tool.execute(ctx, input).await.unwrap();
        let pairs = capture.lock().unwrap().take().unwrap();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].from, "wiki-0123456789abcdef");
        assert_eq!(pairs[0].to, "wiki-fedcba9876543210");
    }

    #[tokio::test]
    async fn submit_relations_tool_rejects_bare_array() {
        // 裸数组不符合 schema（顶层必须为对象），应被拒绝
        let capture: RelationsCapture = Arc::new(Mutex::new(None));
        let tool = SubmitRelationsTool::new(capture.clone());
        let ctx = ToolContext {
            tool_call_id: "test".into(),
            session_id: uuid::Uuid::new_v4(),
            turn_id: 0,
            kernel: None,
            store: None,
            wait: true,
            peer_depth: 0,
        };
        let input = json!([{"from": "wiki-0123456789abcdef", "to": "wiki-fedcba9876543210"}]);
        let result = tool.execute(ctx, input).await;
        assert!(result.is_err());
        assert!(capture.lock().unwrap().is_none());
    }

    #[test]
    fn entry_capture_guard_extracts_wiki_id() {
        let output = ToolOutput::from_json(&json!({
            "success": true,
            "wiki_id": "wiki-abc123",
        }));
        let wiki_id = EntryCaptureGuard::extract_wiki_id(&output);
        assert_eq!(wiki_id.as_deref(), Some("wiki-abc123"));
    }

    #[test]
    fn entry_capture_guard_returns_none_without_wiki_id() {
        let output = ToolOutput::from_json(&json!({
            "success": true,
        }));
        let wiki_id = EntryCaptureGuard::extract_wiki_id(&output);
        assert!(wiki_id.is_none());
    }

    /// 固定名称/输出的内层工具（模拟 MCP 桥接工具）。
    struct FixedOutputTool {
        name: String,
        output: ToolOutput,
    }

    #[async_trait]
    impl Tool for FixedOutputTool {
        fn name(&self) -> &str {
            &self.name
        }
        fn description(&self) -> &str {
            "fixed output"
        }
        fn input_schema(&self) -> Value {
            json!({"type": "object"})
        }
        async fn execute(&self, _ctx: ToolContext, _args: Value) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput {
                content: self.output.content.clone(),
            })
        }
    }

    #[tokio::test]
    async fn entry_capture_guard_captures_edit_id_from_args() {
        // 回归：MCP edit_entry 输出为 op 结果数组（如 [true]），不含 id；
        // 守卫必须从请求参数 id 捕获，否则合并路径永远"未调用工具"
        let capture: CreateEntryCapture = Arc::new(Mutex::new(None));
        let inner = Arc::new(FixedOutputTool {
            name: EDIT_ENTRY_TOOL_NAME.into(),
            output: ToolOutput::from_json(&json!([true])),
        });
        let guard = EntryCaptureGuard::new(inner, capture.clone());
        let ctx = ToolContext {
            tool_call_id: "test".into(),
            session_id: uuid::Uuid::new_v4(),
            turn_id: 0,
            kernel: None,
            store: None,
            wait: true,
            peer_depth: 0,
        };
        let args = json!({"id": "wiki-97a3d93153364a34", "ops": []});
        guard.execute(ctx, args).await.unwrap();
        assert_eq!(capture.lock().unwrap().as_deref(), Some("wiki-97a3d93153364a34"));
    }

    #[tokio::test]
    async fn entry_capture_guard_create_path_prefers_output_id() {
        // create 路径：输出含 created，优先从输出提取
        let capture: CreateEntryCapture = Arc::new(Mutex::new(None));
        let inner = Arc::new(FixedOutputTool {
            name: CREATE_ENTRY_TOOL_NAME.into(),
            output: ToolOutput::from_json(&json!({"created": "wiki-2ecaf57b0b554a43"})),
        });
        let guard = EntryCaptureGuard::new(inner, capture.clone());
        let ctx = ToolContext {
            tool_call_id: "test".into(),
            session_id: uuid::Uuid::new_v4(),
            turn_id: 0,
            kernel: None,
            store: None,
            wait: true,
            peer_depth: 0,
        };
        let args = json!({"type": "summary", "title": "t", "body": "b"});
        guard.execute(ctx, args).await.unwrap();
        assert_eq!(capture.lock().unwrap().as_deref(), Some("wiki-2ecaf57b0b554a43"));
    }

    /// 默认不等待的工具（模拟未覆写 default_wait 的知识库工具）
    struct AsyncByDefaultTool;

    #[async_trait]
    impl Tool for AsyncByDefaultTool {
        fn name(&self) -> &str {
            "async_default"
        }
        fn description(&self) -> &str {
            "dispatched by default"
        }
        fn input_schema(&self) -> Value {
            json!({"type": "object"})
        }
        async fn execute(&self, _ctx: ToolContext, _args: Value) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::text("ok"))
        }
    }

    #[test]
    fn force_wait_guard_overrides_default_wait_and_delegates_metadata() {
        let inner = AsyncByDefaultTool;
        assert!(!inner.default_wait(), "前置：未覆写的工具默认派发");

        let guard = ForceWaitGuard::new(Arc::new(inner));
        assert!(guard.default_wait(), "装饰后必须强制等待");
        assert_eq!(guard.name(), "async_default");
        assert_eq!(guard.description(), "dispatched by default");
    }

    #[tokio::test]
    async fn force_wait_guard_delegates_execute() {
        let guard = ForceWaitGuard::new(Arc::new(AsyncByDefaultTool));
        let ctx = ToolContext {
            tool_call_id: "test".into(),
            session_id: uuid::Uuid::new_v4(),
            turn_id: 0,
            kernel: None,
            store: None,
            wait: true,
            peer_depth: 0,
        };
        let output = guard.execute(ctx, json!({})).await.unwrap();
        assert_eq!(output.content, "ok");
    }

    #[test]
    fn usage_snapshot_total_sums_prompt_and_completion() {
        let snap = UsageSnapshot {
            prompt_tokens: 1000,
            completion_tokens: 500,
        };
        assert_eq!(snap.total(), 1500);
    }

    #[test]
    fn usage_snapshot_default_is_zero() {
        let snap = UsageSnapshot::default();
        assert_eq!(snap.total(), 0);
    }
}
