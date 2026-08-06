//! 项目级单种类串行任务执行器。
//!
//! 每个 [`TaskRunner`] 绑定一个项目与一个任务种类，串行执行该项目下
//! 该种类的 Pending 任务。同一项目不同种类各有独立 runner，并行执行
//! 互不阻塞（如知识库构建与文献导入可同时进行）。
//! 通过 [`TaskQueueState`] 管理各项目各 runner 的生命周期。
//!
//! ## 设计（V2.2）
//!
//! - **单 worker 串行**：同一项目同一种类内任务严格按 FIFO 顺序执行；
//!   不同种类并行（各自的 runner 独立消费）。
//! - **取消令牌**：每个任务关联一个 [`CancellationToken`]，支持运行中取消。
//! - **事件推送**：通过 Tauri `Window::emit` 推送任务进度与终态事件到前端。
//! - **checkpoint 持久化**：执行中持续更新 checkpoint，中断后可从断点恢复。
//! - **会话池集成**：通过 [`SessionPool`] 跨论文复用 runtime（命中模型前缀缓存）。
//! - **分级错误处理**：瞬态错误保留会话 + 指数退避重试；结构性错误销毁会话。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;

use crate::ai_services::storage::ConfigStorage as AiServicesConfigStorage;
use crate::knowledge_builder::context_budget::ContextBudget;
use crate::knowledge_builder::error_classify::{
    backoff_delay, classify_error, LlmErrorKind, MAX_TRANSIENT_RETRIES,
};
use crate::knowledge_builder::events::{
    KbBuildCancelledPayload, KbBuildCompletedPayload, KbBuildFailedPayload, KbBuildStartedPayload,
    EVENT_KB_BUILD_CANCELLED, EVENT_KB_BUILD_COMPLETED, EVENT_KB_BUILD_FAILED,
    EVENT_KB_BUILD_STARTED,
};
use crate::knowledge_builder::session::SessionPool;
use crate::knowledge_builder::types::KnowledgeBuildCheckpoint;
use crate::knowledge_builder::KnowledgeBuilderError as KbError;
use crate::llm_config::model::SceneModelRef;
use crate::llm_config::storage::ConfigStorage as LlmConfigStorage;
use crate::references::error::ReferenceError;
use crate::references::events::{
    ImportCompletedPayload, ImportFailedPayload, ImportProgressPayload, ImportStartedPayload,
    EVENT_IMPORT_COMPLETED, EVENT_IMPORT_FAILED, EVENT_IMPORT_PROGRESS, EVENT_IMPORT_STARTED,
};
use crate::references::importer::{ImportProgress, ReferenceImporter};
use crate::references::storage::ReferenceIndex;

use fluen_knowledge::async_kb::AsyncKnowledgeBase;

use super::error::TaskQueueError;
use super::store::TaskStore;
use super::types::{TaskKind, TaskRecord, TaskStatus};

/// 项目级串行任务执行器。
///
/// 持有项目路径、任务存储、LLM 配置、会话池与 Tauri 句柄。
/// 通过 [`TaskRunner::run_once`] 执行下一个 Pending 任务，
/// 通过 [`TaskRunner::run_loop`] 持续消费直至无 Pending 任务。
pub struct TaskRunner {
    /// 项目根路径。
    project_path: PathBuf,
    /// 本 runner 消费的任务种类（`TaskKind::kind_name`，如 `knowledge_build`）。
    ///
    /// 同项目不同种类的任务由各自 runner 并行消费（互不阻塞）。
    kind: &'static str,
    /// 任务存储（同项目共享）。
    store: Arc<TaskStore>,
    /// LLM 配置存储（用于解析场景化模型）。
    llm_storage: Arc<LlmConfigStorage>,
    /// AI 服务配置存储（用于创建 OCR provider）。
    ai_storage: Arc<AiServicesConfigStorage>,
    /// Tauri 应用句柄（用于 emit 事件与获取窗口）。
    app: AppHandle,
    /// 取消令牌集合（task_id → token）。
    cancel_tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
    /// 项目级会话池（跨论文 runtime 复用）。
    session_pool: Arc<SessionPool>,
}

