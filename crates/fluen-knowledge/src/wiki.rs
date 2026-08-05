use std::path::Path;

use rusqlite::Connection;

use crate::db;
use crate::error::{KnowledgeError, Result};
use crate::frontmatter::{self, WikiFrontMatter};
use crate::id;
use crate::index_md;
use crate::indexer;
use crate::markdown;
use crate::search;
use crate::types::*;
use crate::util::atomic_write;

/// 默认 embedding 模型名（写入 embeddings.model 列）。
const DEFAULT_EMBEDDING_MODEL: &str = "default";

/// index.md 的标题。
const INDEX_TITLE: &str = "知识库索引";

// ════════════════════════════════════════════════════════════════
// 初始化
// ════════════════════════════════════════════════════════════════

/// 初始化 wiki 知识库目录结构。
///
/// 创建以下结构（相对 `references/`）：
/// ```text
/// config.json          # 空占位
/// AGENTS.md            # 空占位
/// CLAUDE.md            # 空占位
/// wiki/
/// ├── index.md         # 空索引
/// ├── index.db         # SQLite（含 FTS5）
/// ├── concepts/
/// ├── entities/
/// └── summaries/
/// ```
///
/// 已存在的文件/目录不会被覆盖。
pub fn init_wiki(references_dir: &Path) -> Result<()> {
    let wiki_dir = references_dir.join("wiki");

    // 创建子目录
    std::fs::create_dir_all(wiki_dir.join("concepts"))?;
    std::fs::create_dir_all(wiki_dir.join("entities"))?;
    std::fs::create_dir_all(wiki_dir.join("summaries"))?;

    // 空占位文件（仅初始化，不覆盖）
    init_empty_file(&references_dir.join("config.json"), "{}")?;
    init_empty_file(&references_dir.join("AGENTS.md"), "")?;
    init_empty_file(&references_dir.join("CLAUDE.md"), "")?;

    // 初始化 SQLite（建表 + FTS5）
    {
        let conn = db::open_db(&wiki_dir)?;
        drop(conn);
    }

    // 创建空 index.md（如不存在）
    let index_path = wiki_dir.join("index.md");
    if !index_path.exists() {
        let now = chrono::Utc::now().to_rfc3339();
        let fm = index_md::IndexFrontMatter {
            title: INDEX_TITLE.to_string(),
            created: now.clone(),
            updated: now,
        };
        let content = index_md::build_index_md(&fm, &[], &[], &[], &[])?;
        index_md::write_index_md(&wiki_dir, &content)?;
    }

    tracing::info!(references_dir = %references_dir.display(), "wiki initialized");
    Ok(())
}

/// 打开 wiki 知识库的 SQLite 连接。
pub fn open_wiki(references_dir: &Path) -> Result<Connection> {
    let wiki_dir = references_dir.join("wiki");
    db::open_db(&wiki_dir)
}

fn init_empty_file(path: &Path, content: &str) -> Result<()> {
    if !path.exists() {
        std::fs::write(path, content)?;
    }
    Ok(())
}

// ════════════════════════════════════════════════════════════════
// 新建条目（工具三）
// ════════════════════════════════════════════════════════════════

/// 新建条目输入参数。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateEntryParams {
    pub wiki_type: WikiType,
    pub title: String,
    pub content: String,
    /// 仅 summaries：源文献路径（如 `raw/ref-xxx.pdf`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// 仅 summaries：作者 wikiID 列表。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    /// 标签名称列表（自动 upsert 为 tagID）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// 关联 wikiID 列表（写入 `## 关联页面` 区）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<String>,
}

