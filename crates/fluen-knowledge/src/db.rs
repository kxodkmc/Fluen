use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, ToSql};

use crate::error::{KnowledgeError, Result};
use crate::types::{WikiEntry, WikiTag, WikiType};

// ── 事务辅助 ─────────────────────────────────────────────────────

/// 在事务中执行操作，失败时自动回滚。
///
/// 使用 `BEGIN IMMEDIATE` 获取写锁，确保多步 DB 操作的原子性。
/// 闭包内的所有 DB 写入要么全部提交，要么全部回滚。
///
/// 注意：文件系统操作无法被 DB 事务回滚。若闭包内包含文件写入且后续
/// DB 操作失败，可能留下孤立文件（可通过 `rebuild_full_index` 修复）。
pub fn transaction<T, F>(conn: &Connection, f: F) -> Result<T>
where
    F: FnOnce(&Connection) -> Result<T>,
{
    conn.execute_batch("BEGIN IMMEDIATE;")?;
    match f(conn) {
        Ok(result) => {
            conn.execute_batch("COMMIT;")?;
            Ok(result)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK;");
            Err(e)
        }
    }
}

/// 打开或创建 wiki 知识库的 SQLite 数据库。
///
/// 数据库路径：`{wiki_dir}/index.db`
pub fn open_db(wiki_dir: &std::path::Path) -> Result<Connection> {
    let db_path = wiki_dir.join("index.db");
    let conn = Connection::open(&db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    conn.execute_batch("PRAGMA busy_timeout=5000;")?;
    init_schema(&conn)?;
    Ok(conn)
}

/// 初始化数据库 schema（第一代，无版本迁移）。
///
/// FTS5 使用 trigram 分词器以支持中文子串匹配——unicode61 无法拆分 CJK 汉字
/// （连续汉字被视为单个 token），导致中文关键词检索失效。trigram 将文本拆为
/// 3 字符滑动窗口，天然支持中文与英文的子串匹配。
/// 注意：trigram 要求查询词 ≥3 字符，短查询由 search_keyword 的 LIKE 回退处理。
fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS entries (
            id          TEXT PRIMARY KEY,
            type        TEXT NOT NULL,
            title       TEXT NOT NULL,
            file_path   TEXT NOT NULL,
            source      TEXT,
            created     TEXT NOT NULL,
            updated     TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS tags (
            id          TEXT PRIMARY KEY,
            title       TEXT NOT NULL UNIQUE
        );

        CREATE TABLE IF NOT EXISTS entry_tags (
            entry_id    TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
            tag_id      TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            PRIMARY KEY (entry_id, tag_id)
        );

        CREATE TABLE IF NOT EXISTS entry_relations (
            from_id     TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
            to_id       TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
            PRIMARY KEY (from_id, to_id)
        );

        CREATE TABLE IF NOT EXISTS embeddings (
            entry_id    TEXT PRIMARY KEY REFERENCES entries(id) ON DELETE CASCADE,
            embedding   BLOB NOT NULL,
            model       TEXT NOT NULL,
            updated     TEXT NOT NULL
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(
            id,
            title,
            type,
            content,
            tokenize='trigram'
        );"
    )?;
    Ok(())
}

// ── 行映射辅助 ─────────────────────────────────────────────────

/// DB 行的原始字段元组：(id, type_str, title, file_path, source, created, updated)。
type RawEntry = (String, String, String, String, Option<String>, String, String);

/// 从 `Row` 提取原始字段（供 `query_map` / `query_row` 使用）。
fn map_entry_row(row: &rusqlite::Row) -> rusqlite::Result<RawEntry> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
    ))
}

/// 从原始行数据构造完整 WikiEntry（含 tags 和 relations）。
///
/// `authors` 始终为空（仅存于 MD frontmatter），`content` 始终为空（需从 MD 文件读取）。
fn raw_to_entry(conn: &Connection, row: RawEntry) -> Result<WikiEntry> {
    let (id, type_str, title, file_path, source, created, updated) = row;
    let wiki_type = WikiType::from_str(&type_str)
        .ok_or_else(|| KnowledgeError::Invalid(format!("invalid wiki type in db: {type_str}")))?;
    Ok(WikiEntry {
        tags: query_entry_tags(conn, &id)?,
        relations: query_entry_relations(conn, &id)?,
        id,
        wiki_type,
        title,
        file_path,
        source,
        authors: vec![],
        content: String::new(),
        created,
        updated,
    })
}

