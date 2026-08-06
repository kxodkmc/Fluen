//! 知识库构建 Tauri commands——前端调用接口。
//!
//! 分为两类：
//!
//! - **构建任务便捷命令**：[`knowledge_build_start`] 解析场景化模型并入队任务。
//! - **知识库直接访问命令**：list / get / query / meta / init，供前端浏览检索知识库。
//!
//! 任务生命周期管理（cancel / retry / list）由 `task_queue::commands` 提供，
//! 本模块不重复实现。

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, State};
use tokio::task::spawn_blocking;

use fluen_knowledge::async_kb::{AsyncKnowledgeBase, AsyncQueryParams};
use fluen_knowledge::types::{
    MetaQueryType, QueryResult, RetrievalMethod, WikiEntry, WikiEntryDetail, WikiType,
};

use crate::ai_services::storage::ConfigStorage as AiServicesConfigStorage;
use crate::llm_config::model::SceneModelRef;
use crate::llm_config::storage::ConfigStorage as LlmConfigStorage;
use crate::task_queue::state::TaskQueueState;
use crate::task_queue::store::TaskStore;
use crate::task_queue::types::{TaskKind, TaskRecord};

use super::error::KnowledgeBuilderError;
use super::types::KnowledgeBuildOptions;

// ---------------------------------------------------------------------------
// 构建任务便捷命令
// ---------------------------------------------------------------------------

/// 启动知识库构建任务。
///
/// 便捷命令：从 LLM 配置解析知识库构建场景模型，创建 `KnowledgeBuild` 任务并入队。
/// 若未配置 `scene_models.knowledge_build`，回退到全局激活模型；
/// 若全局激活模型也不存在，返回 `Config` 错误。
///
/// # 参数
///
/// - `project_path`：项目根路径
/// - `ref_id`：文献 ID
/// - `options`：构建选项（`None` 使用默认）
#[tauri::command]
pub async fn knowledge_build_start(
    project_path: String,
    ref_id: String,
    options: Option<KnowledgeBuildOptions>,
    llm_storage: State<'_, LlmConfigStorage>,
    ai_storage: State<'_, AiServicesConfigStorage>,
    state: State<'_, TaskQueueState>,
    app: AppHandle,
) -> Result<TaskRecord, KnowledgeBuilderError> {
    tracing::info!(
        project_path = %project_path,
        ref_id = %ref_id,
        options = ?options,
        "knowledge_build_start 命令调用"
    );

    // 加载 LLM 配置，解析场景化模型
    let llm_config = llm_storage
        .load()
        .map_err(|e| KnowledgeBuilderError::Config(e.to_string()))?;

    let (provider, model) = llm_config.resolve_knowledge_build().ok_or_else(|| {
        tracing::error!("知识库构建模型不可用：场景模型引用失效且无全局激活项");
        KnowledgeBuilderError::Config(
            "知识库构建模型不可用：scene_models.knowledge_build 引用失效，或未配置场景模型且无全局激活项".into(),
        )
    })?;

    let model_ref = SceneModelRef {
        provider_id: provider.id.clone(),
        model_id: model,
    };
    tracing::debug!(
        provider_id = %model_ref.provider_id,
        model_id = %model_ref.model_id,
        "已解析场景模型"
    );

    let kind = TaskKind::KnowledgeBuild {
        ref_id,
        model_ref,
        options: options.unwrap_or_default(),
    };

    // 直接通过 TaskStore 入队，并触发 runner
    let project_path_buf = PathBuf::from(&project_path);
    let store = Arc::new(TaskStore::new(&project_path_buf));
    let record = store
        .enqueue(kind)
        .map_err(|e| KnowledgeBuilderError::TaskQueue(e.to_string()))?;

    tracing::info!(task_id = %record.id, "知识库构建任务已入队");

    state.try_start_runner(
        project_path_buf,
        record.kind.kind_name(),
        store,
        Arc::new(llm_storage.inner().clone()),
        Arc::new(ai_storage.inner().clone()),
        app,
    );

    Ok(record)
}

// ---------------------------------------------------------------------------
// 知识库直接访问命令
// ---------------------------------------------------------------------------