/// 新建条目（同步）。
///
/// 流程：
/// 1. **去重检查**：summary 按 source 去重，concept/entity 按标题+类型去重。
///    若发现既有条目，合并 tags/relations 后返回（`merged=true`），不新建。
/// 2. 生成 wikiID
/// 3. 标签名称 → tagID（upsert）
/// 4. 写入 MD 文件（frontmatter + 正文 + 关联页面区）
/// 5. 写入 index.db（entries + entry_tags + entry_relations + FTS）
/// 6. 同步 index.md
///
/// **注意**：本函数不生成 embedding。调用方若需语义检索，
/// 应在调用本函数后，使用返回的 `wiki_id` 与 `embedding_text` 调用
/// [`store_embedding`] 写入向量。embedding 的异步计算由调用方负责。
pub fn create_entry(
    conn: &Connection,
    references_dir: &Path,
    params: CreateEntryParams,
) -> Result<CreateEntryResult> {
    db::transaction(conn, |conn| create_entry_impl(conn, references_dir, params))
}

fn create_entry_impl(
    conn: &Connection,
    references_dir: &Path,
    params: CreateEntryParams,
) -> Result<CreateEntryResult> {
    let wiki_dir = references_dir.join("wiki");

    // ── 去重检查 ──
    // summary 按 source 去重（同一文献不应产生两个综述页）
    // concept/entity 按标题（大小写不敏感）+ 类型去重（同一概念不应重复创建）
    let existing: Option<WikiEntry> = match params.wiki_type {
        WikiType::Summary => params
            .source
            .as_ref()
            .and_then(|s| db::find_by_source_exact(conn, s).ok().flatten())
            .filter(|e| e.wiki_type == WikiType::Summary),
        _ => db::find_by_title(conn, &params.title)
            .ok()
            .and_then(|entries| entries.into_iter().find(|e| e.wiki_type == params.wiki_type)),
    };

    if let Some(existing) = existing {
        return merge_into_existing(conn, &wiki_dir, existing, params);
    }

    // ── 新建条目 ──
    let wiki_id = id::generate_wiki_id();
    let now = chrono::Utc::now().to_rfc3339();

    // 标签名称 → tagID
    let tag_mappings: Vec<(String, String)> = if params.tags.is_empty() {
        Vec::new()
    } else {
        db::upsert_tags(conn, &params.tags)?
    };
    let tag_ids: Vec<String> = tag_mappings.iter().map(|(_, id)| id.clone()).collect();

    // 文件路径
    let filename = markdown::build_entry_filename(&wiki_id, &params.title);
    let file_path = format!("wiki/{}/{}", params.wiki_type.dir(), filename);
    let full_path = references_dir.join(&file_path);

    // 正文：先防御性剥离 AI 可能自行写入的关联区，再通过 params.relations 正规写入。
    // 关联关系只能由 params.relations → append_relations 写入，确保链接格式合规。
    let stripped = markdown::strip_relations_section(&params.content);
    let body = if params.relations.is_empty() {
        stripped
    } else {
        markdown::append_relations(&stripped, &params.relations)
    };

    // frontmatter
    let fm = WikiFrontMatter {
        title: params.title.clone(),
        wiki_type: params.wiki_type.as_str().to_string(),
        source: params.source.clone(),
        authors: params.authors.clone(),
        tags: tag_ids.clone(),
        created: now.clone(),
        updated: now.clone(),
    };

    // 写入 MD 文件
    let md_content = frontmatter::build_markdown(&fm, &body)?;
    atomic_write(&full_path, md_content.as_bytes())?;

    // 写入 DB
    let entry = WikiEntry {
        id: wiki_id.clone(),
        wiki_type: params.wiki_type,
        title: params.title.clone(),
        file_path: file_path.clone(),
        source: params.source,
        authors: params.authors,
        tags: tag_ids,
        relations: params.relations,
        content: body.clone(),
        created: now.clone(),
        updated: now,
    };
    db::insert_entry(conn, &entry)?;

    // 同步 index.md
    sync_index_md(&wiki_dir, conn)?;

    tracing::info!(wiki_id = %wiki_id, file_path = %file_path, "wiki entry created");

    Ok(CreateEntryResult {
        success: true,
        wiki_id,
        file_path,
        tags_generated: tag_mappings
            .into_iter()
            .map(|(name, tag_id)| TagMapping { name, tag_id })
            .collect(),
        embedding_text: format!("{}\n{}", params.title, body),
        merged: false,
    })
}