// ── Entry CRUD ──────────────────────────────────────────────────

/// 插入新条目（含 tags、relations、FTS）。
pub fn insert_entry(conn: &Connection, entry: &WikiEntry) -> Result<()> {
    conn.execute(
        "INSERT INTO entries (id, type, title, file_path, source, created, updated)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            entry.id,
            entry.wiki_type.as_str(),
            entry.title,
            entry.file_path,
            entry.source,
            entry.created,
            entry.updated,
        ],
    )?;

    // 插入 tags 关联
    for tag_id in &entry.tags {
        conn.execute(
            "INSERT OR IGNORE INTO entry_tags (entry_id, tag_id) VALUES (?1, ?2)",
            params![entry.id, tag_id],
        )?;
    }

    // 插入 relations（跳过目标条目不存在的项，避免外键约束失败）
    insert_relations_safe(conn, &entry.id, &entry.relations)?;

    // 插入 FTS
    conn.execute(
        "INSERT INTO entries_fts (id, title, type, content) VALUES (?1, ?2, ?3, ?4)",
        params![entry.id, entry.title, entry.wiki_type.as_str(), entry.content],
    )?;

    Ok(())
}

/// 更新条目（含 tags、relations、FTS）。
pub fn update_entry(conn: &Connection, entry: &WikiEntry) -> Result<()> {
    conn.execute(
        "UPDATE entries SET type=?1, title=?2, file_path=?3, source=?4, updated=?5 WHERE id=?6",
        params![
            entry.wiki_type.as_str(),
            entry.title,
            entry.file_path,
            entry.source,
            entry.updated,
            entry.id,
        ],
    )?;

    // 刷新 tags
    conn.execute("DELETE FROM entry_tags WHERE entry_id=?1", params![entry.id])?;
    for tag_id in &entry.tags {
        conn.execute(
            "INSERT OR IGNORE INTO entry_tags (entry_id, tag_id) VALUES (?1, ?2)",
            params![entry.id, tag_id],
        )?;
    }

    // 刷新 relations
    conn.execute("DELETE FROM entry_relations WHERE from_id=?1", params![entry.id])?;
    insert_relations_safe(conn, &entry.id, &entry.relations)?;

    // 刷新 FTS
    conn.execute("DELETE FROM entries_fts WHERE id=?1", params![entry.id])?;
    conn.execute(
        "INSERT INTO entries_fts (id, title, type, content) VALUES (?1, ?2, ?3, ?4)",
        params![entry.id, entry.title, entry.wiki_type.as_str(), entry.content],
    )?;

    Ok(())
}

/// 删除条目（级联删除 tags、relations、embeddings、FTS）。
pub fn delete_entry(conn: &Connection, id: &str) -> Result<()> {
    // 清理指向该条目的 relations
    conn.execute("DELETE FROM entry_relations WHERE to_id=?1", params![id])?;
    // 级联删除处理 entry_tags、entry_relations、embeddings
    conn.execute("DELETE FROM entries WHERE id=?1", params![id])?;
    // FTS 不受外键级联，手动删
    conn.execute("DELETE FROM entries_fts WHERE id=?1", params![id])?;
    Ok(())
}

/// 安全插入 relations：跳过目标条目不存在的项。
///
/// `entry_relations` 表有外键约束 `REFERENCES entries(id)`，若 `to_id` 引用
/// 尚未创建的条目（两阶段 pipeline 中 CreatingSummary 先于 CreatingConcepts），
/// 直接插入会触发 `FOREIGN KEY constraint failed` 中断整个创建流程。
///
/// 此函数先检查 `to_id` 是否存在于 `entries` 表，存在才插入，不存在则跳过
/// 并记录 warning。跳过的 relations 由 pipeline 的 EstablishingRelations
/// 阶段在所有条目创建完成后补建。
fn insert_relations_safe(conn: &Connection, from_id: &str, to_ids: &[String]) -> Result<()> {
    for to_id in to_ids {
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM entries WHERE id = ?1)",
                params![to_id],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if exists {
            conn.execute(
                "INSERT OR IGNORE INTO entry_relations (from_id, to_id) VALUES (?1, ?2)",
                params![from_id, to_id],
            )?;
        } else {
            tracing::warn!(
                from_id = %from_id,
                to_id = %to_id,
                "跳过 relation：目标条目不存在（将在 EstablishingRelations 阶段补建）"
            );
        }
    }
    Ok(())
}

