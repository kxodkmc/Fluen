//! 跨论文会话管理：runtime 复用 + 真实 usage 跟踪 + 销毁决策。
//!
//! V2.1 核心模块。会话跨越多篇论文时复用 runtime，命中模型前缀缓存
//! （DeepSeek / Anthropic 可降本 50-90%）。
//!
//! ## 设计
//!
//! - **runtime 复用**：单例 runtime，跨论文不重建。
//! - **真实 usage 跟踪**：`history_used` 取会话最后一次 LLM 调用的
//!   `(prompt_tokens + completion_tokens)`，避免估算误差导致 Context Overflow。
//! - **销毁决策**：`should_close` 判断超 75% 预算或不支持缓存时主动关闭。
//! - **不持有 Index 快照**：磁盘是唯一真相源，每次实时读取。
//!
//! ## SessionPool
//!
//! [`SessionPool`] 持有单例会话，提供 `acquire` / `destroy` / `keep` 接口。
//! 由 [`TaskQueueState`](crate::task_queue::state::TaskQueueState) 持有，
//! 跨任务共享（同一项目的多篇论文复用同一会话）。

use std::sync::{Arc, Mutex};

use confluent::ConfluentRuntime;
use fluen_knowledge::async_kb::AsyncKnowledgeBase;
use tokio::sync::Mutex as AsyncMutex;

use crate::llm_config::model::{LlmConfig, SceneModelRef};

use super::context_budget::ContextBudget;
use super::error::KnowledgeBuilderError;
use super::llm_helper::{
    build_kb_runtime, CreateEntryCapture, PlanCapture, UsageCapture, UsageSnapshot,
};

/// 跨论文会话：runtime + 预算 + 真实 usage 跟踪。
///
/// 一个会话可处理多篇论文，通过复用 runtime 命中模型前缀缓存。
/// 会话销毁后无法恢复 runtime 历史，但 Task 的 checkpoint 仍可恢复。
pub struct KnowledgeBuildSession {
    runtime: ConfluentRuntime,
    budget: ContextBudget,
    /// 已处理论文 ID 列表（仅统计用，不参与恢复）。
    processed: Vec<String>,
    /// 真实上下文占用：取会话最后一次 LLM 调用的 (prompt + completion)。
    ///
    /// 由 [`update_usage`](Self::update_usage) 从 LLM 响应的 usage 字段更新。
    /// `prompt_tokens` 已包含完整历史（system + 所有历史轮次 + 当前轮输入）。
    history_used: usize,
}

impl KnowledgeBuildSession {
    /// 构建新会话：装配 runtime 与捕获器。
    ///
    /// 返回 `(session, plan_capture, entry_capture, usage_capture)`，
    /// pipeline 持有 capture 引用，在 `runtime.run()` 后读取结果。
    pub async fn new(
        llm: &LlmConfig,
        model_ref: &SceneModelRef,
        kb: AsyncKnowledgeBase,
        budget: ContextBudget,
    ) -> Result<(Self, PlanCapture, CreateEntryCapture, UsageCapture), KnowledgeBuilderError> {
        let plan_capture: PlanCapture = Arc::new(Mutex::new(None));
        let entry_capture: CreateEntryCapture = Arc::new(Mutex::new(None));
        let usage_capture: UsageCapture = Arc::new(Mutex::new(None));

        let runtime = build_kb_runtime(
            llm,
            model_ref,
            kb,
            plan_capture.clone(),
            entry_capture.clone(),
            usage_capture.clone(),
        )
        .await?;

        Ok((
            Self {
                runtime,
                budget,
                processed: Vec::new(),
                history_used: 0,
            },
            plan_capture,
            entry_capture,
            usage_capture,
        ))
    }

    /// 借用 runtime（pipeline 用于 `runtime.run()`）。
    pub fn runtime(&self) -> &ConfluentRuntime {
        &self.runtime
    }

    /// 借用预算配置。
    pub fn budget(&self) -> &ContextBudget {
        &self.budget
    }

    /// 当前会话已用上下文（tokens）。
    pub fn history_used(&self) -> usize {
        self.history_used
    }

    /// 已处理论文数。
    pub fn papers_processed(&self) -> usize {
        self.processed.len()
    }

