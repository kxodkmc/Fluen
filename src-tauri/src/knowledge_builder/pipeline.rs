//! 两阶段流水线：文献 MD → AI 提取 → 知识库条目。
//!
//! ## 设计（V2.1）
//!
//! - **阶段 1（Planning）**：注入 Index 快照（全局去重视图），AI 阅读全文，产出 [`ExtractionPlan`]。
//! - **阶段 2（Execution）**：按计划逐条创建 summary / concept / entity，
//!   每条创建前做 L2 混合检索查重，每次 `engine.chat()` 后从真实 usage 更新会话预算。
//!   最后单独执行 relations 阶段。
//!
//! ## 状态机
//!
//! 通过 [`BuildStage`] 驱动恢复，保证 relations 不被跳过：
//!
//! ```text
//! Planning → CreatingSummary → CreatingConcepts
//!         → CreatingEntities → EstablishingRelations → Done
//! ```
//!
//! ## V2.1 关键改进
//!
//! - **Index 快照注入**：Planning 阶段注入 [`IndexSnapshot`]，AI 拥有全局去重视图。
//! - **L2 混合检索**：每条创建前查询已有条目（Top 3），注入候选供 AI 决策"更新 vs 新建"。
//! - **真实 usage 跟踪**：每次 `engine.chat()` 后从 [`UsageSnapshot`] 读取 LLM 返回的
//!   `prompt_tokens + completion_tokens`，更新 [`KnowledgeBuildSession`] 的 `history_used`。
//! - **会话复用**：runtime 由调用方（runner）通过 [`KnowledgeBuildSession`] 传入，
//!   跨论文复用以命中模型前缀缓存。
//!
//! ## referee 迁移变化
//!
//! - `ConfluentRuntime.run(input)` → `FluenRuntime.chat(session_id, payload)` + `handle.wait()`
//! - `RuntimeObserver` → `EntryCaptureGuard` 装饰器在工具执行时自动写 capture
//! - `UsageObserver` → `run_chat()` 直接返回 `UsageSnapshot`
//! - 每次 `engine.chat()` 使用独立 `SessionId`（由 `new_session_id()` 创建）

use std::path::Path;
use tokio_util::sync::CancellationToken;

use fluen_kb::async_kb::AsyncKb;
use fluen_kb::ids::{Predicate, WikiId, WikiType};
use fluen_kb::search::QueryParams;
use referee_ai::session::SessionId;

use crate::agent_runtime::FluenRuntime;
use crate::llm_config::model::LlmConfig;

use super::chat_retry::chat_until_captured;
use super::error::KnowledgeBuilderError;
use super::events::KbBuildProgressPayload;
use super::index_snapshot::IndexSnapshot;
use super::llm_helper::{
    new_session_id, CreateEntryCapture, PlanCapture, RelationsCapture, RelationPair, UsageCapture,
    UsageSnapshot,
};
use super::prompts::{
    render_create_concept, render_create_entity, render_create_summary,
    render_establish_entry_relations, render_planning, L2Candidate,
};
use super::session::KnowledgeBuildSession;
use super::types::{
    BuildStage, ExtractionPlan, KnowledgeBuildCheckpoint, KnowledgeBuildOptions, PlannedEntry,
};

// ---------------------------------------------------------------------------
// 主入口
// ---------------------------------------------------------------------------