/// 将新参数合并到既有条目（去重路径）。
///
/// 合并 tags（upsert 名称 + 去重追加）和 relations（去重追加），
/// 更新 MD 文件的「## 关联页面」区与 DB 记录，同步 index.md。
/// 返回 `merged=true` 的 CreateEntryResult，调用方可据此区分新建与合并。
fn merge_into_existing(
    conn: &Connection,
    wiki_dir: &Path,
    mut existing: WikiEntry,
    params: CreateEntryParams,
) -> Result<CreateEntryResult> {
    // 1. 合并标签（upsert 名称 + 去重追加到 entry.tags）
    let mut new_tag_mappings: Vec<TagMapping> = Vec::new();
    if !params.tags.is_empty() {
        let mappings = db::upsert_tags(conn, &params.tags)?;
        for (name, tag_id) in &mappings {
            if !existing.tags.contains(tag_id) {
                existing.tags.push(tag_id.clone());
                new_tag_mappings.push(TagMapping {
                    name: name.clone(),
                    tag_id: tag_id.clone(),
                });
            }
        }
    }

    // 2. 合并关联（去重追加）
    let mut added_relations: Vec<String> = Vec::new();
    for r in &params.relations {
        if !existing.relations.contains(r) {
            existing.relations.push(r.clone());
            added_relations.push(r.clone());
        }
    }

    // 3. 读取现有 MD 文件，更新 frontmatter tags + 追加关联页面链接
    let now = chrono::Utc::now().to_rfc3339();
    let file_full_path = wiki_dir
        .parent()
        .unwrap_or(Path::new("."))
        .join(&existing.file_path);
    let mut body = String::new();
    if file_full_path.exists() {
        let raw = std::fs::read_to_string(&file_full_path)?;
        let (fm_str, body_str) = frontmatter::split_front_matter(&raw);
        body = body_str.to_string();

        // 追加新增关联页面链接到正文
        if !added_relations.is_empty() {
            body = markdown::append_relations(&body, &added_relations);
        }

        // 重写 MD 文件：更新 frontmatter tags（始终）+ 新正文（若有 relations 追加）
        // 即使仅添加 tags，也必须重写 frontmatter，否则 rebuild_full_index 从 MD
        // 重新解析时会丢失这些 tag（DB 与 MD 不一致）。
        if let Ok(mut fm) = frontmatter::parse_front_matter(fm_str) {
            fm.tags = existing.tags.clone();
            fm.updated = now.clone();
            let content = frontmatter::build_markdown(&fm, &body)?;
            atomic_write(&file_full_path, content.as_bytes())?;
        }
    }
    existing.content = body;

    // 4. 更新 DB
    existing.updated = now;
    db::update_entry(conn, &existing)?;

    // 5. 同步 index.md
    sync_index_md(wiki_dir, conn)?;

    tracing::info!(
        wiki_id = %existing.id,
        title = %existing.title,
        added_tags = new_tag_mappings.len(),
        added_relations = added_relations.len(),
        "merged duplicate entry instead of creating new"
    );

    Ok(CreateEntryResult {
        success: true,
        wiki_id: existing.id,
        file_path: existing.file_path,
        tags_generated: new_tag_mappings,
        embedding_text: format!("{}\n{}", existing.title, existing.content),
        merged: true,
    })
}

// ════════════════════════════════════════════════════════════════
// 修改条目（工具四）
// ════════════════════════════════════════════════════════════════

/// 修改条目输入参数。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EditEntryParams {
    pub wiki_id: String,
    /// search_replace / insert_after 操作列表。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edits: Vec<EditOp>,
    /// 追加到 `## 关联页面` 区的 wikiID 列表。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add_relations: Vec<String>,
    /// 追加的标签名称列表（自动 upsert 为 tagID）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add_tags: Vec<String>,
}

