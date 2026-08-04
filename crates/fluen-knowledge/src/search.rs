use std::path::Path;

use crate::error::Result;

use crate::db;
use crate::id;
use crate::types::{
    QueryMatch, RetrievalMethod, RetrievalMethodUsed, WikiEntry, WikiType,
};

/// 单条查询的输入参数。
pub struct QueryParams<'a> {
    pub query: &'a str,
    pub wiki_type: Option<WikiType>,
    pub method: RetrievalMethod,
    pub top_k: usize,
    pub include_content: bool,
}

/// 执行单条知识库查询（同步）。
///
/// 根据 query 内容和方法自动选择检索路径：
/// 1. ID 直查（wiki- / ref- / tag- 前缀，或名称精确匹配）
/// 2. keyword（FTS5）
/// 3. semantic（embedding 余弦相似度）
/// 4. hybrid（keyword×0.7 + semantic×0.3）
///
/// embedding 不可用时，semantic/hybrid 自动降级为 keyword。
///
/// `query_embedding` 为调用方预计算的查询向量（语义/混合检索时需要）。
/// 若 method 需要 embedding 但 query_embedding 为 None，自动降级为 keyword。
pub fn search(
    conn: &rusqlite::Connection,
    wiki_dir: &Path,
    params: QueryParams<'_>,
    query_embedding: Option<&[f32]>,
) -> Result<(RetrievalMethodUsed, Vec<QueryMatch>)> {
    // 1. ID 直查
    if let Some((method_used, matches)) = try_id_lookup(conn, wiki_dir, &params)? {
        return Ok((method_used, matches));
    }

    // 2. 判断 embedding 是否可用
    let embedding_available = query_embedding.is_some() && db::has_embeddings(conn)?;

    // 3. 根据方法选择检索路径
    let (method_used, raw_results) = match params.method {
        RetrievalMethod::Keyword => {
            let results = keyword_search(conn, &params)?;
            (RetrievalMethodUsed::Keyword, results)
        }
        RetrievalMethod::Semantic => {
            if embedding_available {
                let results = semantic_search(conn, &params, query_embedding.unwrap())?;
                (RetrievalMethodUsed::Semantic, results)
            } else {
                tracing::warn!("semantic search requested but embedding unavailable, degrading to keyword");
                let results = keyword_search(conn, &params)?;
                (RetrievalMethodUsed::Keyword, results)
            }
        }
        RetrievalMethod::Hybrid => {
            if embedding_available {
                let results = hybrid_search(conn, &params, query_embedding.unwrap())?;
                (RetrievalMethodUsed::Hybrid, results)
            } else {
                tracing::warn!("hybrid search requested but embedding unavailable, degrading to keyword");
                let results = keyword_search(conn, &params)?;
                (RetrievalMethodUsed::Keyword, results)
            }
        }
    };

    // 4. 填充 content（如需要）
    let matches = fill_matches(conn, wiki_dir, raw_results, params.include_content)?;

    Ok((method_used, matches))
}

