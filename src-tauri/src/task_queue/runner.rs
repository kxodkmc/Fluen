//! 项目级串行任务执行器。
//!
//! 每个 [`TaskRunner`] 绑定一个项目，串行执行该项目下的 Pending 任务。
//! 通过 [`TaskQueueState`] 管理各项目的 runner 生命周期。
//!
//! ## 设计
//!
//! - **单 worker 串行**：同一项目内任务严格按 FIFO 顺序执行，避免并发写入知识库。
//! - **取消令牌**：每个任务关联一个 [`CancellationToken`]，支持运行中取消。
//! - **事件推送**：通过 Tauri `Window::emit` 推送任务进度与终态事件到前端。
//! - **checkpoint 持久化**：执行中持续更新 checkpoint，中断后可从断点恢复。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;

use crate::knowledge_builder::events::{
    KbBuildCancelledPayload, KbBuildCompletedPayload, KbBuildFailedPayload, KbBuildStartedPayload,
    EVENT_KB_BUILD_CANCELLED, EVENT_KB_BUILD_COMPLETED, EVENT_KB_BUILD_FAILED,
    EVENT_KB_BUILD_STARTED,
};
use crate::knowledge_builder::types::KnowledgeBuildCheckpoint;
use crate::knowledge_builder::KnowledgeBuilderError as KbError;
use crate::llm_config::storage::ConfigStorage as LlmConfigStorage;

use super::error::TaskQueueError;
use super::store::TaskStore;
use super::types::{TaskKind, TaskRecord, TaskStatus};

/// 项目级串行任务执行器。
///
/// 持有项目路径、任务存储、LLM 配置与 Tauri 句柄。
/// 通过 [`TaskRunner::run_once`] 执行下一个 Pending 任务，
/// 通过 [`TaskRunner::run_loop`] 持续消费直至无 Pending 任务。
pub struct TaskRunner {
    /// 项目根路径。
    project_path: PathBuf,
    /// 任务存储（同项目共享）。
    store: Arc<TaskStore>,
    /// LLM 配置存储（用于解析场景化模型）。
    llm_storage: Arc<LlmConfigStorage>,
    /// Tauri 应用句柄（用于 emit 事件与获取窗口）。
    app: AppHandle,
    /// 取消令牌集合（task_id → token）。
    cancel_tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl TaskRunner {
    /// 创建 runner 实例。
    pub fn new(
        project_path: impl Into<PathBuf>,
        store: Arc<TaskStore>,
        llm_storage: Arc<LlmConfigStorage>,
        app: AppHandle,
        cancel_tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
    ) -> Self {
        Self {
            project_path: project_path.into(),
            store,
            llm_storage,
            app,
            cancel_tokens,
        }
    }

    /// 持续消费 Pending 任务，直到无任务可执行。
    ///
    /// 单 worker 串行：每次循环拾取最早创建的 Pending 任务，执行完毕后再取下一个。
    pub async fn run_loop(self) {
        loop {
            match self.store.next_pending() {
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
        }
    }

    /// 执行知识库构建任务。
    ///
    /// 委托给 [`crate::knowledge_builder::pipeline::build`]，
    /// 持久化产出的 checkpoint，并推送进度事件。
    async fn execute_knowledge_build(
        &self,
        task: &TaskRecord,
        ref_id: &str,
        model_ref: &crate::llm_config::model::SceneModelRef,
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

        // 获取主窗口（用于推送进度事件）
        let window = self.app.get_webview_window("main");

        // 执行 pipeline（checkpoint 被原地更新）
        let project_path = self.project_path.clone();
        let result = crate::knowledge_builder::pipeline::build(
            &project_path,
            ref_id,
            model_ref,
            options,
            &mut checkpoint,
            &llm_config,
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
        .await;

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

        // 返回执行结果
        result.map_err(|e| match e {
            KbError::Cancelled => TaskQueueError::Cancelled,
            other => {
                tracing::error!(task_id = %task.id, ref_id = %ref_id, error = %other, "知识库构建 pipeline 失败");
                TaskQueueError::Execution(other.to_string())
            }
        })?;

        Ok(())
    }

    fn emit_started(&self, task: &TaskRecord) {
        if let Some(window) = self.app.get_webview_window("main") {
            let ref_id = task.kind.ref_id().map(|s| s.to_string());
            let _ = window.emit(
                EVENT_KB_BUILD_STARTED,
                KbBuildStartedPayload {
                    task_id: task.id.clone(),
                    ref_id: ref_id.clone().unwrap_or_default(),
                    title: None,
                },
            );
        }
    }

    fn emit_completed(&self, task: &TaskRecord) {
        if let Some(window) = self.app.get_webview_window("main") {
            let ref_id = task.kind.ref_id().map(|s| s.to_string()).unwrap_or_default();
            // 从 checkpoint 提取 completed payload 字段
            let (summary_id, concept_ids, entity_ids, relations) =
                extract_completed_fields(task);
            let _ = window.emit(
                EVENT_KB_BUILD_COMPLETED,
                KbBuildCompletedPayload {
                    task_id: task.id.clone(),
                    ref_id,
                    summary_id,
                    concept_ids,
                    entity_ids,
                    relations_established: relations,
                },
            );
        }
    }

    fn emit_failed(&self, task: &TaskRecord, error: &str) {
        if let Some(window) = self.app.get_webview_window("main") {
            let ref_id = task.kind.ref_id().map(|s| s.to_string()).unwrap_or_default();
            let _ = window.emit(
                EVENT_KB_BUILD_FAILED,
                KbBuildFailedPayload {
                    task_id: task.id.clone(),
                    ref_id,
                    error: error.to_string(),
                },
            );
        }
    }

    fn emit_cancelled(&self, task: &TaskRecord) {
        if let Some(window) = self.app.get_webview_window("main") {
            let ref_id = task.kind.ref_id().map(|s| s.to_string()).unwrap_or_default();
            let _ = window.emit(
                EVENT_KB_BUILD_CANCELLED,
                KbBuildCancelledPayload {
                    task_id: task.id.clone(),
                    ref_id,
                },
            );
        }
    }
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