/// 修改条目（同步）。
///
/// 流程：
/// 1. 从 DB 读取条目
/// 2. 从 MD 文件读取正文
/// 3. 依次应用 edits（search_replace / insert_after）
/// 4. 追加 add_relations 到 `## 关联页面`
/// 5. 追加 add_tags 到 frontmatter tags
/// 6. 更新 MD 文件 + DB + index.md
///
/// **注意**：本函数不重新生成 embedding。调用方若需更新向量，
/// 应在调用本函数后，使用返回的 `wiki_id` 与 `embedding_text` 调用
/// [`store_embedding`] 写入向量。
pub fn edit_entry(
    conn: &Connection,
    references_dir: &Path,
    params: EditEntryParams,
) -> Result<EditEntryResult> {
    db::transaction(conn, |conn| edit_entry_impl(conn, references_dir, params))
}

fn edit_entry_impl(
    conn: &Connection,
    references_dir: &Path,
    params: EditEntryParams,
) -> Result<EditEntryResult> {
    let wiki_dir = references_dir.join("wiki");

    // 1. 读取现有条目
    let mut entry = db::get_entry(conn, &params.wiki_id)?
        .ok_or_else(|| KnowledgeError::NotFound(format!("entry not found: {}", params.wiki_id)))?;

    // 2. 读取 MD 文件
    let file_full_path = references_dir.join(&entry.file_path);
    let raw_content = std::fs::read_to_string(&file_full_path)?;
    let (fm_str, body) = frontmatter::split_front_matter(&raw_content);
    let mut fm = frontmatter::parse_front_matter(fm_str)?;

    // 3. 应用 edits
    let (new_body, edit_successes) = markdown::apply_edits(body, &params.edits);
    let mut new_body = new_body;

    // 4. 追加关联
    let mut edit_results: Vec<EditItemResult> = params
        .edits
        .iter()
        .zip(edit_successes.iter())
        .map(|(op, &success)| EditItemResult {
            edit_type: edit_op_type(op),
            success,
        })
        .collect();

    if !params.add_relations.is_empty() {
        new_body = markdown::append_relations(&new_body, &params.add_relations);
        // 合并到 entry.relations
        for r in &params.add_relations {
            if !entry.relations.contains(r) {
                entry.relations.push(r.clone());
            }
        }
        edit_results.push(EditItemResult {
            edit_type: "add_relations".to_string(),
            success: true,
        });
    }

    // 5. 追加标签
    if !params.add_tags.is_empty() {
        let new_mappings = db::upsert_tags(conn, &params.add_tags)?;
        for (_, tag_id) in &new_mappings {
            if !entry.tags.contains(tag_id) {
                entry.tags.push(tag_id.clone());
            }
        }
        edit_results.push(EditItemResult {
            edit_type: "add_tags".to_string(),
            success: true,
        });
    }

    // 6. 更新时间戳并写回
    let now = chrono::Utc::now().to_rfc3339();
    fm.updated = now.clone();
    fm.tags = entry.tags.clone();

    let md_content = frontmatter::build_markdown(&fm, &new_body)?;
    atomic_write(&file_full_path, md_content.as_bytes())?;

    entry.content = new_body.clone();
    entry.updated = now.clone();
    db::update_entry(conn, &entry)?;

    // 同步 index.md
    sync_index_md(&wiki_dir, conn)?;

    tracing::info!(wiki_id = %entry.id, edits = edit_results.len(), "wiki entry edited");

    Ok(EditEntryResult {
        success: true,
        wiki_id: entry.id,
        file_path: entry.file_path,
        updated_time: now,
        edit_results,
        embedding_text: format!("{}\n{}", entry.title, new_body),
    })
}

fn edit_op_type(op: &EditOp) -> String {
    match op {
        EditOp::SearchReplace { .. } => "search_replace".to_string(),
        EditOp::InsertAfter { .. } => "insert_after".to_string(),
    }
}