/// 尝试 ID 直查。返回 Some 表示命中 ID 查询路径。
fn try_id_lookup(
    conn: &rusqlite::Connection,
    wiki_dir: &Path,
    params: &QueryParams<'_>,
) -> Result<Option<(RetrievalMethodUsed, Vec<QueryMatch>)>> {
    let query = params.query.trim();

    // wiki-xxx 直查
    if id::is_wiki_id(query) {
        if let Some(entry) = db::get_entry(conn, query)? {
            let content = if params.include_content {
                Some(read_entry_content(wiki_dir, &entry)?)
            } else {
                None
            };
            return Ok(Some((
                RetrievalMethodUsed::DirectIdLookup,
                vec![entry_to_match(entry, 1.0, content)],
            )));
        }
        // ID 不存在也返回 direct_id_lookup（空结果）
        return Ok(Some((RetrievalMethodUsed::DirectIdLookup, vec![])));
    }

    // ref-xxx 反查 summary 页
    if id::is_ref_id(query) {
        if let Some(entry) = db::find_by_ref_id(conn, query)? {
            let content = if params.include_content {
                Some(read_entry_content(wiki_dir, &entry)?)
            } else {
                None
            };
            return Ok(Some((
                RetrievalMethodUsed::DirectIdLookup,
                vec![entry_to_match(entry, 1.0, content)],
            )));
        }
        // refID 无对应 summary 页，返回空
        return Ok(Some((RetrievalMethodUsed::DirectIdLookup, vec![])));
    }

    // tag-xxx 查关联条目
    if id::is_tag_id(query) {
        let entries = db::find_by_tag(conn, query)?;
        let mut matches = Vec::new();
        for entry in entries {
            let content = if params.include_content {
                Some(read_entry_content(wiki_dir, &entry)?)
            } else {
                None
            };
            matches.push(entry_to_match(entry, 1.0, content));
        }
        return Ok(Some((RetrievalMethodUsed::DirectIdLookup, matches)));
    }

    // 名称精确匹配（视为 ID 直查，score=1.0）
    let exact_matches = db::find_by_title(conn, query)?;
    if !exact_matches.is_empty() {
        let mut matches = Vec::new();
        for entry in exact_matches {
            // 应用 type 过滤
            if let Some(wt) = params.wiki_type {
                if entry.wiki_type != wt {
                    continue;
                }
            }
            let content = if params.include_content {
                Some(read_entry_content(wiki_dir, &entry)?)
            } else {
                None
            };
            matches.push(entry_to_match(entry, 1.0, content));
        }
        if !matches.is_empty() {
            return Ok(Some((RetrievalMethodUsed::DirectIdLookup, matches)));
        }
    }

    Ok(None)
}

/// FTS5 关键词检索。
///
/// 返回 (entry_id, normalized_score) 列表。
fn keyword_search(
    conn: &rusqlite::Connection,
    params: &QueryParams<'_>,
) -> Result<Vec<(String, f64)>> {
    let raw = db::search_keyword(conn, params.query, params.top_k, params.wiki_type)?;

    // bm25 分数越小越相关（通常为负值，越负越相关），取负后越大越相关
    if raw.is_empty() {
        return Ok(vec![]);
    }

    let max_neg = raw
        .iter()
        .map(|(_, s)| (-s).max(0.0))
        .fold(0.0f64, f64::max);
    if max_neg == 0.0 {
        return Ok(raw.into_iter().map(|(id, _)| (id, 1.0)).collect());
    }

    Ok(raw
        .into_iter()
        .map(|(id, score)| {
            // 取负后归一化到 [0, 1]，越大越相关
            let neg = (-score).max(0.0);
            let normalized = neg / max_neg;
            (id, normalized)
        })
        .collect())
}

/// 语义检索（embedding 余弦相似度）。
///
/// `query_vec` 为调用方预计算的查询向量。
fn semantic_search(
    conn: &rusqlite::Connection,
    params: &QueryParams<'_>,
    query_vec: &[f32],
) -> Result<Vec<(String, f64)>> {
    // 获取所有条目向量
    let all_embeddings = db::get_all_embeddings(conn)?;

    // 计算余弦相似度
    let mut scored: Vec<(String, f64)> = all_embeddings
        .into_iter()
        .map(|(id, vec)| {
            let sim = cosine_similarity(query_vec, &vec);
            (id, sim)
        })
        .collect();

    // 按相似度降序排序
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // 应用 type 过滤（单次查询，避免 N+1）
    if let Some(wt) = params.wiki_type {
        let type_map = db::list_entry_types(conn)?;
        scored.retain(|(id, _)| type_map.get(id) == Some(&wt));
    }

    // top_k 截断
    scored.truncate(params.top_k);

    Ok(scored)
}

