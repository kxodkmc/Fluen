//! 异步知识库句柄。
//!
//! 在同步 [`crate::wiki`] API 之上提供异步访问层，核心策略：
//!
//! - `rusqlite::Connection` 是 `Send` 但非 `Sync`，通过 `Arc<std::sync::Mutex<Connection>>`
//!   实现跨异步任务安全共享。
//! - 所有 DB 操作通过 `tokio::task::spawn_blocking` 在阻塞线程池执行，
//!   避免阻塞异步运行时。`std::sync::Mutex` 仅在阻塞线程内持有，不影响异步调度。
//! - embedding 计算通过注入的 [`KnowledgeEmbedding`] provider 异步执行，
//!   计算完成后再通过 `spawn_blocking` 写入 DB。
//!
//! # 示例
//!
//! ```no_run
//! # use fluen_knowledge::async_kb::{AsyncKnowledgeBase, AsyncQueryParams};
//! # use fluen_knowledge::types::RetrievalMethod;
//! use std::path::Path;
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let kb = AsyncKnowledgeBase::open("references")?;
//!
//! let result = kb.query(AsyncQueryParams {
//!     query: "machine learning".into(),
//!     method: RetrievalMethod::Keyword,
//!     ..Default::default()
//! }).await?;
//!
//! println!("found {} entries", result.results.len());
//! # Ok(())
//! # }
//! ```

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db;
use crate::error::{KnowledgeError, Result};
use crate::types::*;
use crate::wiki;

// ════════════════════════════════════════════════════════════════
// Owned 参数类型
// ════════════════════════════════════════════════════════════════

/// 异步单条查询参数（owned 版本，可跨 `spawn_blocking` 传递）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsyncQueryParams {
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wiki_type: Option<WikiType>,
    #[serde(default = "default_method")]
    pub method: RetrievalMethod,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    #[serde(default)]
    pub include_content: bool,
}

impl Default for AsyncQueryParams {
    fn default() -> Self {
        Self {
            query: String::new(),
            wiki_type: None,
            method: default_method(),
            top_k: default_top_k(),
            include_content: false,
        }
    }
}

/// 异步批量查询参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsyncBatchQueryParams {
    pub queries: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wiki_type: Option<WikiType>,
    #[serde(default = "default_method")]
    pub method: RetrievalMethod,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    #[serde(default)]
    pub include_content: bool,
}

fn default_method() -> RetrievalMethod {
    RetrievalMethod::Hybrid
}

fn default_top_k() -> usize {
    10
}

// ════════════════════════════════════════════════════════════════
// AsyncKnowledgeBase
// ════════════════════════════════════════════════════════════════

/// 异步知识库句柄。
///
/// 线程安全、可克隆（`Arc` 内部引用计数）。
/// 所有方法均为 `async`，底层通过 `spawn_blocking` 调用同步 wiki API。
pub struct AsyncKnowledgeBase {
    conn: Arc<Mutex<Connection>>,
    references_dir: PathBuf,
    embedding_provider: Option<Arc<dyn KnowledgeEmbedding>>,
}

// 编译期断言：AsyncKnowledgeBase 是 Send + Sync。
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<AsyncKnowledgeBase>();
};