/// 初始化项目的知识库目录结构。
///
/// 创建 `references/wiki/` 下的目录与空索引文件。
/// 已存在的文件不会被覆盖。
#[tauri::command]
pub async fn knowledge_init(project_path: String) -> Result<(), KnowledgeBuilderError> {
    let refs_dir = PathBuf::from(&project_path).join("references");
    spawn_blocking(move || {
        fluen_knowledge::wiki::init_wiki(&refs_dir).map_err(KnowledgeBuilderError::from)
    })
    .await
    .map_err(|e| KnowledgeBuilderError::TaskQueue(format!("spawn_blocking join error: {e}")))??;
    Ok(())
}

/// 列出项目知识库中的所有条目（不含正文）。
#[tauri::command]
pub async fn knowledge_list_entries(
    project_path: String,
) -> Result<Vec<WikiEntry>, KnowledgeBuilderError> {
    let kb = open_kb(&project_path)?;
    kb.list_entries()
        .await
        .map_err(KnowledgeBuilderError::from)
}

/// 获取单个条目详情（含正文、标签名、关联条目标题）。
#[tauri::command]
pub async fn knowledge_get_entry(
    project_path: String,
    wiki_id: String,
) -> Result<Option<WikiEntryDetail>, KnowledgeBuilderError> {
    let kb = open_kb(&project_path)?;
    kb.get_entry(wiki_id)
        .await
        .map_err(KnowledgeBuilderError::from)
}

/// 检索知识库。
///
/// # 参数
///
/// - `query`：查询文本
/// - `wiki_type`：限定条目类型（`None` 不限）
/// - `method`：检索方式（`None` 默认 hybrid）
/// - `top_k`：返回条数上限（`None` 默认 10）
#[tauri::command]
pub async fn knowledge_query(
    project_path: String,
    query: String,
    wiki_type: Option<WikiType>,
    method: Option<RetrievalMethod>,
    top_k: Option<usize>,
) -> Result<QueryResult, KnowledgeBuilderError> {
    let kb = open_kb(&project_path)?;
    let params = AsyncQueryParams {
        query,
        wiki_type,
        method: method.unwrap_or_default(),
        top_k: top_k.unwrap_or(10),
        include_content: false,
    };
    kb.query(params).await.map_err(KnowledgeBuilderError::from)
}

/// 查询知识库元信息。
///
/// `query_type` 为 `overview` / `tags` / `recent`，
/// `limit` 仅对 `recent` 有效（返回最近更新的条目数）。
#[tauri::command]
pub async fn knowledge_meta(
    project_path: String,
    query_type: MetaQueryType,
    limit: Option<usize>,
) -> Result<fluen_knowledge::types::MetaResult, KnowledgeBuilderError> {
    let kb = open_kb(&project_path)?;
    kb.meta(query_type, limit.unwrap_or(10))
        .await
        .map_err(KnowledgeBuilderError::from)
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 打开项目的知识库句柄。
///
/// 知识库目录为 `{project_path}/references/wiki/`。
/// 若未初始化（`index.db` 不存在），返回错误提示前端先调用 `knowledge_init`。
fn open_kb(project_path: &str) -> Result<AsyncKnowledgeBase, KnowledgeBuilderError> {
    let refs_dir = PathBuf::from(project_path).join("references");
    let wiki_db = refs_dir.join("wiki").join("index.db");
    if !wiki_db.exists() {
        return Err(KnowledgeBuilderError::Config(format!(
            "知识库未初始化：{} 不存在，请先调用 knowledge_init",
            wiki_db.display()
        )));
    }
    AsyncKnowledgeBase::open(&refs_dir).map_err(KnowledgeBuilderError::from)
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_kb_returns_error_when_not_initialized() {
        let tmp = tempfile::tempdir().unwrap();
        match open_kb(tmp.path().to_str().unwrap()) {
            Err(err) => {
                assert!(matches!(err, KnowledgeBuilderError::Config(_)));
                assert!(err.to_string().contains("知识库未初始化"));
            }
            Ok(_) => panic!("expected error when wiki not initialized"),
        }
    }

    #[tokio::test]
    async fn open_kb_succeeds_after_init() {
        let tmp = tempfile::tempdir().unwrap();
        let refs_dir = tmp.path().join("references");
        fluen_knowledge::wiki::init_wiki(&refs_dir).unwrap();

        let kb = open_kb(tmp.path().to_str().unwrap()).unwrap();
        let meta = kb.meta(MetaQueryType::Overview, 0).await.unwrap();
        assert_eq!(meta.data.total_entries, 0);
    }
}
