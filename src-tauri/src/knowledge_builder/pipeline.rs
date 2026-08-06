//! 两阶段流水线：文献 MD → AI 提取 → 知识库条目。
//!
//! ## 设计（V2.1）
//!
//! - **阶段 1（Planning）**：注入 Index 快照（全局去重视图），AI 阅读全文，产出 [`ExtractionPlan`]。
//! - **阶段 2（Execution）**：按计划逐条创建 summary / concept / entity，
//!   每条创建前做 L2 混合检索查重，每次 `runtime.run()` 后从真实 usage 更新会话预算。
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
//! - **真实 usage 跟踪**：每次 `runtime.run()` 后从 [`UsageCapture`] 读取 LLM 返回的
//!   `prompt_tokens + completion_tokens`，更新 [`KnowledgeBuildSession`] 的 `history_used`。
//! - **会话复用**：runtime 由调用方（runner）通过 [`KnowledgeBuildSession`] 传入，
//!   跨论文复用以命中模型前缀缓存。

use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::json;
use tokio_util::sync::CancellationToken;

use fluen_knowledge::async_kb::{AsyncKnowledgeBase, AsyncQueryParams};
use fluen_knowledge::types::{RetrievalMethod, WikiType};

use crate::llm_config::model::LlmConfig;

use super::error::KnowledgeBuilderError;
use super::events::KbBuildProgressPayload;
use super::index_snapshot::IndexSnapshot;
use super::llm_helper::{CreateEntryCapture, PlanCapture, UsageCapture, UsageSnapshot};
use super::prompts::{
    render_candidates, render_create_concept, render_create_entity, render_create_summary,
    render_establish_entry_relations, render_planning, render_summarize, L2Candidate, LONG_DOC_THRESHOLD,
};
use super::session::KnowledgeBuildSession;
use super::types::{
    BuildStage, ExtractionPlan, KnowledgeBuildCheckpoint, KnowledgeBuildOptions, PlannedEntry,
};

// ---------------------------------------------------------------------------
// 主入口
// ---------------------------------------------------------------------------