    /// 从 LLM 响应的 usage 字段更新真实占用。
    ///
    /// 在每次 `runtime.run()` 返回后调用。
    /// `prompt_tokens` 已包含完整历史（system + 所有历史轮次 + 当前轮输入）。
    pub fn update_usage(&mut self, snapshot: UsageSnapshot) {
        self.history_used = snapshot.total();
        tracing::debug!(
            history_used = self.history_used,
            budget = self.budget.history_budget(),
            "会话 usage 已更新"
        );
    }

    /// 记录已处理论文（仅统计用，不参与恢复）。
    pub fn record_paper(&mut self, ref_id: &str) {
        self.processed.push(ref_id.to_string());
        tracing::info!(
            ref_id = %ref_id,
            papers_processed = self.processed.len(),
            history_used = self.history_used,
            "会话已记录论文"
        );
    }

    /// 是否应该关闭会话（超 75% 或不支持缓存）。
    ///
    /// 在每篇论文处理完毕后判断，决定下一篇是否复用。
    pub fn should_close(&self, next_paper_md_tokens: usize) -> bool {
        !self
            .budget
            .can_append_paper(self.history_used, next_paper_md_tokens)
    }
}

impl std::fmt::Debug for KnowledgeBuildSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KnowledgeBuildSession")
            .field("budget", &self.budget)
            .field("processed", &self.processed)
            .field("history_used", &self.history_used)
            .finish_non_exhaustive()
    }
}

/// 会话池：持有单例会话，跨任务共享。
///
/// 由 [`TaskQueueState`](crate::task_queue::state::TaskQueueState) 持有，
/// 同一项目的多篇论文通过 [`SessionPool::acquire`] 复用同一会话。
///
/// ## 销毁时机
///
/// - **结构性错误**：[`SessionPool::destroy`]（会话已不可用）
/// - **历史占用超 75%**：[`SessionPool::destroy`]（主动关闭，下次新建）
/// - **进程崩溃**：启动时 [`SessionPool::clear`]（runtime 历史无法恢复）
/// - **瞬态错误 / 用户取消**：保留会话（缓存可复用）
pub struct SessionPool {
    /// 当前活跃会话（含 capture 引用）。
    ///
    /// 使用 `AsyncMutex` 因为 `acquire` 是 async 操作，且需保证并发安全。
    inner: AsyncMutex<Option<ActiveSession>>,
}

/// 活跃会话：session + 共享 capture 引用。
///
/// capture 与 session 一对一绑定，跨论文复用。
struct ActiveSession {
    session: KnowledgeBuildSession,
    plan_capture: PlanCapture,
    entry_capture: CreateEntryCapture,
    usage_capture: UsageCapture,
}

impl Default for SessionPool {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionPool {
    /// 创建空会话池。
    pub fn new() -> Self {
        Self {
            inner: AsyncMutex::new(None),
        }
    }

    /// 获取或重建会话。
    ///
    /// - 若池中已有会话且未超预算：直接复用
    /// - 若池为空或会话已超预算：销毁旧会话，构建新会话
    ///
    /// 返回 `SessionLease`，包含 session 可变引用等价物与 capture 引用。
    /// 调用方在 lease 生命周期内独占访问。
    pub async fn acquire_or_create(
        &self,
        llm: &LlmConfig,
        model_ref: &SceneModelRef,
        kb: AsyncKnowledgeBase,
        budget: ContextBudget,
        next_paper_md_tokens: usize,
    ) -> Result<SessionLease<'_>, KnowledgeBuilderError> {
        let mut guard = self.inner.lock().await;

        // 检查现有会话是否可复用
        let need_rebuild = match guard.as_ref() {
            None => true,
            Some(active) => {
                let should_close = active.session.should_close(next_paper_md_tokens);
                if should_close {
                    tracing::info!(
                        history_used = active.session.history_used(),
                        next_paper_tokens = next_paper_md_tokens,
                        "现有会话超预算，销毁后重建"
                    );
                }
                should_close
            }
        };

        if need_rebuild {
            // 销毁旧会话（drop）
            *guard = None;

            // 构建新会话
            let (session, plan_capture, entry_capture, usage_capture) =
                KnowledgeBuildSession::new(llm, model_ref, kb, budget).await?;
            *guard = Some(ActiveSession {
                session,
                plan_capture,
                entry_capture,
                usage_capture,
            });
        }