impl AsyncKnowledgeBase {
    /// 打开已有知识库（需先调用 [`wiki::init_wiki`] 初始化）。
    pub fn open(references_dir: impl Into<PathBuf>) -> Result<Self> {
        let references_dir = references_dir.into();
        let conn = wiki::open_wiki(&references_dir)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            references_dir,
            embedding_provider: None,
        })
    }

    /// 初始化并打开知识库（若目录结构不存在则创建）。
    pub fn init(references_dir: impl Into<PathBuf>) -> Result<Self> {
        let references_dir = references_dir.into();
        wiki::init_wiki(&references_dir)?;
        Self::open(references_dir)
    }

    /// 注入 embedding provider（链式调用）。
    ///
    /// 注入后，`query`/`query_batch` 在 semantic/hybrid 模式下自动计算查询向量，
    /// `create_entry`/`edit_entry` 后自动计算并存储条目向量。
    pub fn with_embedding_provider(mut self, provider: Arc<dyn KnowledgeEmbedding>) -> Self {
        self.embedding_provider = Some(provider);
        self
    }

    /// 获取 references_dir 路径引用。
    pub fn references_dir(&self) -> &std::path::Path {
        &self.references_dir
    }

    /// 是否已注入 embedding provider。
    pub fn has_embedding_provider(&self) -> bool {
        self.embedding_provider.is_some()
    }

    // ── 内部辅助：在阻塞线程中执行 DB 操作 ──

    /// 在 `spawn_blocking` 中执行闭包，自动加锁 Connection。
    async fn spawn_db<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection, &std::path::Path) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.conn.clone();
        let refs_dir = self.references_dir.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("KnowledgeBase mutex poisoned");
            f(&conn, &refs_dir)
        })
        .await
        .map_err(|e| KnowledgeError::AsyncRuntime(format!("spawn_blocking join error: {e}")))?
    }

    /// 异步计算单条文本的 embedding。
    async fn compute_embedding(&self, text: &str) -> Result<Vec<f32>> {
        let provider = self
            .embedding_provider
            .as_ref()
            .ok_or_else(|| KnowledgeError::Embedding("no embedding provider".into()))?;
        let embeddings = provider
            .embed(vec![text.to_string()])
            .await
            .map_err(|e| KnowledgeError::Embedding(e.to_string()))?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| KnowledgeError::Embedding("embedding provider returned empty".into()))
    }

    // ════════════════════════════════════════════════════════════════
    // 工具一：单条查询
    // ════════════════════════════════════════════════════════════════

    /// 异步单条查询。
    ///
    /// 若注入了 embedding provider 且 method 为 semantic/hybrid，
    /// 自动计算查询向量；否则降级为 keyword。
    pub async fn query(&self, params: AsyncQueryParams) -> Result<QueryResult> {
        let query_embedding = if matches!(
            params.method,
            RetrievalMethod::Semantic | RetrievalMethod::Hybrid
        ) && self.embedding_provider.is_some()
        {
            self.compute_embedding(&params.query).await.ok()
        } else {
            None
        };

        self.spawn_db(move |conn, refs_dir| {
            wiki::query(
                conn,
                refs_dir,
                wiki::QueryEntryParams {
                    query: &params.query,
                    wiki_type: params.wiki_type,
                    method: params.method,
                    top_k: params.top_k,
                    include_content: params.include_content,
                },
                query_embedding.as_deref(),
            )
        })
        .await
    }

    // ════════════════════════════════════════════════════════════════
    // 工具二：批量查询
    // ════════════════════════════════════════════════════════════════

    /// 异步批量查询。
    pub async fn query_batch(&self, params: AsyncBatchQueryParams) -> Result<BatchQueryResult> {
        // 如需 embedding，批量计算
        let query_embeddings: Vec<Option<Vec<f32>>> =
            if matches!(params.method, RetrievalMethod::Semantic | RetrievalMethod::Hybrid)
                && self.embedding_provider.is_some()
            {
                let provider = self.embedding_provider.as_ref().unwrap();
                let texts: Vec<String> = params.queries.clone();
                let all_embeddings = provider
                    .embed(texts)
                    .await
                    .map_err(|e| KnowledgeError::Embedding(e.to_string()))?;
                all_embeddings.into_iter().map(Some).collect()
            } else {
                params.queries.iter().map(|_| None).collect()
            };

        self.spawn_db(move |conn, refs_dir| {
            wiki::query_batch(
                conn,
                refs_dir,
                wiki::BatchQueryParams {
                    queries: params.queries,
                    wiki_type: params.wiki_type,
                    method: params.method,
                    top_k: params.top_k,
                    include_content: params.include_content,
                },
                query_embeddings,
            )
        })
        .await
    }

    // ════════════════════════════════════════════════════════════════
    // 工具三：新建条目
    // ════════════════════════════════════════════════════════════════

    /// 异步新建条目。
    ///
    /// 若注入了 embedding provider，创建后自动计算并存储条目向量。
    pub async fn create_entry(&self, params: wiki::CreateEntryParams) -> Result<CreateEntryResult> {
        let result = self
            .spawn_db(move |conn, refs_dir| wiki::create_entry(conn, refs_dir, params))
            .await?;

        // 异步计算并存储 embedding
        if let Some(_) = &self.embedding_provider {
            if !result.embedding_text.is_empty() {
                self.store_embedding_for(&result.wiki_id, &result.embedding_text)
                    .await
                    .ok(); // embedding 失败不阻塞主流程
            }
        }

        Ok(result)
    }

    // ════════════════════════════════════════════════════════════════
    // 工具四：修改条目
    // ════════════════════════════════════════════════════════════════

    /// 异步修改条目。
    ///
    /// 若注入了 embedding provider，修改后自动重新计算并存储条目向量。
    pub async fn edit_entry(&self, params: wiki::EditEntryParams) -> Result<EditEntryResult> {
        let result = self
            .spawn_db(move |conn, refs_dir| wiki::edit_entry(conn, refs_dir, params))
            .await?;

        if self.embedding_provider.is_some() && !result.embedding_text.is_empty() {
            self.store_embedding_for(&result.wiki_id, &result.embedding_text)
                .await
                .ok();
        }

        Ok(result)
    }

    // ════════════════════════════════════════════════════════════════
    // 工具五：元信息
    // ════════════════════════════════════════════════════════════════

    /// 异步查询元信息。
    pub async fn meta(&self, query_type: MetaQueryType, limit: usize) -> Result<MetaResult> {
        self.spawn_db(move |conn, _| wiki::meta(conn, query_type, limit))
            .await
    }

    // ════════════════════════════════════════════════════════════════
    // 辅助 API
    // ════════════════════════════════════════════════════════════════

    /// 异步获取单条条目详情（含正文、标签名、关联条目标题）。
    pub async fn get_entry(&self, wiki_id: String) -> Result<Option<WikiEntryDetail>> {
        self.spawn_db(move |conn, refs_dir| wiki::get_entry_full(conn, refs_dir, &wiki_id))
            .await
    }

    /// 异步列出所有条目（不含正文）。
    pub async fn list_entries(&self) -> Result<Vec<WikiEntry>> {
        self.spawn_db(move |conn, _| wiki::list_entries(conn)).await
    }

    /// 异步按 source 字段精确查询条目。
    ///
    /// source 字段（如 `raw/ref-xxxx.pdf`）未被 FTS5 索引，也无法通过 keyword
    /// 检索命中。此方法走 `WHERE source = ?` 精确匹配，供 pipeline 在创建
    /// summary 后回查条目使用。
    pub async fn find_by_source(&self, source: String) -> Result<Option<WikiEntry>> {
        self.spawn_db(move |conn, _| db::find_by_source_exact(conn, &source))
            .await
    }

    /// 异步删除条目。
    pub async fn delete_entry(&self, wiki_id: String) -> Result<()> {
        self.spawn_db(move |conn, refs_dir| wiki::delete_entry(conn, refs_dir, &wiki_id))
            .await
    }

    /// 异步全量重建索引。
    ///
    /// **警告**：重建会清空 embeddings 表，重建后需重新计算向量。
    /// 重建完成后，内部 Connection 会被替换为新的连接。
    pub async fn rebuild_index(&self) -> Result<()> {
        let refs_dir = self.references_dir.clone();
        let new_conn = tokio::task::spawn_blocking(move || wiki::rebuild_index(&refs_dir))
            .await
            .map_err(|e| KnowledgeError::AsyncRuntime(format!("spawn_blocking join error: {e}")))??;

        // 替换内部连接
        let mut guard = self.conn.lock().expect("KnowledgeBase mutex poisoned");
        *guard = new_conn;
        Ok(())
    }

    // ════════════════════════════════════════════════════════════════
    // 内部：embedding 存储辅助
    // ════════════════════════════════════════════════════════════════

    /// 计算文本 embedding 并写入 DB。
    async fn store_embedding_for(&self, wiki_id: &str, text: &str) -> Result<()> {
        let vector = self.compute_embedding(text).await?;
        let wiki_id = wiki_id.to_string();
        self.spawn_db(move |conn, _| wiki::store_embedding(conn, &wiki_id, &vector))
            .await
    }
}