/// 按 ID 获取条目（不含 content，需从文件读取）。
pub fn get_entry(conn: &Connection, id: &str) -> Result<Option<WikiEntry>> {
    let row = conn
        .query_row(
            "SELECT id, type, title, file_path, source, created, updated
             FROM entries WHERE id=?1",
            params![id],
            map_entry_row,
        )
        .optional()?;

    row.map(|r| raw_to_entry(conn, r)).transpose()
}

/// 列出所有条目（基本信息，不含 content）。
pub fn list_entries(conn: &Connection) -> Result<Vec<WikiEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, type, title, file_path, source, created, updated FROM entries ORDER BY title"
    )?;
    let rows = stmt.query_map([], map_entry_row)?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(raw_to_entry(conn, row?)?);
    }
    Ok(entries)
}

/// 按类型列出条目。
pub fn list_entries_by_type(conn: &Connection, wiki_type: WikiType) -> Result<Vec<WikiEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, type, title, file_path, source, created, updated
         FROM entries WHERE type=?1 ORDER BY title"
    )?;
    let rows = stmt.query_map(params![wiki_type.as_str()], map_entry_row)?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(raw_to_entry(conn, row?)?);
    }
    Ok(entries)
}

// ── Tag CRUD ────────────────────────────────────────────────────

/// 按名称查找标签，不存在则创建，返回 tagID。
pub fn upsert_tag(conn: &Connection, title: &str) -> Result<String> {
    // 先查
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM tags WHERE title=?1",
            params![title],
            |row| row.get(0),
        )
        .optional()?;

    if let Some(id) = existing {
        return Ok(id);
    }

    // 创建
    let id = crate::id::generate_tag_id();
    conn.execute(
        "INSERT INTO tags (id, title) VALUES (?1, ?2)",
        params![id, title],
    )?;
    Ok(id)
}

/// 批量 upsert 标签，返回 (名称, tagID) 映射列表。
pub fn upsert_tags(conn: &Connection, titles: &[String]) -> Result<Vec<(String, String)>> {
    let mut result = Vec::with_capacity(titles.len());
    for title in titles {
        let id = upsert_tag(conn, title)?;
        result.push((title.clone(), id));
    }
    Ok(result)
}

/// 列出所有标签。
pub fn list_tags(conn: &Connection) -> Result<Vec<WikiTag>> {
    let mut stmt = conn.prepare("SELECT id, title FROM tags ORDER BY title")?;
    let tags = stmt
        .query_map([], |row| Ok(WikiTag {
            id: row.get(0)?,
            title: row.get(1)?,
        }))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(tags)
}

/// 为条目追加标签（去重）。
pub fn add_entry_tags(conn: &Connection, entry_id: &str, tag_ids: &[String]) -> Result<()> {
    for tag_id in tag_ids {
        conn.execute(
            "INSERT OR IGNORE INTO entry_tags (entry_id, tag_id) VALUES (?1, ?2)",
            params![entry_id, tag_id],
        )?;
    }
    Ok(())
}

/// 为条目追加关系（去重）。
pub fn add_entry_relations(conn: &Connection, entry_id: &str, to_ids: &[String]) -> Result<()> {
    for to_id in to_ids {
        conn.execute(
            "INSERT OR IGNORE INTO entry_relations (from_id, to_id) VALUES (?1, ?2)",
            params![entry_id, to_id],
        )?;
    }
    Ok(())
}

