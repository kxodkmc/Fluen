//! 任务队列全局状态：管理各项目 runner 生命周期与取消令牌。
//!
//! 通过 Tauri `manage` 注册为应用级状态，供 commands 与 runner 共享。
//!
//! ## 设计
//!
//! - **取消令牌集合**：`task_id → CancellationToken`，全局共享，commands 与 runner 都可访问。
//! - **活跃 runner 集合**：`project_path` 集合，防止同项目重复 spawn runner。
//! - **runner spawn**：通过 [`TaskQueueState::try_start_runner`] 启动，
//!   runner 完成后自动从活跃集合移除。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use crate::llm_config::storage::ConfigStorage as LlmConfigStorage;

use super::runner::TaskRunner;
use super::store::TaskStore;

/// 任务队列全局状态。
///
/// 持有取消令牌集合与活跃 runner 集合，供 commands 与 runner 共享。
pub struct TaskQueueState {
    /// 取消令牌集合（task_id → token），跨项目共享。
    cancel_tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
    /// 活跃 runner 的项目路径集合（防止重复 spawn）。
    active_runners: Arc<Mutex<HashSet<String>>>,
}

impl TaskQueueState {
    /// 创建空状态实例。
    pub fn new() -> Self {
        Self {
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            active_runners: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// 获取取消令牌集合的引用（供 runner 与 commands 共享）。
    pub fn cancel_tokens(&self) -> Arc<Mutex<HashMap<String, CancellationToken>>> {
        self.cancel_tokens.clone()
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
        let runner = TaskRunner::new(
            project_path,
            store,
            llm_storage,
            app,
            cancel_tokens,
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
}

impl Default for TaskQueueState {
    fn default() -> Self {
        Self::new()
    }
}
