//! 两阶段流水线：文献 MD → AI 提取 → 知识库条目。
//!
//! ## 设计
//!
//! - **阶段 1（Planning）**：AI 阅读全文，`query` 去重，产出 [`ExtractionPlan`]。
//! - **阶段 2（Execution）**：按计划逐条创建 summary / concept / entity，
//!   每条独立 LLM 调用，可中断恢复。最后单独执行 relations 阶段。
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
//! ## 条目 ID 捕获
//!
//! AI 调用 `create_entry` 后，工具返回值中包含 `wiki_id`。
//! [`CreateEntryObserver`](super::llm_helper::CreateEntryObserver) 观察者
//! 自动捕获该 ID 写入共享状态，pipeline 在 `runtime.run()` 返回后直接读取，
//! 无需通过 title 反查（避免了 AI 标点漂移导致反查失败的问题）。

use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::json;
use tokio_util::sync::CancellationToken;

use fluen_knowledge::async_kb::AsyncKnowledgeBase;

use crate::llm_config::model::{LlmConfig, SceneModelRef};

use super::error::KnowledgeBuilderError;
use super::events::KbBuildProgressPayload;
use super::llm_helper::{build_kb_runtime, CreateEntryCapture, PlanCapture};
use super::prompts::{
    render_create_concept, render_create_entity, render_create_summary, render_establish_relations,
    render_planning, render_summarize, LONG_DOC_THRESHOLD,
};
use super::types::{
    BuildStage, ExtractionPlan, KnowledgeBuildCheckpoint, KnowledgeBuildOptions, PlannedEntry,
};

// ---------------------------------------------------------------------------
// 主入口
// ---------------------------------------------------------------------------