/// 按 ID 列表查询标签名称，返回与输入顺序一致的名称列表；找不到时回退为 ID。
pub fn get_tag_titles(conn: &Connection, tag_ids: &[String]) -> Result<Vec<String>> {
    if tag_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = tag_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("SELECT id, title FROM tags WHERE id IN ({})", placeholders);
    let mut stmt = conn.prepare(&sql)?;
    let params: Vec<&dyn ToSql> = tag_ids.iter().map(|id| id as &dyn ToSql).collect();
    let rows = stmt.query_map(params.as_slice(), |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut titles = HashMap::with_capacity(tag_ids.len());
    for row in rows {
        let (id, title) = row?;
        titles.insert(id, title);
    }
    Ok(tag_ids
        .iter()
        .map(|id| titles.get(id).cloned().unwrap_or_else(|| id.clone()))
        .collect())
}

/// 按 ID 列表查询关联条目标题，返回与输入顺序一致的标题列表；找不到时回退为 ID。
pub fn get_relation_titles(conn: &Connection, relation_ids: &[String]) -> Result<Vec<String>> {
    if relation_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = relation_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("SELECT id, title FROM entries WHERE id IN ({})", placeholders);
    let mut stmt = conn.prepare(&sql)?;
    let params: Vec<&dyn ToSql> = relation_ids
        .iter()
        .map(|id| id as &dyn ToSql)
        .collect();
    let rows = stmt.query_map(params.as_slice(), |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut titles = HashMap::with_capacity(relation_ids.len());
    for row in rows {
        let (id, title) = row?;
        titles.insert(id, title);
    }
    Ok(relation_ids
        .iter()
        .map(|id| titles.get(id).cloned().unwrap_or_else(|| id.clone()))
        .collect())
}

// ── Embedding CRUD ──────────────────────────────────────────────

/// 序列化 `Vec<f32>` 为 BLOB（小端字节序）。
pub fn serialize_embedding(vec: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vec.len() * 4);
    for &v in vec {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes
}

/// 反序列化 BLOB 为 `Vec<f32>`。
pub fn deserialize_embedding(blob: &[u8]) -> Vec<f32> {
    if blob.len() % 4 != 0 {
        return Vec::new();
    }
    blob.chunks_exact(4)
        .map(|chunk| {
            let arr: [u8; 4] = chunk.try_into().unwrap();
            f32::from_le_bytes(arr)
        })
        .collect()
}

/// 写入条目的 embedding。
pub fn set_embedding(conn: &Connection, entry_id: &str, embedding: &[f32], model: &str) -> Result<()> {
    let blob = serialize_embedding(embedding);
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO embeddings (entry_id, embedding, model, updated) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(entry_id) DO UPDATE SET embedding=?2, model=?3, updated=?4",
        params![entry_id, blob, model, now],
    )?;
    Ok(())
}

/// 读取条目的 embedding。
pub fn get_embedding(conn: &Connection, entry_id: &str) -> Result<Option<Vec<f32>>> {
    let blob: Option<Vec<u8>> = conn
        .query_row(
            "SELECT embedding FROM embeddings WHERE entry_id=?1",
            params![entry_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(blob.map(|b| deserialize_embedding(&b)))
}

/// 读取所有条目的 embedding（用于语义检索）。
pub fn get_all_embeddings(conn: &Connection) -> Result<Vec<(String, Vec<f32>)>> {
    let mut stmt = conn.prepare("SELECT entry_id, embedding FROM embeddings")?;
    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let blob: Vec<u8> = row.get(1)?;
        Ok((id, deserialize_embedding(&blob)))
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// 删除条目的 embedding。
pub fn delete_embedding(conn: &Connection, entry_id: &str) -> Result<()> {
    conn.execute("DELETE FROM embeddings WHERE entry_id=?1", params![entry_id])?;
    Ok(())
}

/// 检查是否有任何 embedding 数据。
pub fn has_embeddings(conn: &Connection) -> Result<bool> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM embeddings", [], |row| row.get(0))?;
    Ok(count > 0)
}

// ── Search ──────────────────────────────────────────────────────

/// FTS5 关键词检索，返回 (entry_id, bm25_score) 列表。
///
/// bm25 分数越小越相关（SQLite FTS5 约定），调用方需自行取负归一化。
///
/// trigram 分词器要求查询词 ≥3 字符。当所有词均不足 3 字符时，
/// 自动回退到 title LIKE 模糊匹配（正文不在 DB 中，仅匹配标题）。
pub fn search_keyword(conn: &Connection, query: &str, limit: usize, type_filter: Option<WikiType>) -> Result<Vec<(String, f64)>> {
    let fts_query = build_fts_query(query);

    // trigram 无法处理 <3 字符的词，回退到 LIKE 标题匹配
    if fts_query.is_empty() {
        return like_search_title(conn, query, limit, type_filter);
    }

    let sql = if type_filter.is_some() {
        "SELECT e.id, bm25(entries_fts) AS score
         FROM entries_fts fts
         JOIN entries e ON e.id = fts.id
         WHERE entries_fts MATCH ?1 AND fts.type = ?2
         ORDER BY score
         LIMIT ?3"
    } else {
        "SELECT e.id, bm25(entries_fts) AS score
         FROM entries_fts fts
         JOIN entries e ON e.id = fts.id
         WHERE entries_fts MATCH ?1
         ORDER BY score
         LIMIT ?2"
    };

    let mut stmt = conn.prepare(sql)?;
    let limit_i64 = limit as i64;
    let wt_str = type_filter.map(|wt| wt.as_str().to_string());
    let query_params: Vec<&dyn ToSql> = if let Some(ref wt) = wt_str {
        vec![&fts_query, wt, &limit_i64]
    } else {
        vec![&fts_query, &limit_i64]
    };
    let rows = stmt.query_map(query_params.as_slice(), |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
    })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// 构建 FTS5 查询字符串（trigram 分词器专用）。
///
/// 仅保留字符数 ≥3 的词（trigram 最低要求），用双引号包裹为短语查询，
/// 多词之间用 OR 连接。所有词均不足 3 字符时返回空串，调用方据此回退到 LIKE。
fn build_fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .filter(|term| term.chars().count() >= 3)
        .map(|term| format!("\"{}\"", term.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// LIKE 模糊匹配标题（trigram 短查询回退路径）。
///
/// 仅匹配 entries.title 列（正文存储在 MD 文件中，DB 内不可 LIKE）。
/// 返回固定得分 0.5，表示弱匹配。
fn like_search_title(conn: &Connection, query: &str, limit: usize, type_filter: Option<WikiType>) -> Result<Vec<(String, f64)>> {
    let pattern = format!("%{}%", query);
    let limit_i64 = limit as i64;
    let wt_str = type_filter.map(|wt| wt.as_str().to_string());

    let sql = if type_filter.is_some() {
        "SELECT id FROM entries WHERE title LIKE ?1 AND type = ?2 LIMIT ?3"
    } else {
        "SELECT id FROM entries WHERE title LIKE ?1 LIMIT ?2"
    };

    let mut stmt = conn.prepare(sql)?;
    let query_params: Vec<&dyn ToSql> = if let Some(ref wt) = wt_str {
        vec![&pattern, wt, &limit_i64]
    } else {
        vec![&pattern, &limit_i64]
    };
    let rows = stmt.query_map(query_params.as_slice(), |row| {
        Ok((row.get::<_, String>(0)?, 0.5f64))
    })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// 按名称精确查找条目（用于去重和名称直查）。
pub fn find_by_title(conn: &Connection, title: &str) -> Result<Vec<WikiEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, type, title, file_path, source, created, updated
         FROM entries WHERE title = ?1 COLLATE NOCASE"
    )?;
    let rows = stmt.query_map(params![title], map_entry_row)?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(raw_to_entry(conn, row?)?);
    }
    Ok(entries)
}

/// 按 source 字段精确查找条目（用于去重检查）。
///
/// `source` 应为完整路径，如 `raw/ref-xxx.pdf`。
pub fn find_by_source_exact(conn: &Connection, source: &str) -> Result<Option<WikiEntry>> {
    let row = conn
        .query_row(
            "SELECT id, type, title, file_path, source, created, updated
             FROM entries WHERE source = ?1 LIMIT 1",
            params![source],
            map_entry_row,
        )
        .optional()?;

    row.map(|r| raw_to_entry(conn, r)).transpose()
}

/// 按 refID 查找条目（source 字段格式为 `raw/{refID}.ext`）。
///
/// 使用精确前缀模式 `raw/{ref_id}.%` 匹配，避免 `ref-abc` 误匹配 `ref-abcdef`。
/// 支持任意文件扩展名（.pdf、.docx 等）。
pub fn find_by_ref_id(conn: &Connection, ref_id: &str) -> Result<Option<WikiEntry>> {
    let pattern = format!("raw/{}.%", ref_id);
    let row = conn
        .query_row(
            "SELECT id, type, title, file_path, source, created, updated
             FROM entries WHERE source LIKE ?1 LIMIT 1",
            params![pattern],
            map_entry_row,
        )
        .optional()?;

    row.map(|r| raw_to_entry(conn, r)).transpose()
}

/// 按 tagID 查找关联的所有条目。
pub fn find_by_tag(conn: &Connection, tag_id: &str) -> Result<Vec<WikiEntry>> {
    let mut stmt = conn.prepare(
        "SELECT e.id, e.type, e.title, e.file_path, e.source, e.created, e.updated
         FROM entries e
         JOIN entry_tags et ON e.id = et.entry_id
         WHERE et.tag_id = ?1 ORDER BY e.title"
    )?;
    let rows = stmt.query_map(params![tag_id], map_entry_row)?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(raw_to_entry(conn, row?)?);
    }
    Ok(entries)
}

// ── Batch helpers ───────────────────────────────────────────────

/// 批量获取所有条目的 ID 与类型映射（用于语义检索的 type 过滤）。
///
/// 单次查询替代逐条 `get_entry`，避免 N+1 问题。
pub fn list_entry_types(conn: &Connection) -> Result<HashMap<String, WikiType>> {
    let mut stmt = conn.prepare("SELECT id, type FROM entries")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut map = HashMap::new();
    for row in rows {
        let (id, type_str) = row?;
        if let Some(wt) = WikiType::from_str(&type_str) {
            map.insert(id, wt);
        }
    }
    Ok(map)
}

// ── Stats ───────────────────────────────────────────────────────

/// 统计条目总数。
pub fn count_entries(conn: &Connection) -> Result<u32> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))?;
    Ok(count as u32)
}

