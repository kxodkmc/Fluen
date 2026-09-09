//! fluen-kb 适配层（KB 迁移）。
//!
//! 宿主从旧 `fluen-knowledge` 切换到新 SDK `fluen-kb` 的统一收口：
//!
//! - **统一打开入口**：`KbBuilder::open` + embedding 路由器注入，含旧版
//!   schema 检测（旧库需先走 M3 迁移，见 [`is_legacy_schema`] 与
//!   `migration` 模块）。
//! - **前端契约类型**（M4 起为本层定义的 serde 结构，不再借用旧库类型）：
//!   [`WikiEntry`] / [`WikiEntryDetail`] / [`QueryResult`] / [`MetaData`]。
//!   关联携带谓词（[`RelationRef::predicate`] / [`RelationInfo::predicate`]），
//!   正文出 SDK 一律先 `strip` 溯源标签。
//!
//! AI 工具面走进程内 MCP 桥接（`knowledge_mcp_bridge`），本层不提供工具实现。

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use fluen_kb::async_kb::AsyncKb;
use fluen_kb::ids::{SourceId, WikiId, WikiType};
use fluen_kb::search::{SearchHit, SearchMethod};

use crate::builtin_providers::embedding::build_embedding_router;
use crate::llm_config::model::LlmConfig;

use super::error::KnowledgeBuilderError;

/// 正文输出截断上限（与旧 `KnowledgeConfig::max_content_length` 默认一致）。
const MAX_CONTENT_LENGTH: usize = 4096;

// ---------------------------------------------------------------------------
// 前端契约类型（serde）
// ---------------------------------------------------------------------------

/// 关联引用（列表视图：谓词 + 目标 wikiID）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RelationRef {
    /// 关联谓词（如 `related` / `作者` / `应用了`）。
    pub predicate: String,
    /// 目标条目 wikiID。
    pub id: String,
}

/// 关联条目（详情视图：谓词 + wikiID + 标题）。
#[derive(Debug, Clone, Serialize)]
pub struct RelationInfo {
    /// 关联谓词。
    pub predicate: String,
    /// 目标条目 wikiID。
    pub id: String,
    /// 目标条目标题（缺失时回退为 ID）。
    pub title: String,
}

/// 知识库条目（列表视图，不含正文）。
#[derive(Debug, Clone, Serialize)]
pub struct WikiEntry {
    pub id: String,
    /// 条目类型（`summary` / `concept` / `entity`）。
    pub wiki_type: String,
    pub title: String,
    pub file_path: String,
    /// 仅 summaries：源文献 refID（`ref-xxx`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// 出向关联（带谓词）。
    pub relations: Vec<RelationRef>,
    /// 创建时间（RFC3339）。
    pub created: String,
    /// 更新时间（RFC3339）。
    pub updated: String,
}

/// 知识库条目详情（含正文与关联标题）。
#[derive(Debug, Clone, Serialize)]
pub struct WikiEntryDetail {
    pub id: String,
    pub wiki_type: String,
    pub title: String,
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// 出向关联（谓词 + wikiID + 标题；标题缺失时回退为 ID）。
    pub relations: Vec<RelationInfo>,
    /// 正文（已剥离 `<ref-xxx>` 溯源标签，按字符截断）。
    #[serde(skip_serializing_if = "String::is_empty")]
    pub content: String,
    pub created: String,
    pub updated: String,
}