// ════════════════════════════════════════════════════════════════
// 查询（工具一、工具二）
// ════════════════════════════════════════════════════════════════

/// 单条查询参数。
pub struct QueryEntryParams<'a> {
    pub query: &'a str,
    pub wiki_type: Option<WikiType>,
    pub method: RetrievalMethod,
    pub top_k: usize,
    pub include_content: bool,
}

/// 单条查询（同步，工具一）。
///
/// `query_embedding` 为调用方预计算的查询向量（语义/混合检索时需要）。
/// embedding 的异步计算由调用方负责，避免 `&Connection` 跨 `.await`。
pub fn query(
    conn: &Connection,
    references_dir: &Path,
    params: QueryEntryParams<'_>,
    query_embedding: Option<&[f32]>,
) -> Result<QueryResult> {
    let wiki_dir = references_dir.join("wiki");
    let search_params = search::QueryParams {
        query: params.query,
        wiki_type: params.wiki_type,
        method: params.method,
        top_k: params.top_k,
        include_content: params.include_content,
    };

    let (method_used, matches) =
        search::search(conn, &wiki_dir, search_params, query_embedding)?;

    Ok(QueryResult {
        success: true,
        retrieval_method_used: method_used,
        results: matches,
    })
}

/// 批量查询参数。
pub struct BatchQueryParams {
    pub queries: Vec<String>,
    pub wiki_type: Option<WikiType>,
    pub method: RetrievalMethod,
    pub top_k: usize,
    pub include_content: bool,
}

/// 批量查询（同步，工具二）。
///
/// `query_embeddings` 为调用方预计算的查询向量列表，与 `params.queries` 一一对应。
/// 每个元素为 `Some(vec)` 表示该查询有预计算向量，`None` 表示该查询不需要语义检索。
pub fn query_batch(
    conn: &Connection,
    references_dir: &Path,
    params: BatchQueryParams,
    query_embeddings: Vec<Option<Vec<f32>>>,
) -> Result<BatchQueryResult> {
    let wiki_dir = references_dir.join("wiki");
    let mut results = Vec::with_capacity(params.queries.len());

    for (i, query) in params.queries.iter().enumerate() {
        let query_emb = query_embeddings.get(i).and_then(|e| e.as_deref());
        let search_params = search::QueryParams {
            query: query.as_str(),
            wiki_type: params.wiki_type,
            method: params.method,
            top_k: params.top_k,
            include_content: params.include_content,
        };

        let (method_used, matches) =
            search::search(conn, &wiki_dir, search_params, query_emb)?;

        results.push(BatchQueryItem {
            query: query.clone(),
            retrieval_method_used: method_used,
            matches,
        });
    }

    Ok(BatchQueryResult {
        success: true,
        results,
    })
}

// ════════════════════════════════════════════════════════════════
// 元信息（工具五）
// ════════════════════════════════════════════════════════════════

/// 元信息查询（工具五）。
///
/// - `Overview`：返回 total_entries / total_tags / embedding_enabled
/// - `Tags`：额外返回所有标签列表
/// - `Recent`：额外返回最近更新的条目（limit 控制数量）
pub fn meta(
    conn: &Connection,
    query_type: MetaQueryType,
    limit: usize,
) -> Result<MetaResult> {
    let total_entries = db::count_entries(conn)?;
    let total_tags = db::count_tags(conn)?;
    let embedding_enabled = db::has_embeddings(conn)?;

    let (tags, recent_entries) = match query_type {
        MetaQueryType::Overview => (None, None),
        MetaQueryType::Tags => {
            let tags = db::list_tags(conn)?;
            (Some(tags), None)
        }
        MetaQueryType::Recent => {
            let recent = db::recent_entries(conn, limit, None)?;
            let recent_entries: Vec<RecentEntry> = recent
                .into_iter()
                .map(|(id, title)| RecentEntry { id, title })
                .collect();
            (None, Some(recent_entries))
        }
    };

    Ok(MetaResult {
        success: true,
        data: MetaData {
            total_entries,
            total_tags,
            embedding_enabled,
            tags,
            recent_entries,
        },
    })
}