impl Clone for AsyncKnowledgeBase {
    fn clone(&self) -> Self {
        Self {
            conn: self.conn.clone(),
            references_dir: self.references_dir.clone(),
            embedding_provider: self.embedding_provider.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_async_end_to_end() {
        let dir = tempfile::TempDir::new().unwrap();
        let kb = AsyncKnowledgeBase::init(dir.path()).unwrap();

        // 创建条目
        let create_result = kb
            .create_entry(wiki::CreateEntryParams {
                wiki_type: WikiType::Concept,
                title: "Async Test Concept".into(),
                content: "This is a test for async knowledge base.".into(),
                source: None,
                authors: vec![],
                tags: vec!["testing".into()],
                relations: vec![],
            })
            .await
            .unwrap();

        assert!(create_result.success);
        assert!(create_result.wiki_id.starts_with("wiki-"));

        // 查询
        let query_result = kb
            .query(AsyncQueryParams {
                query: "async".into(),
                method: RetrievalMethod::Keyword,
                top_k: 10,
                ..Default::default()
            })
            .await
            .unwrap();

        assert!(query_result.success);
        assert!(!query_result.results.is_empty());

        // 元信息
        let meta = kb.meta(MetaQueryType::Overview, 0).await.unwrap();
        assert!(meta.success);
        assert_eq!(meta.data.total_entries, 1);
    }

    #[tokio::test]
    async fn test_async_concurrent_queries() {
        let dir = tempfile::TempDir::new().unwrap();
        let kb = AsyncKnowledgeBase::init(dir.path()).unwrap();

        // 创建多个条目
        for i in 0..5 {
            kb.create_entry(wiki::CreateEntryParams {
                wiki_type: WikiType::Concept,
                title: format!("Concurrent Concept {i}"),
                content: format!("Content for concept number {i} about concurrency."),
                source: None,
                authors: vec![],
                tags: vec![],
                relations: vec![],
            })
            .await
            .unwrap();
        }

        // 并发查询
        let kb_clone = kb.clone();
        let h1 = tokio::spawn(async move {
            kb_clone
                .query(AsyncQueryParams {
                    query: "concurrency".into(),
                    method: RetrievalMethod::Keyword,
                    ..Default::default()
                })
                .await
        });

        let kb_clone2 = kb.clone();
        let h2 = tokio::spawn(async move {
            kb_clone2
                .meta(MetaQueryType::Overview, 0)
                .await
        });

        let r1 = h1.await.unwrap().unwrap();
        let r2 = h2.await.unwrap().unwrap();

        assert!(!r1.results.is_empty());
        assert_eq!(r2.data.total_entries, 5);
    }
}