/// 知识库构建流水线主体（由 [`build`] 调用，共享会话贯穿全流程）。
///
/// 按 [`BuildStage`] 状态机推进，每个阶段完成后更新 `checkpoint`。
/// 调用方（runner）负责持久化 checkpoint 与会话生命周期管理（会话回收在
/// [`build`] 公共入口统一处理）。
///
/// # 参数
///
/// - `project_path`：项目根路径
/// - `ref_id`：文献 ID
/// - `options`：构建选项
/// - `checkpoint`：断点信息（可变引用，被原地更新）
/// - `llm_config`：LLM 配置（用于构建 L2 检索用的 KB）
/// - `session`：跨论文会话（提供 runtime 与 usage 跟踪）
/// - `conversation`：本文献的共享模型会话（单会话连续模式）
/// - `plan_capture` / `entry_capture` / `usage_capture`：与 session runtime 绑定的共享捕获
/// - `cancel`：取消令牌
/// - `on_progress`：进度回调
async fn build_flow(
    project_path: &Path,
    ref_id: &str,
    options: &KnowledgeBuildOptions,
    checkpoint: &mut KnowledgeBuildCheckpoint,
    llm_config: &LlmConfig,
    session: &mut KnowledgeBuildSession,
    conversation: &SessionId,
    plan_capture: &PlanCapture,
    entry_capture: &CreateEntryCapture,
    relations_capture: &RelationsCapture,
    usage_capture: &UsageCapture,
    cancel: &CancellationToken,
    on_progress: impl Fn(KbBuildProgressPayload),
) -> Result<(), KnowledgeBuilderError> {
    tracing::info!(
        ref_id = %ref_id,
        stage = ?checkpoint.stage,
        options = ?options,
        history_used = session.history_used(),
        "知识库构建启动（V2.1）"
    );

    // 初始化知识库（用于 L2 检索与关联落库，与 session 桥接的 KB 共享同一 DB）
    let refs_dir = project_path.join("references");
    let kb = super::kb_adapter::init_kb_async(&refs_dir, llm_config)?;

    // ── 实时读取 Index 快照（每次构建任务从磁盘解析，确保最新状态）──
    let index_md = read_index_md(&refs_dir);
    let snapshot = IndexSnapshot::parse(&index_md)?;
    tracing::info!(
        ref_id = %ref_id,
        total_entries = snapshot.total_entries(),
        estimated_tokens = snapshot.estimated_tokens(),
        "Index 快照已解析"
    );

    // ── 阶段 1：Planning（注入快照）──────────────────────────────
    if checkpoint.stage == BuildStage::Planning {
        check_cancel(cancel)?;
        on_progress(progress(ref_id, "planning", 0, None));
        tracing::info!(ref_id = %ref_id, "阶段 Planning 开始");

        let md_content = read_md_content(project_path, ref_id)?;
        let (plan, usage) = run_planning(
            session.runtime(),
            &conversation,
            session.thinking_enabled(),
            &md_content,
            options,
            &snapshot,
            plan_capture,
            usage_capture,
            cancel,
        )
        .await?;

        session.update_usage(usage);

        tracing::info!(
            ref_id = %ref_id,
            summary_points = plan.summary_points.len(),
            concepts = plan.concepts.len(),
            entities = plan.entities.len(),
            history_used = session.history_used(),
            "阶段 Planning 完成"
        );
        checkpoint.plan = Some(plan);
        checkpoint.stage = BuildStage::CreatingSummary;
    }

    let plan = checkpoint
        .plan
        .as_ref()
        .ok_or_else(|| KnowledgeBuilderError::StateMachine("缺少 ExtractionPlan".into()))?
        .clone();

    // ── 阶段 2a：CreatingSummary（L2 检索 + usage 更新）──────────
    if checkpoint.stage == BuildStage::CreatingSummary && options.create_summary {
        check_cancel(cancel)?;
        on_progress(progress(ref_id, "creating_summary", 0, None));
        tracing::info!(ref_id = %ref_id, points = plan.summary_points.len(), "阶段 CreatingSummary 开始");

        let candidates = l2_query(&kb, ref_id, Some(WikiType::Summary)).await?;
        let (summary_id, usage) = run_create_summary(
            session.runtime(),
            &conversation,
            session.thinking_enabled(),
            entry_capture,
            usage_capture,
            ref_id,
            &plan.summary_points,
            &candidates,
        )
        .await?;

        session.update_usage(usage);
        tracing::info!(ref_id = %ref_id, summary_id = %summary_id, history_used = session.history_used(), "阶段 CreatingSummary 完成");
        checkpoint.summary_id = Some(summary_id);
        checkpoint.stage = BuildStage::CreatingConcepts;
    }

    // ── 阶段 2b：CreatingConcepts（逐条 L2 检索 + usage 更新）────
    if checkpoint.stage == BuildStage::CreatingConcepts && options.create_concepts {
        tracing::info!(
            ref_id = %ref_id,
            total = plan.concepts.len(),
            already_created = checkpoint.concept_ids.len(),
            "阶段 CreatingConcepts 开始"
        );
        for (i, planned) in plan.concepts.iter().enumerate() {
            check_cancel(cancel)?;

            // 跳过已存在的（plan 标记了 existing_id）
            if planned.existing_id.is_some() {
                tracing::debug!(ref_id = %ref_id, idx = i, title = %planned.title, "跳过已存在 concept");
                continue;
            }
            // 跳过本任务已创建的（中断恢复）
            if i < checkpoint.concept_ids.len() {
                continue;
            }

            on_progress(progress(
                ref_id,
                "creating_concepts",
                i,
                Some(plan.concepts.len()),
            ));
            tracing::debug!(ref_id = %ref_id, idx = i, title = %planned.title, "创建 concept");

            let candidates = l2_query(&kb, &planned.title, Some(WikiType::Concept)).await?;
            let (id, usage) = run_create_concept(
                session.runtime(),
                &conversation,
                session.thinking_enabled(),
                entry_capture,
                usage_capture,
                ref_id,
                planned,
                &candidates,
                cancel,
            )
            .await?;
            session.update_usage(usage);

            tracing::debug!(ref_id = %ref_id, idx = i, wiki_id = %id, history_used = session.history_used(), "concept 创建成功");
            checkpoint.concept_ids.push(id);
        }
        tracing::info!(ref_id = %ref_id, created = checkpoint.concept_ids.len(), "阶段 CreatingConcepts 完成");
        checkpoint.stage = BuildStage::CreatingEntities;
    }

    // ── 阶段 2c：CreatingEntities（逐条 L2 检索 + usage 更新）────
    if checkpoint.stage == BuildStage::CreatingEntities && options.create_entities {
        tracing::info!(
            ref_id = %ref_id,
            total = plan.entities.len(),
            already_created = checkpoint.entity_ids.len(),
            "阶段 CreatingEntities 开始"
        );
        for (i, planned) in plan.entities.iter().enumerate() {
            check_cancel(cancel)?;

            if planned.existing_id.is_some() {
                tracing::debug!(ref_id = %ref_id, idx = i, title = %planned.title, "跳过已存在 entity");
                continue;
            }
            if i < checkpoint.entity_ids.len() {
                continue;
            }

            on_progress(progress(
                ref_id,
                "creating_entities",
                i,
                Some(plan.entities.len()),
            ));
            tracing::debug!(ref_id = %ref_id, idx = i, title = %planned.title, "创建 entity");

            let candidates = l2_query(&kb, &planned.title, Some(WikiType::Entity)).await?;
            let (id, usage) = run_create_entity(
                session.runtime(),
                &conversation,
                session.thinking_enabled(),
                entry_capture,
                usage_capture,
                ref_id,
                planned,
                &candidates,
                cancel,
            )
            .await?;
            session.update_usage(usage);

            tracing::debug!(ref_id = %ref_id, idx = i, wiki_id = %id, history_used = session.history_used(), "entity 创建成功");
            checkpoint.entity_ids.push(id);
        }
        tracing::info!(ref_id = %ref_id, created = checkpoint.entity_ids.len(), "阶段 CreatingEntities 完成");
        checkpoint.stage = BuildStage::EstablishingRelations;
    }

    // ── 阶段 2d：EstablishingRelations（独立阶段）────────────────
    //
    // 分两步：
    //   1. Summary 关联：Rust 端收集所有 concept/entity 的 wikiID，
    //      直接调用 kb.edit_entry 写入（确定性关联，不依赖 AI）。
    //   2. Concept/Entity 关联：构造「wikiID → 标题」对照表传入 AI，
    //      AI 判断语义关联后调用 edit_entry 工具用 wikiID 写入。
    if checkpoint.stage == BuildStage::EstablishingRelations
        && !checkpoint.relations_established
        && options.auto_relations
    {
        check_cancel(cancel)?;
        on_progress(progress(ref_id, "establishing_relations", 0, None));

        let summary_id = checkpoint
            .summary_id
            .as_ref()
            .ok_or_else(|| KnowledgeBuilderError::StateMachine("缺少 summary_id".into()))?;

        // ── 步骤 1：Summary 关联（确定性，Rust 端直接写入）──
        let mut summary_related: Vec<String> = Vec::new();
        for c in &plan.concepts {
            if let Some(id) = &c.existing_id {
                summary_related.push(id.clone());
            }
        }
        for e in &plan.entities {
            if let Some(id) = &e.existing_id {
                summary_related.push(id.clone());
            }
        }
        summary_related.extend(checkpoint.concept_ids.iter().cloned());
        summary_related.extend(checkpoint.entity_ids.iter().cloned());
        summary_related.sort();
        summary_related.dedup();

        if !summary_related.is_empty() {
            tracing::info!(
                ref_id = %ref_id,
                summary_id = %summary_id,
                related_count = summary_related.len(),
                "写入 summary 关联"
            );
            let summary_wiki_id = WikiId::new(summary_id)?;
            let relations = summary_related
                .iter()
                .map(|id| Ok((Predicate::related(), WikiId::new(id)?)))
                .collect::<Result<Vec<_>, KnowledgeBuilderError>>()?;
            kb.merge(summary_wiki_id, String::new(), relations)
                .await?;
        }

        // ── 步骤 2：Concept/Entity 关联（AI 判断，工具写入）──
        // 构造「wikiID → 类型 → 标题」对照表
        let mut entries_table: Vec<(String, &'static str, String)> = Vec::new();

        // summary
        if let Some(sid) = &checkpoint.summary_id {
            entries_table.push((sid.clone(), "summary", "文献综述".into()));
        }
        // concepts：existing_id（已有）+ concept_ids（新建），标题来自 plan
        for (i, c) in plan.concepts.iter().enumerate() {
            let id = c
                .existing_id
                .clone()
                .or_else(|| checkpoint.concept_ids.get(i).cloned());
            if let Some(id) = id {
                entries_table.push((id, "concept", c.title.clone()));
            }
        }
        // entities：同上
        for (i, e) in plan.entities.iter().enumerate() {
            let id = e
                .existing_id
                .clone()
                .or_else(|| checkpoint.entity_ids.get(i).cloned());
            if let Some(id) = id {
                entries_table.push((id, "entity", e.title.clone()));
            }
        }

        if entries_table.len() >= 2 {
            tracing::info!(
                ref_id = %ref_id,
                entry_count = entries_table.len(),
                "AI 建立 concept/entity 关联"
            );
            let (pairs, usage) = run_establish_entry_relations(
                session.runtime(),
                &conversation,
                session.thinking_enabled(),
                relations_capture,
                usage_capture,
                &entries_table,
                cancel,
            )
            .await?;
            session.update_usage(usage);

            // Rust 端确定性落库：merge 关联并自动补双向
            apply_relations(&kb, &pairs).await?;
        }

        tracing::info!(ref_id = %ref_id, history_used = session.history_used(), "阶段 EstablishingRelations 完成");
        checkpoint.relations_established = true;
        checkpoint.stage = BuildStage::Done;
    }

    // 若跳过了某些阶段（options 关闭），直接置 Done
    if checkpoint.stage != BuildStage::Done {
        tracing::debug!(ref_id = %ref_id, stage = ?checkpoint.stage, "未执行的阶段被跳过，直接置 Done");
        checkpoint.stage = BuildStage::Done;
    }

    session.record_paper(ref_id);
    tracing::info!(
        ref_id = %ref_id,
        summary_id = ?checkpoint.summary_id,
        concepts = checkpoint.concept_ids.len(),
        entities = checkpoint.entity_ids.len(),
        relations = checkpoint.relations_established,
        papers_processed = session.papers_processed(),
        history_used = session.history_used(),
        "知识库构建完成"
    );
    Ok(())
}

/// 执行知识库构建流水线（单会话连续模式的公共入口）。
///
/// 为整篇文献创建一个**共享模型会话**并从 Planning 一直用到关联建立结束：
/// 原文只在规划那一轮进入上下文一次，创建条目（综述/概念/实体/关联）通过历史里
/// 已有原文来保证"原文优先"，不再每步重复注入整篇原文。
///
/// 本函数用 `build_flow` 包裹，并**保证会话无论成功还是失败都在返回前被回收**
/// （RAII 语义），避免中途失败（如用户取消、LLM 错误）时模型会话泄漏。
///
/// # 参数
///
/// 同 [`build_flow`]。
#[allow(clippy::too_many_arguments)]
pub async fn build(
    project_path: &Path,
    ref_id: &str,
    options: &KnowledgeBuildOptions,
    checkpoint: &mut KnowledgeBuildCheckpoint,
    llm_config: &LlmConfig,
    session: &mut KnowledgeBuildSession,
    plan_capture: &PlanCapture,
    entry_capture: &CreateEntryCapture,
    relations_capture: &RelationsCapture,
    usage_capture: &UsageCapture,
    cancel: &CancellationToken,
    on_progress: impl Fn(KbBuildProgressPayload),
) -> Result<(), KnowledgeBuilderError> {
    let conversation = new_session_id();
    let result = build_flow(
        project_path,
        ref_id,
        options,
        checkpoint,
        llm_config,
        session,
        &conversation,
        plan_capture,
        entry_capture,
        relations_capture,
        usage_capture,
        cancel,
        on_progress,
    )
    .await;
    // 无论成败统一回收共享会话
    session.runtime().remove_session(conversation);
    result
}

// ---------------------------------------------------------------------------
// 阶段实现
// ---------------------------------------------------------------------------
// 系统提示词与催促重试闭环见 chat_retry 模块。

/// Planning 阶段：AI 阅读完整原文 + Index 快照，产出 ExtractionPlan。
///
/// 规划阶段**直接使用完整文献原文**（不做 AI 预摘要——预摘要会丢失文献信息、
/// 诱发占位/失真，见占位综述问题的根因修复）。`md_content` 已由调用方从
/// `references/md/{ref_id}.md` 读入。
/// 返回 `(plan, usage)`，usage 用于更新会话的真实上下文占用。
async fn run_planning(
    runtime: &FluenRuntime,
    session_id: &SessionId,
    thinking_enabled: bool,
    md_content: &str,
    options: &KnowledgeBuildOptions,
    snapshot: &IndexSnapshot,
    plan_capture: &PlanCapture,
    usage_capture: &UsageCapture,
    cancel: &CancellationToken,
) -> Result<(ExtractionPlan, UsageSnapshot), KnowledgeBuilderError> {
    tracing::info!(chars = md_content.chars().count(), "Planning 文献长度");

    // 可追溯埋点：回显真正注入规划阶段的文献原文前缀，供核对「模型收到的内容」是否与源 MD 一致。
    tracing::debug!(
        md_chars = md_content.chars().count(),
        md_prefix = %md_preview(md_content, 240),
        "Planning 注入的文献原文前缀"
    );

    // 渲染 prompt（注入完整原文 + Index 快照）
    let prompt = render_planning(md_content, options, snapshot);
    tracing::debug!(prompt_chars = prompt.chars().count(), "Planning prompt 渲染完成");

    // 清空 capture（循环内由闭包逐次取出）
    {
        let mut cap = plan_capture.lock().unwrap();
        *cap = None;
    }

    // 复用共享会话：全文原文在这一轮进入历史，供后续创建条目沿用（不重复注入）
    let (plan, usage) = chat_until_captured(
        runtime,
        thinking_enabled,
        session_id.clone(),
        &prompt,
        "请立即调用 submit_plan 工具提交文献提取计划（包含 summary_points / concepts / entities）",
        || plan_capture.lock().expect("plan capture poisoned").take(),
    )
    .await?;

    // 取消检查
    if cancel.is_cancelled() {
        return Err(KnowledgeBuilderError::Cancelled);
    }

    tracing::debug!(
        summary_points = ?plan.summary_points,
        concepts = ?plan.concepts.iter().map(|c| &c.title).collect::<Vec<_>>(),
        entities = ?plan.entities.iter().map(|e| &e.title).collect::<Vec<_>>(),
        prompt_tokens = usage.prompt_tokens,
        completion_tokens = usage.completion_tokens,
        "Planning 已捕获 ExtractionPlan"
    );

    // 记录 usage 到 capture（供 session.update_usage 读取）
    super::llm_helper::set_usage(usage_capture, usage);

    Ok((plan, usage))
}

/// 创建 summary 条目。
///
/// AI 调用 `knowledge_create_entry`（新建）或 `knowledge_edit_entry`
/// （合并到已有相似条目），[`super::llm_helper::EntryCaptureGuard`]
/// 自动捕获返回的 `wiki_id`。返回 `(wiki_id, usage)`。
async fn run_create_summary(
    runtime: &FluenRuntime,
    session_id: &SessionId,
    thinking_enabled: bool,
    entry_capture: &CreateEntryCapture,
    usage_capture: &UsageCapture,
    ref_id: &str,
    summary_points: &[String],
    candidates: &[L2Candidate],
) -> Result<(String, UsageSnapshot), KnowledgeBuilderError> {
    let prompt = render_create_summary(ref_id, summary_points, candidates);

    clear_capture(entry_capture);

    // 复用共享会话：原文已在历史中，本函数只追加综述创建提示
    let (wiki_id, usage) = chat_until_captured(
        runtime,
        thinking_enabled,
        session_id.clone(),
        &prompt,
        "请立即调用 knowledge_create_entry（新建）或 knowledge_edit_entry（合并）工具完成 summary 条目写入",
        || entry_capture.lock().expect("entry capture poisoned").take(),
    )
    .await?;

    super::llm_helper::set_usage(usage_capture, usage);
    Ok((wiki_id, usage))
}

/// 创建单个 concept 条目。
///
/// AI 调用 `knowledge_create_entry`（新建）或 `knowledge_edit_entry`
/// （合并到已有相似条目），[`super::llm_helper::EntryCaptureGuard`]
/// 自动捕获返回的 `wiki_id`。返回 `(wiki_id, usage)`。
async fn run_create_concept(
    runtime: &FluenRuntime,
    session_id: &SessionId,
    thinking_enabled: bool,
    entry_capture: &CreateEntryCapture,
    usage_capture: &UsageCapture,
    ref_id: &str,
    planned: &PlannedEntry,
    candidates: &[L2Candidate],
    cancel: &CancellationToken,
) -> Result<(String, UsageSnapshot), KnowledgeBuilderError> {
    let prompt = render_create_concept(ref_id, &planned.title, &planned.brief, candidates);

    clear_capture(entry_capture);

    // 复用共享会话：原文已在历史中，本函数只追加概念创建提示
    let (wiki_id, usage) = chat_until_captured(
        runtime,
        thinking_enabled,
        session_id.clone(),
        &prompt,
        &format!(
            "请立即调用 knowledge_create_entry（新建）或 knowledge_edit_entry（合并）工具完成 concept 条目「{}」的写入",
            planned.title
        ),
        || entry_capture.lock().expect("entry capture poisoned").take(),
    )
    .await?;

    if cancel.is_cancelled() {
        return Err(KnowledgeBuilderError::Cancelled);
    }

    super::llm_helper::set_usage(usage_capture, usage);
    Ok((wiki_id, usage))
}

/// 创建单个 entity 条目。
///
/// AI 调用 `knowledge_create_entry`（新建）或 `knowledge_edit_entry`
/// （合并到已有相似条目），[`super::llm_helper::EntryCaptureGuard`]
/// 自动捕获返回的 `wiki_id`。返回 `(wiki_id, usage)`。
async fn run_create_entity(
    runtime: &FluenRuntime,
    session_id: &SessionId,
    thinking_enabled: bool,
    entry_capture: &CreateEntryCapture,
    usage_capture: &UsageCapture,
    ref_id: &str,
    planned: &PlannedEntry,
    candidates: &[L2Candidate],
    cancel: &CancellationToken,
) -> Result<(String, UsageSnapshot), KnowledgeBuilderError> {
    let prompt = render_create_entity(ref_id, &planned.title, &planned.brief, candidates);

    clear_capture(entry_capture);

    // 复用共享会话：原文已在历史中，本函数只追加实体创建提示
    let (wiki_id, usage) = chat_until_captured(
        runtime,
        thinking_enabled,
        session_id.clone(),
        &prompt,
        &format!(
            "请立即调用 knowledge_create_entry（新建）或 knowledge_edit_entry（合并）工具完成 entity 条目「{}」的写入",
            planned.title
        ),
        || entry_capture.lock().expect("entry capture poisoned").take(),
    )
    .await?;

    if cancel.is_cancelled() {
        return Err(KnowledgeBuilderError::Cancelled);
    }

    super::llm_helper::set_usage(usage_capture, usage);
    Ok((wiki_id, usage))
}

/// AI 驱动的 concept/entity 关联建立。
///
/// 传入「wikiID → 类型 → 标题」对照表，AI 基于标题判断语义关联，
/// 通过 `submit_relations` 工具结构化提交关联对（Rust 端负责落库与补双向）。
/// 返回 `(关联对, usage)`。
async fn run_establish_entry_relations(
    runtime: &FluenRuntime,
    session_id: &SessionId,
    thinking_enabled: bool,
    relations_capture: &RelationsCapture,
    usage_capture: &UsageCapture,
    entries: &[(String, &str, String)],
    cancel: &CancellationToken,
) -> Result<(Vec<RelationPair>, UsageSnapshot), KnowledgeBuilderError> {
    let prompt = render_establish_entry_relations(entries);

    // 清空 capture（循环内由闭包逐次取出）
    {
        let mut cap = relations_capture.lock().unwrap();
        *cap = None;
    }

    // 复用共享会话：关联判定在同一下上文完成，无独立历史
    let (pairs, usage) = chat_until_captured(
        runtime,
        thinking_enabled,
        session_id.clone(),
        &prompt,
        "请立即调用 submit_relations 工具提交条目间关联关系（from/to 为 wikiID，无需双向提交）",
        || relations_capture.lock().expect("relations capture poisoned").take(),
    )
    .await?;

    if cancel.is_cancelled() {
        return Err(KnowledgeBuilderError::Cancelled);
    }

    super::llm_helper::set_usage(usage_capture, usage);
    Ok((pairs, usage))
}

// ---------------------------------------------------------------------------
// L2 混合检索
// ---------------------------------------------------------------------------

/// L2 混合检索：每条创建前查询已有条目（Top 3），供 AI 决策"更新 vs 新建"。
///
/// 默认使用 hybrid 检索（向量 + 关键词）；Embedding 未配置时自动降级为关键词检索。
/// 返回紧凑的 [`L2Candidate`] 列表（含 wiki_id 供 AI 合并调用）。
async fn l2_query(
    kb: &AsyncKb,
    query: &str,
    wiki_type: Option<WikiType>,
) -> Result<Vec<L2Candidate>, KnowledgeBuilderError> {
    let params = QueryParams {
        query: query.to_string(),
        wiki_type,
        method: fluen_kb::search::SearchMethod::Hybrid,
        top_k: Some(3),
        include_content: false,
        expand: 0,
    };
    let hits = kb.query(params).await.map_err(KnowledgeBuilderError::from)?;
    let candidates: Vec<L2Candidate> = hits
        .into_iter()
        .map(|h| L2Candidate {
            wiki_id: h.meta.id.as_str().to_string(),
            title: h.meta.title,
            score: h.score,
        })
        .collect();
    tracing::debug!(query = %query, count = candidates.len(), "L2 检索完成");
    Ok(candidates)
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 将 AI 提交的关联对经 `merge` 落库，并自动补反向关联（去重幂等）。
async fn apply_relations(
    kb: &AsyncKb,
    pairs: &[RelationPair],
) -> Result<(), KnowledgeBuilderError> {
    // 按 from 分组，减少 rebuild 次数
    use std::collections::BTreeMap;
    let mut grouped: BTreeMap<String, Vec<(Predicate, WikiId)>> = BTreeMap::new();
    for pair in pairs {
        let predicate = match &pair.predicate {
            Some(p) => Predicate::new(p).map_err(KnowledgeBuilderError::from)?,
            None => Predicate::related(),
        };
        let from = WikiId::new(&pair.from)?;
        let to = WikiId::new(&pair.to)?;
        grouped
            .entry(pair.from.clone())
            .or_default()
            .push((predicate.clone(), to.clone()));
        grouped
            .entry(pair.to.clone())
            .or_default()
            .push((predicate, from.clone()));
    }
    for (from_raw, relations) in grouped {
        let id = WikiId::new(&from_raw)?;
        kb.merge(id, String::new(), relations)
            .await
            .map_err(KnowledgeBuilderError::from)?;
    }
    Ok(())
}

/// 检查取消令牌。
fn check_cancel(cancel: &CancellationToken) -> Result<(), KnowledgeBuilderError> {
    if cancel.is_cancelled() {
        Err(KnowledgeBuilderError::Cancelled)
    } else {
        Ok(())
    }
}

/// 构造进度 payload。
fn progress(
    ref_id: &str,
    stage: &str,
    created_count: usize,
    total_planned: Option<usize>,
) -> KbBuildProgressPayload {
    KbBuildProgressPayload {
        task_id: String::new(), // 由 runner 填充
        ref_id: ref_id.to_string(),
        stage: stage.to_string(),
        created_count,
        total_planned,
        detail: None,
    }
}

/// 截取文献原文前缀的预览片段（单行，去除换行），用于日志回显确认注入内容。
fn md_preview(md: &str, max_chars: usize) -> String {
    let mut s = md.chars().take(max_chars).collect::<String>();
    s = s.replace('\n', " ");
    if md.chars().count() > max_chars {
        s.push_str("…");
    }
    s
}

/// 读取文献 MD 内容。
fn read_md_content(project_path: &Path, ref_id: &str) -> Result<String, KnowledgeBuilderError> {
    let md_path = project_path
        .join("references")
        .join("md")
        .join(format!("{ref_id}.md"));
    tracing::debug!(ref_id = %ref_id, md_path = %md_path.display(), "读取文献 MD");
    std::fs::read_to_string(&md_path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            tracing::error!(ref_id = %ref_id, md_path = %md_path.display(), "文献 MD 不存在");
            KnowledgeBuilderError::MdNotFound(md_path)
        } else {
            tracing::error!(ref_id = %ref_id, error = %e, "读取文献 MD 失败");
            KnowledgeBuilderError::Io(e)
        }
    })
}