/// 执行知识库构建流水线。
///
/// 按 [`BuildStage`] 状态机推进，每个阶段完成后更新 `checkpoint`。
/// 调用方（runner）负责持久化 checkpoint。
///
/// # 参数
///
/// - `project_path`：项目根路径
/// - `ref_id`：文献 ID
/// - `model_ref`：场景化模型引用（任务级锁定）
/// - `options`：构建选项
/// - `checkpoint`：断点续传信息（可变引用，被原地更新）
/// - `llm_config`：LLM 配置
/// - `cancel`：取消令牌
/// - `on_progress`：进度回调
pub async fn build(
    project_path: &Path,
    ref_id: &str,
    model_ref: &SceneModelRef,
    options: &KnowledgeBuildOptions,
    checkpoint: &mut KnowledgeBuildCheckpoint,
    llm_config: &LlmConfig,
    cancel: &CancellationToken,
    on_progress: impl Fn(KbBuildProgressPayload),
) -> Result<(), KnowledgeBuilderError> {
    tracing::info!(
        ref_id = %ref_id,
        stage = ?checkpoint.stage,
        provider_id = %model_ref.provider_id,
        model_id = %model_ref.model_id,
        options = ?options,
        "知识库构建启动"
    );

    // 初始化知识库（根据配置注入 Embedding 路由器）
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

    // 构建运行时（PlanCapture 捕获计划，EntryCapture 捕获 wiki_id）
    let plan_capture: PlanCapture = Arc::new(Mutex::new(None));
    let entry_capture: CreateEntryCapture = Arc::new(Mutex::new(None));
    let runtime = build_kb_runtime(
        llm_config,
        model_ref,
        kb.clone(),
        plan_capture.clone(),
        entry_capture.clone(),
    )
    .await?;

    // ── 阶段 1：Planning ──────────────────────────────────────────
    if checkpoint.stage == BuildStage::Planning {
        check_cancel(cancel)?;
        on_progress(progress(ref_id, "planning", 0, None));
        tracing::info!(ref_id = %ref_id, "阶段 Planning 开始");

        let md_content = read_md_content(project_path, ref_id)?;
        let plan = run_planning(&runtime, &kb, &md_content, options, &plan_capture, cancel).await?;

        tracing::info!(
            ref_id = %ref_id,
            summary_points = plan.summary_points.len(),
            concepts = plan.concepts.len(),
            entities = plan.entities.len(),
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

    // ── 阶段 2a：CreatingSummary ─────────────────────────────────
    if checkpoint.stage == BuildStage::CreatingSummary && options.create_summary {
        check_cancel(cancel)?;
        on_progress(progress(ref_id, "creating_summary", 0, None));
        tracing::info!(ref_id = %ref_id, points = plan.summary_points.len(), "阶段 CreatingSummary 开始");

        let summary_id =
            run_create_summary(&runtime, &entry_capture, ref_id, &plan.summary_points, cancel).await?;

        tracing::info!(ref_id = %ref_id, summary_id = %summary_id, "阶段 CreatingSummary 完成");
        checkpoint.summary_id = Some(summary_id);
        checkpoint.stage = BuildStage::CreatingConcepts;
    }

    // ── 阶段 2b：CreatingConcepts（逐条）─────────────────────────
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

            let id = run_create_concept(&runtime, &entry_capture, planned, cancel).await?;
            tracing::debug!(ref_id = %ref_id, idx = i, wiki_id = %id, "concept 创建成功");
            checkpoint.concept_ids.push(id);
        }
        tracing::info!(ref_id = %ref_id, created = checkpoint.concept_ids.len(), "阶段 CreatingConcepts 完成");
        checkpoint.stage = BuildStage::CreatingEntities;
    }

    // ── 阶段 2c：CreatingEntities（逐条）─────────────────────────
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

            let id = run_create_entity(&runtime, &entry_capture, planned, cancel).await?;
            tracing::debug!(ref_id = %ref_id, idx = i, wiki_id = %id, "entity 创建成功");
            checkpoint.entity_ids.push(id);
        }
        tracing::info!(ref_id = %ref_id, created = checkpoint.entity_ids.len(), "阶段 CreatingEntities 完成");
        checkpoint.stage = BuildStage::EstablishingRelations;
    }

    // ── 阶段 2d：EstablishingRelations（独立阶段）────────────────
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

        // 收集所有关联 ID：plan 中的 existing_id + 本任务新建的
        let mut related_ids: Vec<String> = Vec::new();
        for c in &plan.concepts {
            if let Some(id) = &c.existing_id {
                related_ids.push(id.clone());
            }
        }
        for e in &plan.entities {
            if let Some(id) = &e.existing_id {
                related_ids.push(id.clone());
            }
        }
        related_ids.extend(checkpoint.concept_ids.iter().cloned());
        related_ids.extend(checkpoint.entity_ids.iter().cloned());
        // 去重
        related_ids.sort();
        related_ids.dedup();

        tracing::info!(
            ref_id = %ref_id,
            summary_id = %summary_id,
            related_count = related_ids.len(),
            "阶段 EstablishingRelations 开始"
        );

        if !related_ids.is_empty() {
            run_establish_relations(&runtime, summary_id, &related_ids, cancel).await?;
        }
        tracing::info!(ref_id = %ref_id, "阶段 EstablishingRelations 完成");

        checkpoint.relations_established = true;
        checkpoint.stage = BuildStage::Done;
    }

    // 若跳过了某些阶段（options 关闭），直接置 Done
    if checkpoint.stage != BuildStage::Done {
        tracing::debug!(ref_id = %ref_id, stage = ?checkpoint.stage, "未执行的阶段被跳过，直接置 Done");
        checkpoint.stage = BuildStage::Done;
    }

    tracing::info!(
        ref_id = %ref_id,
        summary_id = ?checkpoint.summary_id,
        concepts = checkpoint.concept_ids.len(),
        entities = checkpoint.entity_ids.len(),
        relations = checkpoint.relations_established,
        "知识库构建完成"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 阶段实现
// ---------------------------------------------------------------------------

/// Planning 阶段：AI 阅读全文，query 去重，产出 ExtractionPlan。
///
/// 长文献（> 50000 字）先摘要预处理。
async fn run_planning(
    runtime: &confluent::ConfluentRuntime,
    _kb: &AsyncKnowledgeBase,
    md_content: &str,
    options: &KnowledgeBuildOptions,
    plan_capture: &PlanCapture,
    cancel: &CancellationToken,
) -> Result<ExtractionPlan, KnowledgeBuilderError> {
    let char_count = md_content.chars().count();
    tracing::info!(chars = char_count, threshold = LONG_DOC_THRESHOLD, "Planning 文献长度");

    // 长文献摘要预处理
    let effective_content = if char_count > LONG_DOC_THRESHOLD {
        tracing::info!(chars = char_count, "长文献，启用摘要预处理");
        on_planning_summarize(runtime, md_content, cancel).await?
    } else {
        md_content.to_string()
    };

    // 渲染 prompt
    let prompt = render_planning(&effective_content, options);
    tracing::debug!(prompt_chars = prompt.chars().count(), "Planning prompt 渲染完成");

    // 清空 capture
    {
        let mut cap = plan_capture.lock().unwrap();
        *cap = None;
    }

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

    tracing::debug!(
        summary_points = plan.summary_points.len(),
        concepts = plan.concepts.len(),
        entities = plan.entities.len(),
        "Planning 已捕获 ExtractionPlan"
    );
    Ok(plan)
}

/// 长文献摘要预处理。
async fn on_planning_summarize(
    runtime: &confluent::ConfluentRuntime,
    md_content: &str,
    cancel: &CancellationToken,
) -> Result<String, KnowledgeBuilderError> {
    let prompt = render_summarize(md_content);
    let input = json!(prompt);
    let result = runtime.run(input).await.map_err(|e| {
        if cancel.is_cancelled() {
            KnowledgeBuilderError::Cancelled
        } else {
            KnowledgeBuilderError::Llm(e.to_string())
        }
    })?;

    // AI 返回的摘要文本
    let summary = result
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| result.to_string());

    Ok(summary)
}