        // 返回 lease（持有 guard）
        Ok(SessionLease { guard })
    }

    /// 强制销毁会话（结构性错误恢复用）。
    pub async fn destroy(&self) {
        let mut guard = self.inner.lock().await;
        if guard.is_some() {
            tracing::info!("销毁会话池中的会话（结构性错误）");
        }
        *guard = None;
    }

    /// 保留会话（瞬态错误重试或用户取消）。
    ///
    /// 语义明确：不销毁，缓存可复用。无操作。
    pub async fn keep(&self) {
        // no-op
    }

    /// 清空会话池（启动时调用）。
    ///
    /// 进程崩溃后 runtime 历史无法恢复，必须清理。
    pub async fn clear(&self) {
        let mut guard = self.inner.lock().await;
        if guard.is_some() {
            tracing::info!("清理会话池（启动恢复）");
        }
        *guard = None;
    }

    /// 当前会话已用上下文（None 表示无活跃会话）。
    pub async fn history_used(&self) -> Option<usize> {
        self.inner
            .lock()
            .await
            .as_ref()
            .map(|a| a.session.history_used())
    }

    /// 当前会话已处理论文数。
    pub async fn papers_processed(&self) -> usize {
        self.inner
            .lock()
            .await
            .as_ref()
            .map(|a| a.session.papers_processed())
            .unwrap_or(0)
    }
}

/// 会话租约：持有 `AsyncMutex` 守卫，提供对 session 与 capture 的访问。
///
/// 生命周期内独占访问 session。drop 时自动释放锁。
pub struct SessionLease<'a> {
    guard: tokio::sync::MutexGuard<'a, Option<ActiveSession>>,
}

impl<'a> SessionLease<'a> {
    /// 借用 session（不可变）。
    pub fn session(&self) -> &KnowledgeBuildSession {
        &self
            .guard
            .as_ref()
            .expect("lease 仅在 acquire 后存在")
            .session
    }

    /// 可变借用 session。
    pub fn session_mut(&mut self) -> &mut KnowledgeBuildSession {
        &mut self
            .guard
            .as_mut()
            .expect("lease 仅在 acquire 后存在")
            .session
    }

    /// 借用 plan_capture。
    pub fn plan_capture(&self) -> PlanCapture {
        self.guard
            .as_ref()
            .expect("lease 仅在 acquire 后存在")
            .plan_capture
            .clone()
    }

    /// 借用 entry_capture。
    pub fn entry_capture(&self) -> CreateEntryCapture {
        self.guard
            .as_ref()
            .expect("lease 仅在 acquire 后存在")
            .entry_capture
            .clone()
    }

    /// 借用 usage_capture。
    pub fn usage_capture(&self) -> UsageCapture {
        self.guard
            .as_ref()
            .expect("lease 仅在 acquire 后存在")
            .usage_capture
            .clone()
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn should_close_when_above_budget() {
        let budget = ContextBudget::new(256_000);
        // history_budget = 174000, history_used=130000, next=20000
        // 130000 + 50000 = 180000 > 174000 → should_close=true
        // 直接验证 budget 逻辑（session.should_close 委托给 budget.can_append_paper）
        let should_close = !budget.can_append_paper(130_000, 20_000);
        assert!(should_close);
    }

    #[test]
    fn should_not_close_when_below_budget() {
        let budget = ContextBudget::new(256_000);
        let should_close = !budget.can_append_paper(50_000, 20_000);
        assert!(!should_close);
    }

    #[tokio::test]
    async fn session_pool_default_is_empty() {
        let pool = SessionPool::new();
        assert_eq!(pool.papers_processed().await, 0);
        assert_eq!(pool.history_used().await, None);
    }

    #[tokio::test]
    async fn session_pool_clear_silent_on_empty() {
        let pool = SessionPool::new();
        // 不应 panic
        pool.clear().await;
        assert_eq!(pool.papers_processed().await, 0);
    }

    #[tokio::test]
    async fn session_pool_destroy_silent_on_empty() {
        let pool = SessionPool::new();
        // 不应 panic
        pool.destroy().await;
        assert_eq!(pool.history_used().await, None);
    }

    #[tokio::test]
    async fn session_pool_keep_is_noop() {
        let pool = SessionPool::new();
        // 不应 panic
        pool.keep().await;
    }
}