/// 统计标签总数。
pub fn count_tags(conn: &Connection) -> Result<u32> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM tags", [], |row| row.get(0))?;
    Ok(count as u32)
}

/// 获取最近更新的条目（按 updated 降序）。
pub fn recent_entries(conn: &Connection, limit: usize, type_filter: Option<WikiType>) -> Result<Vec<(String, String)>> {
    let sql = if type_filter.is_some() {
        "SELECT id, title FROM entries WHERE type=?1 ORDER BY updated DESC LIMIT ?2"
    } else {
        "SELECT id, title FROM entries ORDER BY updated DESC LIMIT ?1"
    };

    let mut stmt = conn.prepare(sql)?;
    let limit_i64 = limit as i64;
    let wt_str = type_filter.map(|wt| wt.as_str().to_string());
    let query_params: Vec<&dyn ToSql> = if let Some(ref wt) = wt_str {
        vec![wt, &limit_i64]
    } else {
        vec![&limit_i64]
    };
    let rows = stmt.query_map(query_params.as_slice(), |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

// ── Helpers ─────────────────────────────────────────────────────

fn query_entry_tags(conn: &Connection, entry_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.id FROM tags t
         JOIN entry_tags et ON t.id = et.tag_id
         WHERE et.entry_id=?1 ORDER BY t.title"
    )?;
    let tags = stmt
        .query_map(params![entry_id], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(tags)
}

fn query_entry_relations(conn: &Connection, entry_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT to_id FROM entry_relations WHERE from_id=?1")?;
    let relations = stmt
        .query_map(params![entry_id], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(relations)
}

/// 清理指向已删除条目的孤立关系。
pub fn cleanup_orphan_relations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DELETE FROM entry_relations WHERE to_id NOT IN (SELECT id FROM entries);
         DELETE FROM entry_relations WHERE from_id NOT IN (SELECT id FROM entries);"
    )?;
    Ok(())
}