/// 混合检索：关键词高权重，完全匹配时关键词主导、向量辅助；非完全匹配时加权融合。
///
/// 权重策略（动态插值）：
/// - keyword_score 越高，keyword 权重越大（0.7 → 0.9），semantic 权重越小（0.3 → 0.1）
/// - 完全匹配（kw=1.0）：final ≈ 0.9 + 0.1·sem，关键词主导，向量仅作微调辅助
/// - 部分匹配（kw=0.5）：final = 0.4 + 0.1·sem，关键词与向量共同决定
/// - 无关键词命中（kw=0.0）：final = 0.3·sem，纯语义兜底
///
/// 两路结果按 entry_id 配对后加权求和（取较高者逻辑已被加权求和取代，
/// 避免单路命中直接顶满导致的向量信号丢失）。
fn hybrid_search(
    conn: &rusqlite::Connection,
    params: &QueryParams<'_>,
    query_vec: &[f32],
) -> Result<Vec<(String, f64)>> {
    let keyword_results = keyword_search(conn, params)?;
    let semantic_results = semantic_search(conn, params, query_vec)?;

    // 合并两路分数：保留 (keyword_score, semantic_score) 对，缺失侧记 0.0
    let mut merged: std::collections::HashMap<String, (f64, f64)> =
        std::collections::HashMap::new();

    for (id, score) in &keyword_results {
        merged.entry(id.clone()).or_insert((0.0, 0.0)).0 = *score;
    }
    for (id, score) in &semantic_results {
        merged.entry(id.clone()).or_insert((0.0, 0.0)).1 = *score;
    }

    // 动态权重：keyword_score 越高，关键词权重越大
    let mut results: Vec<(String, f64)> = merged
        .into_iter()
        .map(|(id, (kw, sem))| {
            let kw_weight = 0.7 + 0.2 * kw; // 0.7 ~ 0.9
            let sem_weight = 1.0 - kw_weight; // 0.3 ~ 0.1
            let final_score = kw * kw_weight + sem * sem_weight;
            (id, final_score)
        })
        .collect();

    // 按分数降序排序
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // top_k 截断
    results.truncate(params.top_k);

    Ok(results)
}

/// 将 (entry_id, score) 列表填充为完整的 QueryMatch。
fn fill_matches(
    conn: &rusqlite::Connection,
    wiki_dir: &Path,
    raw: Vec<(String, f64)>,
    include_content: bool,
) -> Result<Vec<QueryMatch>> {
    let mut matches = Vec::with_capacity(raw.len());
    for (id, score) in raw {
        let entry = match db::get_entry(conn, &id)? {
            Some(e) => e,
            None => continue,
        };
        let content = if include_content {
            Some(read_entry_content(wiki_dir, &entry)?)
        } else {
            None
        };
        matches.push(entry_to_match(entry, score, content));
    }
    Ok(matches)
}

/// 将 WikiEntry 转为 QueryMatch。
fn entry_to_match(entry: WikiEntry, score: f64, content: Option<String>) -> QueryMatch {
    QueryMatch {
        wiki_id: entry.id,
        wiki_type: entry.wiki_type,
        title: entry.title,
        file_path: entry.file_path,
        score,
        content,
    }
}

/// 读取条目的 Markdown 正文（不含 frontmatter）。
fn read_entry_content(wiki_dir: &Path, entry: &WikiEntry) -> Result<String> {
    let path = wiki_dir
        .parent()
        .unwrap_or(Path::new("."))
        .join(&entry.file_path);
    if !path.exists() {
        return Ok(String::new());
    }
    let content = std::fs::read_to_string(&path)?;
    let (_, body) = crate::frontmatter::split_front_matter(&content);
    Ok(body.to_string())
}

/// 计算余弦相似度。
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;

    for i in 0..a.len() {
        let av = a[i] as f64;
        let bv = b[i] as f64;
        dot += av * bv;
        norm_a += av * av;
        norm_b += bv * bv;
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);

        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c) - 0.0).abs() < 1e-6);

        let d = vec![1.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &d);
        assert!((sim - 0.70710678).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
        assert_eq!(cosine_similarity(&[1.0], &[]), 0.0);
    }
}
