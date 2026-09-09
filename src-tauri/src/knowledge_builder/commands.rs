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

use crate::ai_services::storage::ConfigStorage as AiServicesConfigStorage;
use crate::llm_config::model::SceneModelRef;
use crate::llm_config::storage::ConfigStorage as LlmConfigStorage;
use crate::task_queue::state::TaskQueueState;
use crate::task_queue::store::TaskStore;
use crate::task_queue::types::{TaskKind, TaskRecord};

use super::error::KnowledgeBuilderError;
use super::kb_adapter::{self, MetaData, QueryResult, WikiEntry, WikiEntryDetail};
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
/// 创建 `references/wiki/` 下的目录与索引（`KbBuilder::open` 幂等：
/// 已存在的文件不会被覆盖）。
#[tauri::command]
pub async fn knowledge_init(project_path: String) -> Result<(), KnowledgeBuilderError> {
    let refs_dir = PathBuf::from(&project_path).join("references");
    spawn_blocking(move || {
        fluen_kb::KbBuilder::new(&refs_dir)
            .open()
            .map_err(KnowledgeBuilderError::from)
    })
    .await
    .map_err(|e| KnowledgeBuilderError::TaskQueue(format!("spawn_blocking join error: {e}")))??;
    Ok(())
}

/// 列出项目知识库中的所有条目（不含正文；关联带谓词）。
#[tauri::command]
pub async fn knowledge_list_entries(
    app: AppHandle,
    project_path: String,
) -> Result<Vec<WikiEntry>, KnowledgeBuilderError> {
    let kb = open_kb(&app, &project_path).await?;
    let metas = kb
        .list_entries(None)
        .await
        .map_err(KnowledgeBuilderError::from)?;
    Ok(metas.iter().map(kb_adapter::meta_to_entry).collect())
}

/// 获取单个条目详情（含正文（已剥离溯源标签）与关联（谓词 + 标题））。
#[tauri::command]
pub async fn knowledge_get_entry(
    app: AppHandle,
    project_path: String,
    wiki_id: String,
) -> Result<Option<WikiEntryDetail>, KnowledgeBuilderError> {
    let kb = open_kb(&app, &project_path).await?;
    let id = fluen_kb::WikiId::new(&wiki_id).map_err(KnowledgeBuilderError::from)?;
    let doc = match kb.get_entry(id).await {
        Ok(doc) => doc,
        Err(fluen_kb::KbError::NotFound(_)) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let titles = kb_adapter::title_map(&kb).await?;
    Ok(Some(kb_adapter::doc_to_detail(&doc, &titles)))
}

/// 检索知识库。
///
/// # 参数
///
/// - `query`：查询文本
/// - `wiki_type`：限定条目类型（`None` 不限；`summary` / `concept` / `entity`）
/// - `method`：检索方式（`None` 默认 hybrid；`keyword` / `semantic` / `hybrid`）
/// - `top_k`：返回条数上限（`None` 默认 10）
#[tauri::command]
pub async fn knowledge_query(
    app: AppHandle,
    project_path: String,
    query: String,
    wiki_type: Option<String>,
    method: Option<String>,
    top_k: Option<usize>,
) -> Result<QueryResult, KnowledgeBuilderError> {
    let kb = open_kb(&app, &project_path).await?;
    let method = kb_adapter::parse_method(method.as_deref().unwrap_or("hybrid"))?;
    let wiki_type = match wiki_type.as_deref() {
        None | Some("") => None,
        Some(s) => Some(kb_adapter::parse_wiki_type(s)?),
    };
    let params = fluen_kb::QueryParams {
        query,
        wiki_type,
        method,
        top_k: Some(top_k.unwrap_or(10)),
        include_content: false,
        expand: 0,
    };
    let hits = kb.query(params).await.map_err(KnowledgeBuilderError::from)?;
    Ok(QueryResult {
        success: true,
        results: hits.iter().map(kb_adapter::hit_to_match).collect(),
    })
}

/// 查询知识库元信息。
///
/// `query_type` 为 `overview`（仅统计）/ `recent`（附最近更新条目，
/// `limit` 对其生效）。新库无 tags 概念，`tags` 查询类型已随迁移移除。
#[tauri::command]
pub async fn knowledge_meta(
    app: AppHandle,
    project_path: String,
    query_type: String,
    limit: Option<usize>,
) -> Result<MetaData, KnowledgeBuilderError> {
    match query_type.as_str() {
        "overview" | "recent" => {}
        other => {
            return Err(KnowledgeBuilderError::Config(format!(
                "query_type 取值非法: {other:?}（可选 overview / recent）"
            )));
        }
    }
    let kb = open_kb(&app, &project_path).await?;
    kb_adapter::meta_result(&kb, &query_type, limit.unwrap_or(10)).await
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 打开项目的知识库句柄（不注入 embedding）。
///
/// 首次用新库打开旧项目时自动执行存量迁移（弹窗确认）。
/// 知识库目录为 `{project_path}/references/wiki/`。
/// 若未初始化（`index.db` 不存在），返回错误提示前端先调用 `knowledge_init`。
async fn open_kb(
    app: &AppHandle,
    project_path: &str,
) -> Result<fluen_kb::async_kb::AsyncKb, KnowledgeBuilderError> {
    let refs_dir = PathBuf::from(project_path).join("references");
    super::migration::ensure_migrated(app, &refs_dir).await?;
    kb_adapter::open_plain(&refs_dir)
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_kb_missing_fails_without_migration() {
        let tmp = tempfile::tempdir().unwrap();
        // open_plain（无迁移路径）在未初始化时必须报错
        let refs_dir = tmp.path().join("references");
        match kb_adapter::open_plain(&refs_dir) {
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
        fluen_kb::KbBuilder::new(&refs_dir).open().unwrap();

        let kb = kb_adapter::open_plain(&refs_dir).unwrap();
        let meta = kb_adapter::meta_result(&kb, "overview", 0).await.unwrap();
        assert_eq!(meta.total_entries, 0);
    }

    #[tokio::test]
    async fn meta_result_recent_returns_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let kb = fluen_kb::KbBuilder::new(tmp.path())
            .open()
            .unwrap()
            .into_async();
        kb.create(
            fluen_kb::WikiType::Concept,
            "元信息测试".into(),
            "正文".into(),
            vec![],
            None,
            vec![],
        )
        .await
        .unwrap();

        let data = kb_adapter::meta_result(&kb, "recent", 10).await.unwrap();
        assert_eq!(data.total_entries, 1);
        assert_eq!(data.recent_entries.len(), 1);
        assert_eq!(data.recent_entries[0].title, "元信息测试");
    }

    #[test]
    fn parse_wiki_type_and_method_validate() {
        assert!(kb_adapter::parse_wiki_type("concept").is_ok());
        assert!(kb_adapter::parse_wiki_type("bogus").is_err());
        assert!(kb_adapter::parse_method("hybrid").is_ok());
        assert!(kb_adapter::parse_method("bogus").is_err());
    }
}