impl TaskRunner {
    /// 创建 runner 实例。
    pub fn new(
        project_path: impl Into<PathBuf>,
        kind: &'static str,
        store: Arc<TaskStore>,
        llm_storage: Arc<LlmConfigStorage>,
        ai_storage: Arc<AiServicesConfigStorage>,
        app: AppHandle,
        cancel_tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
        session_pool: Arc<SessionPool>,
    ) -> Self {
        Self {
            project_path: project_path.into(),
            kind,
            store,
            llm_storage,
            ai_storage,
            app,
            cancel_tokens,
            session_pool,
        }
    }

    /// 持续消费本种类 Pending 任务，直到无任务可执行。
    ///
    /// 单 worker 串行：每次循环拾取最早创建的本种类 Pending 任务，
    /// 执行完毕后再取下一个。不同种类的 runner 并行执行互不阻塞。
    ///
    /// 以 `&self` 借用，调用方可多次调用（如退出前复查队列兜底）。
    pub async fn run_loop(&self) {
        loop {
            match self.store.next_pending_of(self.kind) {
                Ok(Some(task)) => {
                    if let Err(e) = self.execute_one(&task).await {
                        tracing::error!(
                            task_id = %task.id,
                            error = %e,
                            "任务执行失败"
                        );
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    tracing::error!(error = %e, "读取 Pending 任务失败，退出 runner 循环");
                    break;
                }
            }
        }
    }

    /// 队列中是否仍有本种类 Pending 任务（runner 退出前复查用）。
    pub fn has_pending(&self) -> bool {
        self.store.has_pending_of(self.kind)
    }

    /// 执行单个任务（含状态转换、事件推送、checkpoint 持久化）。
    async fn execute_one(&self, task: &TaskRecord) -> Result<(), TaskQueueError> {
        tracing::info!(task_id = %task.id, kind = ?task.kind, "任务开始执行");
        // 标记为 Running
        self.store.update_status(&task.id, TaskStatus::Running)?;

        // 注册取消令牌
        let cancel_token = CancellationToken::new();
        {
            let mut tokens = self.cancel_tokens.lock().unwrap();
            tokens.insert(task.id.clone(), cancel_token.clone());
        }

        // 推送 started 事件
        self.emit_started(task);

        // 执行任务
        let result = self.dispatch(task, &cancel_token).await;

        // 无论结果如何，先移除取消令牌
        {
            let mut tokens = self.cancel_tokens.lock().unwrap();
            tokens.remove(&task.id);
        }

        // 根据结果更新终态并推送事件
        match result {
            Ok(()) => {
                tracing::info!(task_id = %task.id, "任务执行成功");
                self.store.update_status(&task.id, TaskStatus::Completed)?;
                self.emit_completed(task);
            }
            Err(TaskQueueError::Cancelled) => {
                tracing::info!(task_id = %task.id, "任务被取消");
                self.store
                    .update_status(&task.id, TaskStatus::Cancelled)?;
                self.emit_cancelled(task);
            }
            Err(ref e) => {
                tracing::error!(task_id = %task.id, error = %e, "任务执行失败");
                let err_msg = e.to_string();
                self.store
                    .update_status_with_error(&task.id, TaskStatus::Failed, &err_msg)?;
                self.emit_failed(task, &err_msg);
            }
        }

        Ok(())
    }

    /// 按任务种类分发到对应的执行器。
    async fn dispatch(
        &self,
        task: &TaskRecord,
        cancel: &CancellationToken,
    ) -> Result<(), TaskQueueError> {
        match &task.kind {
            TaskKind::KnowledgeBuild {
                ref_id,
                model_ref,
                options,
            } => self
                .execute_knowledge_build(task, ref_id, model_ref, options, cancel)
                .await,
            TaskKind::ReferenceImport {
                file_path,
                reference_id,
                force,
            } => self
                .execute_reference_import(task, file_path, reference_id, *force, cancel)
                .await,
        }
    }

    /// 执行文献导入任务（OCR 转 Markdown）。
    ///
    /// 复用 [`ReferenceImporter`] 的完整导入管线（preflight → 去重 →
    /// 备份 → OCR → 保存），进度通过 `reference:import_progress` 事件推送。
    /// 重跑幂等：去重检查跳过自身条目（中断恢复 / 重试场景）。
    async fn execute_reference_import(
        &self,
        task: &TaskRecord,
        file_path: &str,
        reference_id: &str,
        force: bool,
        cancel: &CancellationToken,
    ) -> Result<(), TaskQueueError> {
        tracing::info!(
            task_id = %task.id,
            reference_id = %reference_id,
            file = %file_path,
            force,
            "文献导入任务分发"
        );

        let ai_config = self
            .ai_storage
            .get()
            .map_err(|e| TaskQueueError::Execution(format!("读取 OCR 配置失败: {e}")))?;
        let importer = ReferenceImporter::new(&ai_config)
            .map_err(|e| TaskQueueError::Execution(e.to_string()))?;

        let index = ReferenceIndex::new(&self.project_path);
        let window = self.app.get_webview_window("main");

        let task_id = task.id.clone();
        let reference_id_owned = reference_id.to_string();
        let result = importer
            .import_single(
                file_path,
                &self.project_path,
                &index,
                reference_id,
                force,
                move |progress: ImportProgress| {
                    if let Some(ref window) = window {
                        let _ = window.emit(
                            EVENT_IMPORT_PROGRESS,
                            &ImportProgressPayload {
                                job_id: task_id.clone(),
                                reference_id: reference_id_owned.clone(),
                                stage: progress.stage.clone(),
                                ocr_progress: progress.ocr_progress,
                            },
                        );
                    }
                },
                cancel,
            )
            .await;

        match result {
            Ok(entry) => {
                tracing::info!(
                    task_id = %task.id,
                    reference_id = %entry.id,
                    title = %entry.title,
                    "文献导入任务执行成功"
                );
                // 直接携带 entry 推送完成事件（避免 emit_completed 二次读索引）
                if let Some(window) = self.app.get_webview_window("main") {
                    let _ = window.emit(
                        EVENT_IMPORT_COMPLETED,
                        &ImportCompletedPayload {
                            job_id: task.id.clone(),
                            reference_id: entry.id.clone(),
                            entry,
                        },
                    );
                }
                Ok(())
            }
            Err(ReferenceError::Cancelled) => {
                tracing::warn!(task_id = %task.id, reference_id = %reference_id, "文献导入任务被取消");
                Err(TaskQueueError::Cancelled)
            }
            Err(e) => {
                let err_msg = e.to_string();
                tracing::error!(
                    task_id = %task.id,
                    reference_id = %reference_id,
                    error = %err_msg,
                    "文献导入任务执行失败"
                );
                Err(TaskQueueError::Execution(err_msg))
            }
        }
    }

    /// 执行知识库构建任务。
    ///
    /// V2.1 流程：
    /// 1. 加载 LLM 配置与 checkpoint
    /// 2. 构建 KB（用于 L2 检索）与 ContextBudget
    /// 3. 从 SessionPool 获取或创建会话（跨论文复用）
    /// 4. 执行 pipeline（带瞬态错误重试）
    /// 5. 持久化 checkpoint
    /// 6. 分级错误处理：瞬态保留会话，结构性销毁会话
    async fn execute_knowledge_build(
        &self,
        task: &TaskRecord,
        ref_id: &str,
        model_ref: &SceneModelRef,
        options: &crate::knowledge_builder::types::KnowledgeBuildOptions,
        cancel: &CancellationToken,
    ) -> Result<(), TaskQueueError> {
        tracing::info!(
            task_id = %task.id,
            ref_id = %ref_id,
            provider_id = %model_ref.provider_id,
            model_id = %model_ref.model_id,
            "知识库构建任务分发"
        );

        // 加载 LLM 配置
        let llm_config = self
            .llm_storage
            .load()
            .map_err(|e| TaskQueueError::Execution(format!("加载 LLM 配置失败: {e}")))?;

        // 反序列化 checkpoint
        let mut checkpoint: KnowledgeBuildCheckpoint = if task.checkpoint.is_null() {
            KnowledgeBuildCheckpoint::default()
        } else {
            serde_json::from_value(task.checkpoint.clone())
                .map_err(|e| TaskQueueError::Execution(format!("checkpoint 解析失败: {e}")))?
        };
        tracing::debug!(
            task_id = %task.id,
            stage = ?checkpoint.stage,
            has_plan = checkpoint.plan.is_some(),
            "checkpoint 已加载"
        );

        // 构建 KB（用于会话创建时装配 runtime 内的知识工具）
        let refs_dir = self.project_path.join("references");
        let kb = match crate::builtin_providers::embedding::build_embedding_router(&llm_config) {
            Some(router) => AsyncKnowledgeBase::init(&refs_dir)
                .map_err(|e| TaskQueueError::Execution(format!("知识库初始化失败: {e}")))?
                .with_embedding_provider(Arc::new(router)),
            None => AsyncKnowledgeBase::init(&refs_dir)
                .map_err(|e| TaskQueueError::Execution(format!("知识库初始化失败: {e}")))?,
        };

        // 计算上下文预算
        let budget = ContextBudget::from_config(&llm_config, model_ref)
            .map_err(|e| TaskQueueError::Execution(format!("上下文预算计算失败: {e}")))?;

        // 估算下一篇论文的 token 数（用于判断是否复用会话）
        let next_paper_tokens = estimate_paper_tokens(&self.project_path, ref_id);

        // 获取主窗口（用于推送进度事件）
        let window = self.app.get_webview_window("main");
        let project_path = self.project_path.clone();

        // ── 获取或创建会话，执行 pipeline（带瞬态错误重试）──
        //
        // 使用 block 限定 lease 生命周期：lease 持有 AsyncMutex 守卫，
        // 必须在调用 session_pool.destroy() 之前释放。
        let pipeline_result: Result<(), KbError> = {
            let mut lease = self
                .session_pool
                .acquire_or_create(
                    &llm_config,
                    model_ref,
                    kb,
                    budget,
                    next_paper_tokens,
                )
                .await
                .map_err(|e| {
                    TaskQueueError::Execution(format!("会话获取失败: {e}"))
                })?;

            // 克隆 captures（Arc 廉价复制），然后获取 session 可变借用
            let plan_capture = lease.plan_capture();
            let entry_capture = lease.entry_capture();
            let usage_capture = lease.usage_capture();
            let session = lease.session_mut();

            self.execute_pipeline_with_retry(
                session,
                &mut checkpoint,
                &project_path,
                ref_id,
                options,
                &llm_config,
                &plan_capture,
                &entry_capture,
                &usage_capture,
                cancel,
                |progress| {
                    if let Some(ref window) = window {
                        let _ = window.emit(
                            crate::knowledge_builder::events::EVENT_KB_BUILD_PROGRESS,
                            &progress,
                        );
                    }
                },
            )
            .await
        }; // lease 在此释放

        // 无论 Ok 还是 Err，都持久化 checkpoint（保留进度，支持中断恢复）
        match serde_json::to_value(&checkpoint) {
            Ok(ckpt_value) => {
                if let Err(e) = self.store.update_checkpoint(&task.id, &ckpt_value) {
                    tracing::warn!(task_id = %task.id, error = %e, "持久化 checkpoint 失败");
                }
            }
            Err(e) => {
                tracing::warn!(task_id = %task.id, error = %e, "序列化 checkpoint 失败");
            }
        }

        // ── 分级错误处理 ──
        match pipeline_result {
            Ok(()) => Ok(()),
            Err(KbError::Cancelled) => {
                // 用户取消：保留会话（缓存仍可复用）
                tracing::info!(task_id = %task.id, "用户取消，保留会话");
                Err(TaskQueueError::Cancelled)
            }
            Err(err) => {
                match classify_error(&err) {
                    LlmErrorKind::Transient { .. } => {
                        // 瞬态错误重试耗尽：任务 Failed，但保留会话（下次任务仍可复用）
                        tracing::warn!(task_id = %task.id, "瞬态错误重试耗尽，保留会话");
                        Err(TaskQueueError::Execution(err.to_string()))
                    }
                    LlmErrorKind::Structural(reason) => {
                        // 结构性错误：销毁会话（会话已不可用）
                        tracing::error!(
                            task_id = %task.id,
                            reason = ?reason,
                            "结构性错误，销毁会话"
                        );
                        self.session_pool.destroy().await;
                        Err(TaskQueueError::Execution(err.to_string()))
                    }
                }
            }
        }
    }

    /// 带瞬态错误重试的 pipeline 执行。
    ///
    /// - 瞬态错误（429/503/Timeout/Network）：保留会话，指数退避重试（最多 3 次）
    /// - 结构性错误（Context Overflow/Schema/Parse/Auth）：不重试，直接返回
    /// - 用户取消：不重试，直接返回
    ///
    /// 重试时 pipeline 从 checkpoint 继续，已完成的条目不会重复创建。
    async fn execute_pipeline_with_retry(
        &self,
        session: &mut crate::knowledge_builder::session::KnowledgeBuildSession,
        checkpoint: &mut KnowledgeBuildCheckpoint,
        project_path: &std::path::Path,
        ref_id: &str,
        options: &crate::knowledge_builder::types::KnowledgeBuildOptions,
        llm_config: &crate::llm_config::model::LlmConfig,
        plan_capture: &crate::knowledge_builder::llm_helper::PlanCapture,
        entry_capture: &crate::knowledge_builder::llm_helper::CreateEntryCapture,
        usage_capture: &crate::knowledge_builder::llm_helper::UsageCapture,
        cancel: &CancellationToken,
        on_progress: impl Fn(crate::knowledge_builder::events::KbBuildProgressPayload),
    ) -> Result<(), KbError> {
        let mut attempt = 0u32;
        loop {
            let result = crate::knowledge_builder::pipeline::build(
                project_path,
                ref_id,
                options,
                checkpoint,
                llm_config,
                session,
                plan_capture,
                entry_capture,
                usage_capture,
                cancel,
                &on_progress,
            )
            .await;

            match result {
                Ok(()) => return Ok(()),
                Err(KbError::Cancelled) => return Err(KbError::Cancelled),
                Err(err) => {
                    match classify_error(&err) {
                        LlmErrorKind::Transient { retry_after_ms } => {
                            attempt += 1;
                            if attempt > MAX_TRANSIENT_RETRIES {
                                tracing::warn!(attempt, "瞬态错误重试耗尽");
                                return Err(err);
                            }
                            let delay = backoff_delay(attempt - 1).max(retry_after_ms);
                            tracing::warn!(
                                attempt,
                                delay_ms = delay,
                                "瞬态错误，退避重试"
                            );
                            tokio::time::sleep(Duration::from_millis(delay)).await;
                            // 保留会话，从 checkpoint 继续重试
                        }
                        LlmErrorKind::Structural(_) => {
                            return Err(err);
                        }
                    }
                }
            }
        }
    }

    fn emit_started(&self, task: &TaskRecord) {
        let Some(window) = self.app.get_webview_window("main") else {
            return;
        };
        match &task.kind {
            TaskKind::KnowledgeBuild { ref_id, .. } => {
                let _ = window.emit(
                    EVENT_KB_BUILD_STARTED,
                    KbBuildStartedPayload {
                        task_id: task.id.clone(),
                        ref_id: ref_id.clone(),
                        title: None,
                    },
                );
            }
            TaskKind::ReferenceImport {
                reference_id,
                file_path,
                ..
            } => {
                let _ = window.emit(
                    EVENT_IMPORT_STARTED,
                    ImportStartedPayload {
                        job_id: task.id.clone(),
                        reference_id: reference_id.clone(),
                        filename: filename_of(file_path),
                    },
                );
            }
        }
    }

    fn emit_completed(&self, task: &TaskRecord) {
        let Some(window) = self.app.get_webview_window("main") else {
            return;
        };
        match &task.kind {
            TaskKind::KnowledgeBuild { ref_id, .. } => {
                // 从 checkpoint 提取 completed payload 字段
                let (summary_id, concept_ids, entity_ids, relations) =
                    extract_completed_fields(task);
                let _ = window.emit(
                    EVENT_KB_BUILD_COMPLETED,
                    KbBuildCompletedPayload {
                        task_id: task.id.clone(),
                        ref_id: ref_id.clone(),
                        summary_id,
                        concept_ids,
                        entity_ids,
                        relations_established: relations,
                    },
                );
            }
            TaskKind::ReferenceImport { .. } => {
                // 完成事件已在 execute_reference_import 成功时携带 entry 推送，
                // 此处不再重复 emit（避免二次读索引与重复事件）。
            }
        }
    }

    fn emit_failed(&self, task: &TaskRecord, error: &str) {
        let Some(window) = self.app.get_webview_window("main") else {
            return;
        };
        match &task.kind {
            TaskKind::KnowledgeBuild { ref_id, .. } => {
                let _ = window.emit(
                    EVENT_KB_BUILD_FAILED,
                    KbBuildFailedPayload {
                        task_id: task.id.clone(),
                        ref_id: ref_id.clone(),
                        error: error.to_string(),
                    },
                );
            }
            TaskKind::ReferenceImport { reference_id, .. } => {
                let _ = window.emit(
                    EVENT_IMPORT_FAILED,
                    ImportFailedPayload {
                        job_id: task.id.clone(),
                        reference_id: reference_id.clone(),
                        error: error.to_string(),
                    },
                );
            }
        }
    }

    fn emit_cancelled(&self, task: &TaskRecord) {
        let Some(window) = self.app.get_webview_window("main") else {
            return;
        };
        match &task.kind {
            TaskKind::KnowledgeBuild { ref_id, .. } => {
                let _ = window.emit(
                    EVENT_KB_BUILD_CANCELLED,
                    KbBuildCancelledPayload {
                        task_id: task.id.clone(),
                        ref_id: ref_id.clone(),
                    },
                );
            }
            // 文献导入无独立取消事件：沿用失败事件 + 固定文案「已取消」，
            // 与前端 CANCEL_MESSAGE 约定保持一致。
            TaskKind::ReferenceImport { reference_id, .. } => {
                let _ = window.emit(
                    EVENT_IMPORT_FAILED,
                    ImportFailedPayload {
                        job_id: task.id.clone(),
                        reference_id: reference_id.clone(),
                        error: "已取消".into(),
                    },
                );
            }
        }
    }
}

/// 从文件路径提取文件名（用于事件展示），失败时回退 `"unknown"`。
fn filename_of(file_path: &str) -> String {
    std::path::Path::new(file_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".into())
}

/// 从任务 checkpoint 中提取 completed payload 所需字段。
fn extract_completed_fields(
    task: &TaskRecord,
) -> (Option<String>, Vec<String>, Vec<String>, bool) {
    if task.checkpoint.is_null() {
        return (None, Vec::new(), Vec::new(), false);
    }
    let ck: KnowledgeBuildCheckpoint = match serde_json::from_value(task.checkpoint.clone()) {
        Ok(c) => c,
        Err(_) => return (None, Vec::new(), Vec::new(), false),
    };
    (
        ck.summary_id,
        ck.concept_ids,
        ck.entity_ids,
        ck.relations_established,
    )
}

/// 估算文献 MD 的 token 数（用于会话预算判断）。
///
/// 粗略估算：`chars / 2`（中英混合文本约 2 字符 = 1 token）。
/// 文件不存在时返回默认值 20_000（典型论文大小）。
fn estimate_paper_tokens(project_path: &std::path::Path, ref_id: &str) -> usize {
    let md_path = project_path
        .join("references")
        .join("md")
        .join(format!("{ref_id}.md"));
    match std::fs::read_to_string(&md_path) {
        Ok(content) => {
            let tokens = content.chars().count() / 2;
            tracing::debug!(ref_id = %ref_id, estimated_tokens = tokens, "论文 token 估算");
            tokens
        }
        Err(_) => {
            tracing::debug!(
                ref_id = %ref_id,
                "文献 MD 不存在，使用默认 token 估算"
            );
            20_000
        }
    }
}

/// 取消指定任务的辅助函数（供 state 调用）。
///
/// 取出对应 task_id 的 CancellationToken 并调用 cancel()。
/// 返回是否成功取消（false 表示任务不在活跃集合中）。
pub fn cancel_task(
    cancel_tokens: &Arc<Mutex<HashMap<String, CancellationToken>>>,
    task_id: &str,
) -> bool {
    let tokens = cancel_tokens.lock().unwrap();
    if let Some(token) = tokens.get(task_id) {
        token.cancel();
        true
    } else {
        false
    }
}

/// 检查任务是否正在执行（用于状态校验）。
pub fn is_task_running(
    cancel_tokens: &Arc<Mutex<HashMap<String, CancellationToken>>>,
    task_id: &str,
) -> bool {
    cancel_tokens.lock().unwrap().contains_key(task_id)
}