/// 执行知识库构建流水线。
///
/// 按 [`BuildStage`] 状态机推进，每个阶段完成后更新 `checkpoint`。
/// 调用方（runner）负责持久化 checkpoint 与会话生命周期管理。
///
/// # 参数
///
/// - `project_path`：项目根路径
/// - `ref_id`：文献 ID
/// - `options`：构建选项
/// - `checkpoint`：断点续传信息（可变引用，被原地更新）
/// - `llm_config`：LLM 配置（用于构建 L2 检索用的 KB）
/// - `session`：跨论文会话（提供 runtime 与 usage 跟踪）
/// - `plan_capture` / `entry_capture` / `usage_capture`：与 session runtime 绑定的共享捕获
/// - `cancel`：取消令牌
/// - `on_progress`：进度回调
pub async fn build(
    project_path: &Path,
    ref_id: &str,
    options: &KnowledgeBuildOptions,
    checkpoint: &mut KnowledgeBuildCheckpoint,
    llm_config: &LlmConfig,
    session: &mut KnowledgeBuildSession,
    plan_capture: &PlanCapture,
    entry_capture: &CreateEntryCapture,
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

    // 初始化知识库（用于 L2 检索，与 session runtime 内的 KB 共享同一 DB）
    let refs_dir = project_path.join("references");
    let kb = match crate::builtin_providers::embedding::build_embedding_router(llm_config) {
        Some(router) => {
            tracing::debug!(ref_id = %ref_id, "Embedding 已启用，注入路由器");
            AsyncKnowledgeBase::init(&refs_dir)?
                .with_embedding_provider(Arc::new(router))
        }
        None => {
            tracing::debug!(ref_id = %ref_id, "Embedding 已禁用，使用纯关键词检索");
            AsyncKnowledgeBase::init(&refs_dir)?
        }
    };

    // ── 实时读取 Index 快照（每次构建任务从磁盘解析，确保最新状态）──
    let index_md = read_index_md(&refs_dir);
    let snapshot = IndexSnapshot::parse(&index_md)?;
    tracing::info!(
        ref_id = %ref_id,
        total_entries = snapshot.total_entries(),
        tags = snapshot.tags.len(),
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
            entry_capture,
            usage_capture,
            ref_id,
            &plan.summary_points,
            &candidates,
            cancel,
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
                entry_capture,
                usage_capture,
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
                entry_capture,
                usage_capture,
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
            kb.edit_entry(fluen_knowledge::wiki::EditEntryParams {
                wiki_id: summary_id.clone(),
                edits: Vec::new(),
                add_relations: summary_related,
                add_tags: Vec::new(),
            })
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
            let usage = run_establish_entry_relations(
                session.runtime(),
                usage_capture,
                &entries_table,
                cancel,
            )
            .await?;
            session.update_usage(usage);
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

// ---------------------------------------------------------------------------
// 阶段实现
// ---------------------------------------------------------------------------

/// Planning 阶段：AI 阅读全文 + Index 快照，产出 ExtractionPlan。
///
/// 长文献（> 50000 字）先摘要预处理。
/// 返回 `(plan, usage)`，usage 用于更新会话的真实上下文占用。
async fn run_planning(
    runtime: &confluent::ConfluentRuntime,
    md_content: &str,
    options: &KnowledgeBuildOptions,
    snapshot: &IndexSnapshot,
    plan_capture: &PlanCapture,
    usage_capture: &UsageCapture,
    cancel: &CancellationToken,
) -> Result<(ExtractionPlan, UsageSnapshot), KnowledgeBuilderError> {
    let char_count = md_content.chars().count();
    tracing::info!(chars = char_count, threshold = LONG_DOC_THRESHOLD, "Planning 文献长度");

    // 长文献摘要预处理（usage 不外传，已被后续 Planning 调用的 usage 覆盖）
    let effective_content = if char_count > LONG_DOC_THRESHOLD {
        tracing::info!(chars = char_count, "长文献，启用摘要预处理");
        let (summary, _) =
            on_planning_summarize(runtime, md_content, usage_capture, cancel).await?;
        summary
    } else {
        md_content.to_string()
    };

    // 渲染 prompt（注入 Index 快照）
    let prompt = render_planning(&effective_content, options, snapshot);
    tracing::debug!(prompt_chars = prompt.chars().count(), "Planning prompt 渲染完成");

    // 清空 capture
    {
        let mut cap = plan_capture.lock().unwrap();
        *cap = None;
    }
    clear_usage(usage_capture);

    // 运行 AI
    let input = json!(prompt);
    let _result = runtime.run(input).await.map_err(|e| {
        if cancel.is_cancelled() {
            KnowledgeBuilderError::Cancelled
        } else {
            tracing::error!(error = %e, "Planning LLM 调用失败");
            KnowledgeBuilderError::Llm(e.to_string())
        }
    })?;

    // 从 capture 读取 plan
    let plan = {
        let mut cap = plan_capture.lock().unwrap();
        cap.take()
            .ok_or_else(|| KnowledgeBuilderError::AiOutput("AI 未调用 submit_plan 工具".into()))?
    };

    let usage = take_usage(usage_capture);

    tracing::debug!(
        summary_points = plan.summary_points.len(),
        concepts = plan.concepts.len(),
        entities = plan.entities.len(),
        prompt_tokens = usage.prompt_tokens,
        completion_tokens = usage.completion_tokens,
        "Planning 已捕获 ExtractionPlan"
    );
    Ok((plan, usage))
}

/// 长文献摘要预处理。
///
/// 返回 `(摘要文本, usage)`。usage 由调用方决定是否使用。
async fn on_planning_summarize(
    runtime: &confluent::ConfluentRuntime,
    md_content: &str,
    usage_capture: &UsageCapture,
    cancel: &CancellationToken,
) -> Result<(String, UsageSnapshot), KnowledgeBuilderError> {
    let prompt = render_summarize(md_content);
    let input = json!(prompt);

    clear_usage(usage_capture);
    let result = runtime.run(input).await.map_err(|e| {
        if cancel.is_cancelled() {
            KnowledgeBuilderError::Cancelled
        } else {
            KnowledgeBuilderError::Llm(e.to_string())
        }
    })?;
    let usage = take_usage(usage_capture);

    // AI 返回的摘要文本
    let summary = result
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| result.to_string());

    Ok((summary, usage))
}

/// 创建 summary 条目。
///
/// AI 调用 `knowledge_create_entry`（新建）或 `knowledge_edit_entry`
/// （合并到已有相似条目），[`super::llm_helper::CreateEntryObserver`]
/// 自动捕获返回的 `wiki_id`。返回 `(wiki_id, usage)`。
async fn run_create_summary(
    runtime: &confluent::ConfluentRuntime,
    entry_capture: &CreateEntryCapture,
    usage_capture: &UsageCapture,
    ref_id: &str,
    summary_points: &[String],
    candidates: &[L2Candidate],
    cancel: &CancellationToken,
) -> Result<(String, UsageSnapshot), KnowledgeBuilderError> {
    let prompt = render_create_summary(ref_id, summary_points, candidates);
    let input = json!(prompt);

    clear_capture(entry_capture);
    clear_usage(usage_capture);

    runtime.run(input).await.map_err(|e| {
        if cancel.is_cancelled() {
            KnowledgeBuilderError::Cancelled
        } else {
            tracing::error!(ref_id = %ref_id, error = %e, "create_summary LLM 调用失败");
            KnowledgeBuilderError::Llm(e.to_string())
        }
    })?;

    let wiki_id = take_capture(entry_capture, "summary")?;
    let usage = take_usage(usage_capture);
    Ok((wiki_id, usage))
}

/// 创建单个 concept 条目。
///
/// AI 调用 `knowledge_create_entry`（新建）或 `knowledge_edit_entry`
/// （合并到已有相似条目），[`super::llm_helper::CreateEntryObserver`]
/// 自动捕获返回的 `wiki_id`。返回 `(wiki_id, usage)`。
async fn run_create_concept(
    runtime: &confluent::ConfluentRuntime,
    entry_capture: &CreateEntryCapture,
    usage_capture: &UsageCapture,
    planned: &PlannedEntry,
    candidates: &[L2Candidate],
    cancel: &CancellationToken,
) -> Result<(String, UsageSnapshot), KnowledgeBuilderError> {
    let prompt = render_create_concept(&planned.title, &planned.brief, candidates);
    let input = json!(prompt);

    clear_capture(entry_capture);
    clear_usage(usage_capture);

    runtime.run(input).await.map_err(|e| {
        if cancel.is_cancelled() {
            KnowledgeBuilderError::Cancelled
        } else {
            tracing::error!(title = %planned.title, error = %e, "create_concept LLM 调用失败");
            KnowledgeBuilderError::Llm(e.to_string())
        }
    })?;

    let wiki_id = take_capture(entry_capture, &format!("concept '{}'", planned.title))?;
    let usage = take_usage(usage_capture);
    Ok((wiki_id, usage))
}

/// 创建单个 entity 条目。
///
/// AI 调用 `knowledge_create_entry`（新建）或 `knowledge_edit_entry`
/// （合并到已有相似条目），[`super::llm_helper::CreateEntryObserver`]
/// 自动捕获返回的 `wiki_id`。返回 `(wiki_id, usage)`。
async fn run_create_entity(
    runtime: &confluent::ConfluentRuntime,
    entry_capture: &CreateEntryCapture,
    usage_capture: &UsageCapture,
    planned: &PlannedEntry,
    candidates: &[L2Candidate],
    cancel: &CancellationToken,
) -> Result<(String, UsageSnapshot), KnowledgeBuilderError> {
    let prompt = render_create_entity(&planned.title, &planned.brief, candidates);
    let input = json!(prompt);

    clear_capture(entry_capture);
    clear_usage(usage_capture);

    runtime.run(input).await.map_err(|e| {
        if cancel.is_cancelled() {
            KnowledgeBuilderError::Cancelled
        } else {
            tracing::error!(title = %planned.title, error = %e, "create_entity LLM 调用失败");
            KnowledgeBuilderError::Llm(e.to_string())
        }
    })?;

    let wiki_id = take_capture(entry_capture, &format!("entity '{}'", planned.title))?;
    let usage = take_usage(usage_capture);
    Ok((wiki_id, usage))
}

/// AI 驱动的 concept/entity 关联建立。
///
/// 传入「wikiID → 类型 → 标题」对照表，AI 基于标题判断语义关联，
/// 通过 `edit_entry` 工具用 wikiID 写入，确保链接格式合规。
/// 返回 usage。
async fn run_establish_entry_relations(
    runtime: &confluent::ConfluentRuntime,
    usage_capture: &UsageCapture,
    entries: &[(String, &str, String)],
    cancel: &CancellationToken,
) -> Result<UsageSnapshot, KnowledgeBuilderError> {
    let prompt = render_establish_entry_relations(entries);
    let input = json!(prompt);

    clear_usage(usage_capture);

    runtime.run(input).await.map_err(|e| {
        if cancel.is_cancelled() {
            KnowledgeBuilderError::Cancelled
        } else {
            tracing::error!(error = %e, "establish_entry_relations LLM 调用失败");
            KnowledgeBuilderError::Llm(e.to_string())
        }
    })?;

    Ok(take_usage(usage_capture))
}

// ---------------------------------------------------------------------------
// L2 混合检索
// ---------------------------------------------------------------------------

/// L2 混合检索：每条创建前查询已有条目（Top 3），供 AI 决策"更新 vs 新建"。
///
/// 默认使用 hybrid 检索（向量 + 关键词）；Embedding 未配置时自动降级为关键词检索。
/// 返回紧凑的 [`L2Candidate`] 列表（含 wiki_id 供 AI 合并调用）。
async fn l2_query(
    kb: &AsyncKnowledgeBase,
    query: &str,
    wiki_type: Option<WikiType>,
) -> Result<Vec<L2Candidate>, KnowledgeBuilderError> {
    let params = AsyncQueryParams {
        query: query.to_string(),
        wiki_type,
        method: RetrievalMethod::Hybrid,
        top_k: 3,
        include_content: false,
    };
    let result = kb.query(params).await.map_err(KnowledgeBuilderError::from)?;
    let candidates: Vec<L2Candidate> = result
        .results
        .into_iter()
        .map(|m| L2Candidate {
            wiki_id: m.wiki_id,
            title: m.title,
            score: m.score,
        })
        .collect();
    tracing::debug!(query = %query, count = candidates.len(), "L2 检索完成");
    Ok(candidates)
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

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

/// 清空 entry capture（每次 `runtime.run()` 前调用）。
fn clear_capture(capture: &CreateEntryCapture) {
    *capture.lock().expect("entry capture poisoned") = None;
}

/// 从 entry capture 中取出 wiki_id（每次 `runtime.run()` 后调用）。
///
/// 若 capture 为空，说明 AI 未调用 `create_entry`，返回错误。
fn take_capture(capture: &CreateEntryCapture, context: &str) -> Result<String, KnowledgeBuilderError> {
    let wiki_id = capture
        .lock()
        .expect("entry capture poisoned")
        .take()
        .ok_or_else(|| {
            tracing::error!(context = %context, "AI 未调用条目操作工具（create_entry/edit_entry，capture 为空）");
            KnowledgeBuilderError::AiOutput(format!(
                "AI 未调用 create_entry / edit_entry 创建 {context}"
            ))
        })?;
    tracing::debug!(context = %context, wiki_id = %wiki_id, "从 capture 获取 wiki_id");
    Ok(wiki_id)
}

/// 清空 usage capture（每次 `runtime.run()` 前调用）。
fn clear_usage(capture: &UsageCapture) {
    *capture.lock().expect("usage capture poisoned") = None;
}

/// 从 usage capture 中取出 usage 快照（每次 `runtime.run()` 后调用）。
///
/// 若 capture 为空（如 LLM 未返回 usage 字段），返回零值。
fn take_usage(capture: &UsageCapture) -> UsageSnapshot {
    capture
        .lock()
        .expect("usage capture poisoned")
        .take()
        .unwrap_or_default()
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
    fn take_usage_returns_default_when_empty() {
        let capture: UsageCapture = Arc::new(Mutex::new(None));
        let usage = take_usage(&capture);
        assert_eq!(usage.total(), 0);
    }

    #[test]
    fn take_usage_returns_value_when_set() {
        let capture: UsageCapture = Arc::new(Mutex::new(Some(UsageSnapshot {
            prompt_tokens: 1000,
            completion_tokens: 500,
        })));
        let usage = take_usage(&capture);
        assert_eq!(usage.total(), 1500);
        // take 后应清空
        assert!(capture.lock().unwrap().is_none());
    }

    #[test]
    fn clear_usage_sets_to_none() {
        let capture: UsageCapture = Arc::new(Mutex::new(Some(UsageSnapshot {
            prompt_tokens: 100,
            completion_tokens: 50,
        })));
        clear_usage(&capture);
        assert!(capture.lock().unwrap().is_none());
    }

    #[test]
    fn render_candidates_reused_from_prompts() {
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