/// 单条检索结果。
#[derive(Debug, Clone, Serialize)]
pub struct QueryMatch {
    pub wiki_id: String,
    pub wiki_type: String,
    pub title: String,
    pub file_path: String,
    pub score: f64,
    /// 正文（仅 include_content=true；已剥离溯源标签并截断）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// 检索结果。
#[derive(Debug, Clone, Serialize)]
pub struct QueryResult {
    pub success: bool,
    pub results: Vec<QueryMatch>,
}

/// 近期更新条目（仅含 id 与标题）。
#[derive(Debug, Clone, Serialize)]
pub struct RecentEntry {
    pub id: String,
    pub title: String,
}

/// 元信息数据体（新库无 tags 概念）。
#[derive(Debug, Clone, Default, Serialize)]
pub struct MetaData {
    pub total_entries: u32,
    /// 仅 `query_type=recent` 时返回。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_entries: Vec<RecentEntry>,
}

// ---------------------------------------------------------------------------
// 参数解析
// ---------------------------------------------------------------------------

/// 解析前端传入的条目类型字符串（`summary` / `concept` / `entity`）。
pub fn parse_wiki_type(value: &str) -> Result<WikiType, KnowledgeBuilderError> {
    WikiType::parse(value)
        .ok_or_else(|| KnowledgeBuilderError::Config(format!("wiki_type 取值非法: {value:?}（可选 summary / concept / entity）")))
}

/// 解析前端传入的检索方式字符串（默认 hybrid）。
pub fn parse_method(value: &str) -> Result<SearchMethod, KnowledgeBuilderError> {
    match value {
        "keyword" => Ok(SearchMethod::Keyword),
        "semantic" => Ok(SearchMethod::Semantic),
        "hybrid" => Ok(SearchMethod::Hybrid),
        other => Err(KnowledgeBuilderError::Config(format!(
            "method 取值非法: {other:?}（可选 keyword / semantic / hybrid）"
        ))),
    }
}

// ---------------------------------------------------------------------------
// 打开入口
// ---------------------------------------------------------------------------

/// 打开项目知识库（同步句柄，含 embedding 路由器注入）。
///
/// `index.db` 不存在 → `Config` 错误（提示先调用 `knowledge_init`）；
/// 旧版 v1 schema → `Config` 错误（提示先迁移；供 agent 工具静默降级）。
pub fn open_kb(refs_dir: &Path, llm: &LlmConfig) -> Result<fluen_kb::handle::Kb, KnowledgeBuilderError> {
    let kb = open_handle(refs_dir, false)?;
    attach_router(&kb, llm);
    Ok(kb)
}

/// 打开（不存在则初始化）项目知识库（同步句柄，含 embedding 路由器注入）。
///
/// 与旧 `AsyncKnowledgeBase::init` 语义一致，供构建任务（runner）使用。
pub fn init_kb(refs_dir: &Path, llm: &LlmConfig) -> Result<fluen_kb::handle::Kb, KnowledgeBuilderError> {
    let kb = open_handle(refs_dir, true)?;
    attach_router(&kb, llm);
    Ok(kb)
}

/// 打开（不存在则初始化）项目知识库（异步句柄，含 embedding）。
///
/// 供 pipeline 直调（L2 检索 / 关联写入）；AI 工具面走 MCP 桥接。
pub fn init_kb_async(refs_dir: &Path, llm: &LlmConfig) -> Result<AsyncKb, KnowledgeBuilderError> {
    Ok(init_kb(refs_dir, llm)?.into_async())
}

/// 打开项目知识库（不注入 embedding，语义检索降级为关键词）。
///
/// 供前端知识库访问命令使用。
pub fn open_plain(refs_dir: &Path) -> Result<AsyncKb, KnowledgeBuilderError> {
    Ok(open_handle(refs_dir, false)?.into_async())
}

fn attach_router(kb: &fluen_kb::handle::Kb, llm: &LlmConfig) {
    if let Some(router) = build_embedding_router(llm) {
        if let Err(e) = kb.embed().attach(std::sync::Arc::new(router)) {
            tracing::warn!("embedding 注入失败（语义检索将降级为关键词）: {e}");
        }
    }
}

/// 打开同步句柄（含未初始化检测与旧版 schema 拦截）。
fn open_handle(refs_dir: &Path, create_if_missing: bool) -> Result<fluen_kb::handle::Kb, KnowledgeBuilderError> {
    let wiki_db = refs_dir.join("wiki").join("index.db");
    if wiki_db.exists() {
        if is_legacy_schema(&wiki_db)? {
            return Err(KnowledgeBuilderError::Config(
                "检测到旧版知识库（schema v1），请先执行数据迁移".into(),
            ));
        }
    } else if !create_if_missing {
        return Err(KnowledgeBuilderError::Config(format!(
            "知识库未初始化：{} 不存在，请先调用 knowledge_init",
            wiki_db.display()
        )));
    }

    fluen_kb::KbBuilder::new(refs_dir)
        .open()
        .map_err(KnowledgeBuilderError::from)
}

/// 检测旧版 v1 schema：`user_version == 0` 且存在旧特征表（tags / entry_tags）。
///
/// 背景：旧库从不写 user_version，与"全新空库"无法通过 user_version 区分，
/// 而新库会把 user_version=0 的库静默标记为 v2（随后运行时因缺列报错），
/// 因此由宿主提前识别并拦截。`false` 还包括"文件存在但无任何表"的空库
/// （交由 SDK 正常 init）。
pub fn is_legacy_schema(wiki_db: &Path) -> Result<bool, KnowledgeBuilderError> {
    let conn = rusqlite::Connection::open(wiki_db)
        .map_err(|e| KnowledgeBuilderError::Config(format!("打开 index.db 失败: {e}")))?;
    let version: i32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| KnowledgeBuilderError::Config(format!("读取 user_version 失败: {e}")))?;
    if version != 0 {
        return Ok(false);
    }
    let legacy_tables: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('tags','entry_tags')",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    Ok(legacy_tables > 0)
}

// ---------------------------------------------------------------------------
// 类型转换：fluen-kb → 前端契约
// ---------------------------------------------------------------------------

/// 剥离正文中的 `<ref-xxx>` 溯源标签（所有出 SDK 的正文统一过此函数）。
pub fn strip_body(body: &str) -> String {
    let (plain, _) = fluen_kb::syntax::strip(body);
    plain
}

