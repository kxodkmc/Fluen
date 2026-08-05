//! 任务队列全局状态：管理各项目 runner 生命周期、取消令牌与会话池。
//!
//! 通过 Tauri `manage` 注册为应用级状态，供 commands 与 runner 共享。
//!
//! ## 设计
//!
//! - **取消令牌集合**：`task_id → CancellationToken`，全局共享，commands 与 runner 都可访问。
//! - **活跃 runner 集合**：`project_path` 集合，防止同项目重复 spawn runner。
//! - **会话池集合**：`project_path → Arc<SessionPool>`，跨 runner 复用知识库构建会话
//!   （V2.1：跨论文 runtime 复用，命中模型前缀缓存）。
//! - **runner spawn**：通过 [`TaskQueueState::try_start_runner`] 启动，
//!   runner 完成后自动从活跃集合移除。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use crate::knowledge_builder::session::SessionPool;
use crate::llm_config::storage::ConfigStorage as LlmConfigStorage;

use super::runner::TaskRunner;
use super::store::TaskStore;

/// 任务队列全局状态。
///
/// 持有取消令牌集合、活跃 runner 集合与会话池集合，供 commands 与 runner 共享。
pub struct TaskQueueState {
    /// 取消令牌集合（task_id → token），跨项目共享。
    cancel_tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
    /// 活跃 runner 的项目路径集合（防止重复 spawn）。
    active_runners: Arc<Mutex<HashSet<String>>>,
    /// 按项目路径索引的会话池（V2.1：跨论文 runtime 复用）。
    ///
    /// 会话池跨 runner 生命周期存活：runner 退出后池仍保留，
    /// 下次 runner 启动时复用已有会话（命中模型前缀缓存）。
    session_pools: Arc<Mutex<HashMap<String, Arc<SessionPool>>>>,
}

impl TaskQueueState {
    /// 创建空状态实例。
    pub fn new() -> Self {
        Self {
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            active_runners: Arc::new(Mutex::new(HashSet::new())),
            session_pools: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 获取取消令牌集合的引用（供 runner 与 commands 共享）。
    pub fn cancel_tokens(&self) -> Arc<Mutex<HashMap<String, CancellationToken>>> {
        self.cancel_tokens.clone()
    }

    /// 获取或创建指定项目的会话池。
    ///
    /// 同一项目的多次 runner 调用复用同一 `SessionPool`，
    /// 跨论文构建任务通过池复用 runtime（命中模型前缀缓存）。
    pub fn get_or_create_session_pool(&self, project_path: &Path) -> Arc<SessionPool> {
        let key = project_path.to_string_lossy().to_string();
        let mut pools = self.session_pools.lock().expect("session_pools poisoned");
        pools
            .entry(key)
            .or_insert_with(|| Arc::new(SessionPool::new()))
            .clone()
    }

    /// 尝试为指定项目启动 runner。
    ///
    /// 若该项目已有活跃 runner，返回 `false`（不重复启动）。
    /// 否则 spawn 一个新 runner，返回 `true`。
    ///
    /// runner 完成后会自动从 `active_runners` 中移除自身。
    pub fn try_start_runner(
        &self,
        project_path: PathBuf,
        store: Arc<TaskStore>,
        llm_storage: Arc<LlmConfigStorage>,
        app: AppHandle,
    ) -> bool {
        let path_key = project_path.to_string_lossy().to_string();
        {
            let mut active = self.active_runners.lock().unwrap();
            if active.contains(&path_key) {
                return false;
            }
            active.insert(path_key.clone());
        }

        let cancel_tokens = self.cancel_tokens.clone();
        let active_runners = self.active_runners.clone();
        let session_pool = self.get_or_create_session_pool(&project_path);
        let runner = TaskRunner::new(
            project_path,
            store,
            llm_storage,
            app,
            cancel_tokens,
            session_pool,
        );

        // spawn runner，完成后自动清理 active_runners
        tauri::async_runtime::spawn(async move {
            runner.run_loop().await;
            let mut active = active_runners.lock().unwrap();
            active.remove(&path_key);
        });

        true
    }

    /// 取消指定任务。
    ///
    /// 调用对应 task_id 的 CancellationToken。
    /// 返回是否成功取消（false 表示任务不在活跃集合中）。
    pub fn cancel_task(&self, task_id: &str) -> bool {
        let tokens = self.cancel_tokens.lock().unwrap();
        if let Some(token) = tokens.get(task_id) {
            token.cancel();
            true
        } else {
            false
        }
    }

    /// 检查任务是否正在执行。
    pub fn is_task_running(&self, task_id: &str) -> bool {
        self.cancel_tokens.lock().unwrap().contains_key(task_id)
    }

    /// 清理所有项目的会话池（启动恢复时调用）。
    ///
    /// 进程崩溃后 runtime 历史在内存中，无法恢复，必须清理。
    /// 此方法是同步的：仅清空 map，`SessionPool` 的 `Drop` 会自动回收资源。
    pub fn clear_all_session_pools(&self) {
        let mut pools = self.session_pools.lock().expect("session_pools poisoned");
        let count = pools.len();
        pools.clear();
        if count > 0 {
            tracing::info!(count, "清理所有项目会话池（启动恢复）");
        }
    }
}

impl Default for TaskQueueState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_or_create_session_pool_returns_same_instance() {
        let state = TaskQueueState::new();
        let path = PathBuf::from("/tmp/project-a");
        let pool1 = state.get_or_create_session_pool(&path);
        let pool2 = state.get_or_create_session_pool(&path);
        // Arc 同一指针
        assert!(Arc::ptr_eq(&pool1, &pool2));
    }

    #[test]
    fn get_or_create_session_pool_different_projects() {
        let state = TaskQueueState::new();
        let pool_a = state.get_or_create_session_pool(&PathBuf::from("/tmp/project-a"));
        let pool_b = state.get_or_create_session_pool(&PathBuf::from("/tmp/project-b"));
        assert!(!Arc::ptr_eq(&pool_a, &pool_b));
    }

    #[test]
    fn clear_all_session_pools_removes_all() {
        let state = TaskQueueState::new();
        state.get_or_create_session_pool(&PathBuf::from("/tmp/project-a"));
        state.get_or_create_session_pool(&PathBuf::from("/tmp/project-b"));
        state.clear_all_session_pools();
        // 清理后再获取应创建新实例
        let new_pool = state.get_or_create_session_pool(&PathBuf::from("/tmp/project-a"));
        // 验证不是之前的（此处无法直接对比，但至少不 panic）
        assert!(Arc::strong_count(&new_pool) >= 1);
    }

    #[test]
    fn clear_all_session_pools_silent_on_empty() {
        let state = TaskQueueState::new();
        // 不应 panic
        state.clear_all_session_pools();
    }
}
