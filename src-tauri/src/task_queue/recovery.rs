//! 启动时中断接续。
//!
//! App 启动时调用 [`recover_on_startup`]：
//! 1. 清理所有项目的会话池（runtime 历史在内存中，崩溃后无法恢复）。
//! 2. 对每个项目，将 Running 任务重置为 Pending。
//! 3. 若项目存在 Pending 任务，触发 [`TaskRunner`] 继续执行。
//!
//! ## 设计
//!
//! 不在启动时硬编码扫描文件系统（避免依赖项目根路径解析），
//! 由调用方传入需要恢复的项目路径列表。
//!
//! 会话池清理为全局一次性操作：进程崩溃后所有 runtime 历史均失效，
//! 必须全部销毁；新会话从 checkpoint 继续，业务正确性不受影响。

use std::path::PathBuf;
use std::sync::Arc;

use tauri::AppHandle;

use crate::ai_services::storage::ConfigStorage as AiServicesConfigStorage;
use crate::llm_config::storage::ConfigStorage as LlmConfigStorage;

use super::state::TaskQueueState;
use super::store::TaskStore;

/// 全部任务种类（启动恢复时按种类分别检查并启动对应 runner）。
///
/// 新增任务种类时在此登记即可自动获得崩溃恢复能力。
const ALL_TASK_KINDS: [&str; 2] = ["knowledge_build", "reference_import"];

/// 启动时恢复：对给定项目列表执行中断接续。
///
/// 流程：
/// 1. 清理所有项目的会话池（runtime 历史在内存中，崩溃后无法恢复）。
/// 2. 对每个项目，创建 `TaskStore`，将 Running 任务重置为 Pending。
/// 3. 对每种任务种类，若存在 Pending 任务，spawn 对应种类的
///    `TaskRunner` 继续执行（不同种类并行，互不阻塞）。
///
/// 返回触发恢复的 runner 数量。
pub fn recover_on_startup(
    app: &AppHandle,
    llm_storage: &Arc<LlmConfigStorage>,
    ai_storage: &Arc<AiServicesConfigStorage>,
    state: &TaskQueueState,
    project_paths: Vec<PathBuf>,
) -> usize {
    // 启动时必须清理会话池：进程崩溃后 runtime 历史在内存中无法恢复，
    // 必须全部销毁；新会话从 checkpoint 继续，业务正确性不受影响。
    state.clear_all_session_pools();

    let mut recovered = 0usize;
    for project_path in project_paths {
        let store = Arc::new(TaskStore::new(&project_path));

        // 将 Running 任务重置为 Pending
        match store.reset_running_to_pending() {
            Ok(reset_ids) => {
                if !reset_ids.is_empty() {
                    tracing::info!(
                        project = %project_path.display(),
                        count = reset_ids.len(),
                        "恢复时重置 Running 任务为 Pending"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    project = %project_path.display(),
                    error = %e,
                    "恢复时重置 Running 任务失败"
                );
                continue;
            }
        }

        // 按种类分别检查并启动对应 runner
        for kind in ALL_TASK_KINDS {
            let has_pending = match store.next_pending_of(kind) {
                Ok(Some(_)) => true,
                Ok(None) => false,
                Err(e) => {
                    tracing::warn!(
                        project = %project_path.display(),
                        kind,
                        error = %e,
                        "读取 Pending 任务失败"
                    );
                    false
                }
            };

            if has_pending {
                if state.try_start_runner(
                    project_path.clone(),
                    kind,
                    store.clone(),
                    llm_storage.clone(),
                    ai_storage.clone(),
                    app.clone(),
                ) {
                    recovered += 1;
                }
            }
        }
    }
    recovered
}