/// 按字符截断（与旧 `KnowledgeConfig::truncate_content` 一致）。
pub fn truncate_content(content: &str) -> String {
    if content.chars().count() <= MAX_CONTENT_LENGTH {
        content.to_string()
    } else {
        let truncated: String = content.chars().take(MAX_CONTENT_LENGTH).collect();
        format!("{truncated}…(truncated)")
    }
}

/// 条目相对路径（相对 references/，如 `wiki/concepts/wiki-xxx-标题.md`）。
pub fn rel_file_path(wiki_type: WikiType, id: &WikiId, title: &str) -> String {
    let filename = fluen_kb::store::filename::entry_filename(id, title);
    format!("wiki/{}/{}", wiki_type.dir(), filename)
}

fn type_str(t: WikiType) -> String {
    t.as_str().to_string()
}

fn relation_refs(meta_relations: &[(fluen_kb::ids::Predicate, WikiId)]) -> Vec<RelationRef> {
    meta_relations
        .iter()
        .map(|(p, to)| RelationRef {
            predicate: p.as_str().to_string(),
            id: to.as_str().to_string(),
        })
        .collect()
}

/// [`fluen_kb::EntryMeta`] → [`WikiEntry`]（不含正文）。
pub fn meta_to_entry(meta: &fluen_kb::EntryMeta) -> WikiEntry {
    WikiEntry {
        id: meta.id.as_str().to_string(),
        wiki_type: type_str(meta.wiki_type),
        title: meta.title.clone(),
        file_path: meta.file_path.clone(),
        source: meta.sources.first().map(|s| s.as_str().to_string()),
        relations: relation_refs(&meta.relations),
        created: meta.created.clone(),
        updated: meta.updated.clone(),
    }
}

/// [`fluen_kb::SearchHit`] → [`QueryMatch`]（正文 strip + 截断）。
pub fn hit_to_match(hit: &SearchHit) -> QueryMatch {
    QueryMatch {
        wiki_id: hit.meta.id.as_str().to_string(),
        wiki_type: type_str(hit.meta.wiki_type),
        title: hit.meta.title.clone(),
        file_path: hit.meta.file_path.clone(),
        score: hit.score,
        content: hit
            .content
            .as_ref()
            .map(|c| truncate_content(&strip_body(c))),
    }
}

/// [`fluen_kb::EntryDocument`] → [`WikiEntryDetail`]。
///
/// `titles`：wikiID → 标题映射（供关联标题），由调用方从 `list_entries`
/// 聚合；缺失时回退为 ID。
pub fn doc_to_detail(
    doc: &fluen_kb::EntryDocument,
    titles: &HashMap<String, String>,
) -> WikiEntryDetail {
    let sources = fluen_kb::syntax::collect_sources(&doc.body);
    let relations = doc
        .relations
        .iter()
        .map(|(p, to)| RelationInfo {
            predicate: p.as_str().to_string(),
            id: to.as_str().to_string(),
            title: titles
                .get(to.as_str())
                .cloned()
                .unwrap_or_else(|| to.as_str().to_string()),
        })
        .collect();
    WikiEntryDetail {
        id: doc.id.as_str().to_string(),
        wiki_type: type_str(doc.wiki_type),
        title: doc.title.clone(),
        file_path: rel_file_path(doc.wiki_type, &doc.id, &doc.title),
        source: sources.first().map(|s| s.as_str().to_string()),
        relations,
        content: truncate_content(&strip_body(&doc.body)),
        created: doc.created.clone(),
        updated: doc.updated.clone(),
    }
}

/// 聚合 wikiID → 标题映射（供关联标题等展示字段）。
pub async fn title_map(kb: &AsyncKb) -> Result<HashMap<String, String>, KnowledgeBuilderError> {
    let metas = kb.list_entries(None).await?;
    Ok(metas
        .into_iter()
        .map(|m| (m.id.as_str().to_string(), m.title))
        .collect())
}

/// 元信息聚合：以 `list_entries` 派生 [`MetaData`]。
///
/// `query_type`：`overview`（仅统计）/ `recent`（附最近更新条目）。
/// 新库无 tags 概念；`tags` 查询类型已随迁移移除。
pub async fn meta_result(
    kb: &AsyncKb,
    query_type: &str,
    limit: usize,
) -> Result<MetaData, KnowledgeBuilderError> {
    let metas = kb.list_entries(None).await?;
    let mut data = MetaData {
        total_entries: metas.len() as u32,
        recent_entries: Vec::new(),
    };
    if query_type == "recent" {
        let mut sorted = metas.clone();
        sorted.sort_by(|a, b| b.updated.cmp(&a.updated));
        data.recent_entries = sorted
            .into_iter()
            .take(limit)
            .map(|m| RecentEntry {
                id: m.id.as_str().to_string(),
                title: m.title,
            })
            .collect();
    }
    Ok(data)
}