/// 读取 `references/wiki/index.md` 内容。
///
/// 文件不存在时返回空字符串（知识库未初始化或首次构建）。
fn read_index_md(refs_dir: &Path) -> String {
    let index_path = refs_dir.join("wiki").join("index.md");
    match std::fs::read_to_string(&index_path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::warn!(
                index_path = %index_path.display(),
                "index.md 不存在，使用空快照"
            );
            String::new()
        }
        Err(e) => {
            tracing::warn!(
                index_path = %index_path.display(),
                error = %e,
                "读取 index.md 失败，使用空快照"
            );
            String::new()
        }
    }
}

/// 清空 entry capture（每次 `engine.chat()` 前调用）。
fn clear_capture(capture: &CreateEntryCapture) {
    *capture.lock().expect("entry capture poisoned") = None;
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_md_content_missing_file_returns_md_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let err = read_md_content(tmp.path(), "ref-nonexistent").unwrap_err();
        assert!(matches!(err, KnowledgeBuilderError::MdNotFound(_)));
    }

    #[test]
    fn md_preview_truncates_and_condenses_newlines() {
        let md = "标题\n正文继续一段很长的内容用于测试预览截断";
        let p = md_preview(md, 6);
        assert!(!p.contains('\n'));
        assert!(p.contains('…'));

        let short = md_preview("abc", 10);
        assert_eq!(short, "abc");
        assert!(!short.contains('…'));
    }

    /// 机制测试：规划 prompt 必须完整内嵌注入的文献原文，且不得夹带任何硬编码外来主题
    /// （这里以「数智化」为反例）——验证"正文进 prompt"这一步符合预期。
    #[test]
    fn render_planning_embeds_full_literature_without_foreign_anchor() {
        let md = "本研究采用元分析方法，检验父母控制对中小学生心理健康的影响……";
        let opts = super::super::types::KnowledgeBuildOptions::default();
        let snapshot = super::super::index_snapshot::IndexSnapshot::default();
        let rendered = render_planning(md, &opts, &snapshot);

        // 1) 完整原文必须真实存在于渲染后的 prompt 中（不是占位、不是被截断）
        assert!(rendered.contains(md), "规划 prompt 必须包含完整文献正文");
        assert!(rendered.contains("父母控制"));
        // 2) 规划阶段的 prompt 不得携带任何外来「数智化」硬编码锚点
        assert!(
            !rendered.contains("数智化"),
            "规划 prompt 不应夹带外来主题锚点「数智化」"
        );
    }

    /// 机制测试（端到端、mock 模型、模拟数据）：验证"注入文献正文 → 模型确实收到该正文 →
    /// 模型返回的计划无损回流"的整条机制是否按预期运行。
    ///
    /// mock 的 ProberProvider 会在收到请求时断言用户消息里含注入的文献标记（"元分析"），
    /// 若正文被丢弃则断言失败；随后返回一个基于该文献的正常计划。若 pipeline 解析/回传
    /// 被污染，plan.concepts 与 mock 返回不一致也会失败。
    #[tokio::test]
    async fn planning_injects_literature_to_model_and_roundtrips_plan() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::{Arc, Mutex};

        use async_trait::async_trait;
        use futures::stream::BoxStream;
        use referee_ai::provider::{
            ChatRequest, ChatResponse, FinishReason, LlmError, LLMProvider, Message, MessageContent,
            ModelSpec, MultimodalCapabilities, ProviderCapabilities, ProviderId, Role, StreamChunk,
            ToolCall, ToolCallFunction,
        };
        use serde_json::json;

        use super::super::index_snapshot::IndexSnapshot;
        use super::super::llm_helper::{new_session_id, PlanCapture, SubmitPlanTool};
        use tokio_util::sync::CancellationToken;

        // 构造一份「真实规模」的长文献正文（约 1.8 万字符，与真实导入的期刊论文相当），
        // 末端带唯一标记，用于验证连长文尾部都能完整送达模型（排除"只发头部/被截断"）。
        let filler = "本研究作为教养方式长期追踪的一部分，从自我调节、内化问题与外化行为等维度考察父母控制对儿童发展的作用，并结合纵向数据进行历时比较。\n";
        let mut md = String::new();
        for _ in 0..280 {
            md.push_str(filler);
        }
        md.push_str("结论标记：父母控制与中小学生心理问题存在中等程度的正相关（末端）。");
        assert!(
            md.chars().count() > 18_000,
            "前置：文献正文应大于 1.8 万字符"
        );

        // 由假模型记录「它收到的 user 消息里是否含注入文献的末端标记」
        let received_marker = Arc::new(Mutex::new(false));
        let probe_received = received_marker.clone();

        struct ProbeProvider {
            received: Arc<Mutex<bool>>,
            calls: AtomicU32,
        }

        #[async_trait]
        impl LLMProvider for ProbeProvider {
            fn id(&self) -> ProviderId {
                ProviderId::new("probe")
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
                // 与生产一致的宽松上下文（默认 128K），确保真实文献（~1.8万字符）不被截断
                ModelSpec {
                    context_window_tokens: 128 * 1024,
                    max_output_tokens: 16 * 1024,
                }
            }
            async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, LlmError> {
                let mark = self.calls.fetch_add(1, Ordering::SeqCst);
                // 第 1 次调用：断言模型收到的 user 消息里确实含注入文献的『末端』标记
                if mark == 0 {
                    let got = req
                        .messages
                        .iter()
                        .filter(|m| matches!(m.role, Role::User))
                        .find_map(|m| m.content.as_text())
                        .map(|t| t.contains("中等程度的正相关（末端）"))
                        .unwrap_or(false);
                    *self.received.lock().unwrap() = got;
                }
                let text = |t: &str| MessageContent::text(t.to_string());
                if mark == 0 {
                    // 首轮：直接调用 submit_plan（携带基于该文献的正常计划）
                    let args = json!({
                        "summary_points": ["父母控制与中小学生心理问题存在中等程度正相关"],
                        "concepts": [{"title": "父母控制", "brief": "一种控制型教养行为"}],
                        "entities": []
                    })
                    .to_string();
                    Ok(ChatResponse {
                        id: "probe".into(),
                        model: "probe".into(),
                        message: Message {
                            role: Role::Assistant,
                            content: text("我来提交计划。"),
                            reasoning_content: None,
                            tool_calls: vec![ToolCall {
                                id: "call_probe_1".into(),
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
                    })
                } else {
                    // 收敛轮：不再调用工具
                    Ok(ChatResponse {
                        id: "probe".into(),
                        model: "probe".into(),
                        message: Message {
                            role: Role::Assistant,
                            content: text("计划已提交。"),
                            reasoning_content: None,
                            tool_calls: vec![],
                            tool_call_id: None,
                            usage: None,
                        },
                        finish_reason: FinishReason::Stop,
                        usage: None,
                    })
                }
            }
            async fn chat_stream(
                &self,
                _req: ChatRequest,
            ) -> Result<BoxStream<'static, Result<StreamChunk, LlmError>>, LlmError> {
                Err(LlmError::Protocol("on-probe-no-streaming".into()))
            }
        }

        let capture: PlanCapture = Arc::new(Mutex::new(None));
        // 知识库构建必须把完整文献注入规划阶段的 user 消息。referee 的会话默认
        // prompt 预算为 8000 token，会把长文献整条丢弃；此处按生产修复后的配置，
        // 抬高会话 prompt 预算到模型默认上下文（128K token），验证文献可达模型。
        let mut engine_config = referee_ai::engine::EngineConfig::default();
        engine_config.session.prompt_budget_tokens = 128 * 1024;
        let runtime = crate::agent_runtime::FluenRuntimeBuilder::new(Arc::new(ProbeProvider {
            received: probe_received,
            calls: AtomicU32::new(0),
        }))
        .with_config(engine_config)
        .with_tool(Arc::new(SubmitPlanTool::new(capture.clone())))
        .build();

        let session_id = new_session_id();
        let snapshot = IndexSnapshot::default();
        let options = super::super::types::KnowledgeBuildOptions::default();
        let usage_capture: super::super::llm_helper::UsageCapture = Arc::new(Mutex::new(None));
        let cancel = CancellationToken::new();

        let (plan, _usage) = run_planning(
            &runtime,
            &session_id,
            false,
            &md,
            &options,
            &snapshot,
            &capture,
            &usage_capture,
            &cancel,
        )
        .await
        .unwrap();

        // 1) 机制验证：注入的长文文献『末端』确实被模型收到（既未被丢弃，也未被截断）
        assert!(
            *received_marker.lock().unwrap(),
            "模型必须收到注入长文的『末端』标记"
        );
        // 2) 机制验证：模型返回的计划无损回流，未被流水线改写 / 污染
        assert_eq!(plan.concepts.len(), 1);
        assert_eq!(plan.concepts[0].title, "父母控制");
        assert_eq!(plan.summary_points.len(), 1);
        runtime.remove_session(session_id);
    }

    #[test]
    fn read_md_content_reads_existing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let md_dir = tmp.path().join("references").join("md");
        std::fs::create_dir_all(&md_dir).unwrap();
        std::fs::write(md_dir.join("ref-abc.md"), "# 文献标题\n正文内容").unwrap();

        let content = read_md_content(tmp.path(), "ref-abc").unwrap();
        assert!(content.contains("文献标题"));
    }

    #[test]
    fn check_cancel_returns_cancelled_when_cancelled() {
        let token = CancellationToken::new();
        assert!(check_cancel(&token).is_ok());

        token.cancel();
        let err = check_cancel(&token).unwrap_err();
        assert!(matches!(err, KnowledgeBuilderError::Cancelled));
    }

    #[test]
    fn progress_payload_fields() {
        let p = progress("ref-abc", "creating_concepts", 3, Some(10));
        assert_eq!(p.ref_id, "ref-abc");
        assert_eq!(p.stage, "creating_concepts");
        assert_eq!(p.created_count, 3);
        assert_eq!(p.total_planned, Some(10));
    }

    #[test]
    fn read_index_md_returns_empty_when_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let refs_dir = tmp.path().join("references");
        let content = read_index_md(&refs_dir);
        assert!(content.is_empty());
    }

    #[test]
    fn read_index_md_returns_content_when_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let wiki_dir = tmp.path().join("references").join("wiki");
        std::fs::create_dir_all(&wiki_dir).unwrap();
        std::fs::write(wiki_dir.join("index.md"), "# index\n- [[concepts/wiki-abc-测试]]").unwrap();

        let content = read_index_md(&tmp.path().join("references"));
        assert!(content.contains("测试"));
    }

    #[test]
    fn render_candidates_reused_from_prompts() {
        use super::super::prompts::render_candidates;
        // 验证 L2Candidate 渲染（来自 prompts 模块）
        let candidates = vec![L2Candidate {
            wiki_id: "wiki-abc".into(),
            title: "机器学习".into(),
            score: 0.85,
        }];
        let rendered = render_candidates(&candidates);
        assert!(rendered.contains("wiki-abc"));
        assert!(rendered.contains("机器学习"));
    }
}