// ════════════════════════════════════════════════════════════════
// Embedding 存储（供调用方在异步计算后调用）
// ════════════════════════════════════════════════════════════════

/// 将预计算的 embedding 向量写入 DB（同步）。
///
/// 调用方应先异步计算 embedding（通过 `KnowledgeEmbedding::embed`），
/// 再调用本函数将向量持久化。这样避免了 `&Connection` 跨 `.await`。
pub fn store_embedding(
    conn: &Connection,
    wiki_id: &str,
    vector: &[f32],
) -> Result<()> {
    db::set_embedding(conn, wiki_id, vector, DEFAULT_EMBEDDING_MODEL)
}

// ════════════════════════════════════════════════════════════════
// 辅助 API（前端 / 管理端点用）
// ════════════════════════════════════════════════════════════════

/// 列出所有条目（不含正文）。
pub fn list_entries(conn: &Connection) -> Result<Vec<WikiEntry>> {
    db::list_entries(conn)
}

/// 获取单条条目详情（含正文、标签名、关联条目标题）。
pub fn get_entry_full(
    conn: &Connection,
    references_dir: &Path,
    wiki_id: &str,
) -> Result<Option<WikiEntryDetail>> {
    let Some(mut entry) = db::get_entry(conn, wiki_id)? else {
        return Ok(None);
    };
    let file_path = references_dir.join(&entry.file_path);
    if file_path.exists() {
        let content = std::fs::read_to_string(&file_path)?;
        let (_, body) = frontmatter::split_front_matter(&content);
        entry.content = body.to_string();
    }
    let tag_titles = db::get_tag_titles(conn, &entry.tags)?;
    let relation_titles = db::get_relation_titles(conn, &entry.relations)?;
    Ok(Some(WikiEntryDetail {
        entry,
        tag_titles,
        relation_titles,
    }))
}

/// 删除条目（MD 文件 + DB 记录 + index.md 同步）。
pub fn delete_entry(
    conn: &Connection,
    references_dir: &Path,
    wiki_id: &str,
) -> Result<()> {
    db::transaction(conn, |conn| {
        let wiki_dir = references_dir.join("wiki");
        indexer::delete_entry_full(conn, &wiki_dir, wiki_id)?;
        sync_index_md(&wiki_dir, conn)?;
        Ok(())
    })
}

/// 全量重建索引（扫描 wiki/ 下所有 MD 文件，重建 DB + index.md）。
///
/// **警告**：全量重建会清空 embeddings 表，重建后需重新计算向量。
pub fn rebuild_index(references_dir: &Path) -> Result<Connection> {
    let wiki_dir = references_dir.join("wiki");
    indexer::rebuild_full_index(&wiki_dir)
}

/// 清理已删除文献相关的 wiki 条目。
pub fn cleanup_for_deleted_reference(
    conn: &Connection,
    references_dir: &Path,
    reference_id: &str,
) -> Result<Vec<String>> {
    db::transaction(conn, |conn| {
        let wiki_dir = references_dir.join("wiki");
        let deleted = indexer::cleanup_for_deleted_reference(conn, &wiki_dir, reference_id)?;
        if !deleted.is_empty() {
            sync_index_md(&wiki_dir, conn)?;
        }
        Ok(deleted)
    })
}

// ════════════════════════════════════════════════════════════════
// 内部 helper
// ════════════════════════════════════════════════════════════════

/// 从 DB 数据重建 index.md。
fn sync_index_md(wiki_dir: &Path, conn: &Connection) -> Result<()> {
    let entries = db::list_entries(conn)?;
    let tags = db::list_tags(conn)?;
    index_md::rebuild_index_md(wiki_dir, INDEX_TITLE, &entries, &tags)?;
    Ok(())
}