/// 创建 summary 条目。
///
/// AI 调用 `knowledge_create_entry`，[`CreateEntryObserver`] 自动捕获返回的 `wiki_id`。
/// pipeline 在 `runtime.run()` 返回后从 capture 读取，无需反查。
async fn run_create_summary(
    runtime: &confluent::ConfluentRuntime,
    entry_capture: &CreateEntryCapture,
    ref_id: &str,
    summary_points: &[String],
    cancel: &CancellationToken,
) -> Result<String, KnowledgeBuilderError> {
    let prompt = render_create_summary(ref_id, summary_points);
    let input = json!(prompt);

    clear_capture(entry_capture);

    runtime.run(input).await.map_err(|e| {
        if cancel.is_cancelled() {
            KnowledgeBuilderError::Cancelled
        } else {
            tracing::error!(ref_id = %ref_id, error = %e, "create_summary LLM 调用失败");
            KnowledgeBuilderError::Llm(e.to_string())
        }
    })?;

    take_capture(entry_capture, "summary")
}

/// 创建单个 concept 条目。
///
/// AI 调用 `knowledge_create_entry`，[`CreateEntryObserver`] 自动捕获返回的 `wiki_id`。
async fn run_create_concept(
    runtime: &confluent::ConfluentRuntime,
    entry_capture: &CreateEntryCapture,
    planned: &PlannedEntry,
    cancel: &CancellationToken,
) -> Result<String, KnowledgeBuilderError> {
    let prompt = render_create_concept(&planned.title, &planned.brief);
    let input = json!(prompt);

    clear_capture(entry_capture);

    runtime.run(input).await.map_err(|e| {
        if cancel.is_cancelled() {
            KnowledgeBuilderError::Cancelled
        } else {
            tracing::error!(title = %planned.title, error = %e, "create_concept LLM 调用失败");
            KnowledgeBuilderError::Llm(e.to_string())
        }
    })?;

    take_capture(entry_capture, &format!("concept '{}'", planned.title))
}

/// 创建单个 entity 条目。
///
/// AI 调用 `knowledge_create_entry`，[`CreateEntryObserver`] 自动捕获返回的 `wiki_id`。
async fn run_create_entity(
    runtime: &confluent::ConfluentRuntime,
    entry_capture: &CreateEntryCapture,
    planned: &PlannedEntry,
    cancel: &CancellationToken,
) -> Result<String, KnowledgeBuilderError> {
    let prompt = render_create_entity(&planned.title, &planned.brief);
    let input = json!(prompt);

    clear_capture(entry_capture);

    runtime.run(input).await.map_err(|e| {
        if cancel.is_cancelled() {
            KnowledgeBuilderError::Cancelled
        } else {
            tracing::error!(title = %planned.title, error = %e, "create_entity LLM 调用失败");
            KnowledgeBuilderError::Llm(e.to_string())
        }
    })?;

    take_capture(entry_capture, &format!("entity '{}'", planned.title))
}

/// 建立 relations：AI 调用 `edit_entry` 对 summary 添加 relations。
async fn run_establish_relations(
    runtime: &confluent::ConfluentRuntime,
    summary_id: &str,
    related_ids: &[String],
    cancel: &CancellationToken,
) -> Result<(), KnowledgeBuilderError> {
    let prompt = render_establish_relations(summary_id, related_ids);
    let input = json!(prompt);

    runtime.run(input).await.map_err(|e| {
        if cancel.is_cancelled() {
            KnowledgeBuilderError::Cancelled
        } else {
            tracing::error!(summary_id = %summary_id, error = %e, "establish_relations LLM 调用失败");
            KnowledgeBuilderError::Llm(e.to_string())
        }
    })?;

    Ok(())
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

/// 清空 capture（每次 `runtime.run()` 前调用）。
fn clear_capture(capture: &CreateEntryCapture) {
    *capture.lock().expect("entry capture poisoned") = None;
}

/// 从 capture 中取出 wiki_id（每次 `runtime.run()` 后调用）。
///
/// 若 capture 为空，说明 AI 未调用 `create_entry`，返回错误。
fn take_capture(capture: &CreateEntryCapture, context: &str) -> Result<String, KnowledgeBuilderError> {
    let wiki_id = capture
        .lock()
        .expect("entry capture poisoned")
        .take()
        .ok_or_else(|| {
            tracing::error!(context = %context, "AI 未调用 create_entry（capture 为空）");
            KnowledgeBuilderError::AiOutput(format!("AI 未调用 create_entry 创建 {context}"))
        })?;
    tracing::debug!(context = %context, wiki_id = %wiki_id, "从 capture 获取 wiki_id");
    Ok(wiki_id)
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
}
