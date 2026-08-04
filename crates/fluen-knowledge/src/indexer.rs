use rusqlite::Connection;
use std::path::Path;

use crate::db;
use crate::error::{KnowledgeError, Result};
use crate::frontmatter;
use crate::id;
use crate::markdown;
use crate::types::{WikiEntry, WikiType};
use crate::util::atomic_write;

/// 全量重建 wiki 知识库索引。
///
/// 扫描 `wiki/{concepts,entities,summaries}/` 下的所有 `.md` 文件，
/// 解析 frontmatter + 正文，重建 SQLite 数据库 + index.md。
///
/// 重建过程在单个事务中执行：DELETE + INSERT 全部成功后才 COMMIT，
/// 任一步骤失败则 ROLLBACK，避免数据丢失。
///
/// **警告**：全量重建会清空 `embeddings` 表。如果之前存储了 embedding 向量，
/// 重建后需重新计算并写入（调用方负责）。
pub fn rebuild_full_index(wiki_dir: &Path) -> Result<Connection> {
    let conn = db::open_db(wiki_dir)?;
    db::transaction(&conn, |conn| rebuild_full_index_impl(conn, wiki_dir))?;
    Ok(conn)
}

fn rebuild_full_index_impl(conn: &Connection, wiki_dir: &Path) -> Result<()> {
    // 清空现有数据
    conn.execute_batch(
        "DELETE FROM entry_relations;
         DELETE FROM entry_tags;
         DELETE FROM embeddings;
         DELETE FROM entries;"
    )?;

    let mut count = 0;

    // 扫描三个子目录
    for wiki_type in [WikiType::Concept, WikiType::Entity, WikiType::Summary] {
        let type_dir = wiki_dir.join(wiki_type.dir());
        if !type_dir.exists() {
            continue;
        }

        for entry in std::fs::read_dir(&type_dir)? {
            let entry = entry?;
            let path = entry.path();
            let filename = entry.file_name();
            let name = filename.to_string_lossy();

            if !name.ends_with(".md") || !name.starts_with("wiki-") {
                continue;
            }

            match parse_entry_file(&path, wiki_type) {
                Ok(wiki_entry) => {
                    if let Err(e) = db::insert_entry(conn, &wiki_entry) {
                        tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "failed to index entry"
                        );
                    } else {
                        count += 1;
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "failed to parse entry file during rebuild"
                    );
                }
            }
        }
    }

    // 重建 index.md
    let entries = db::list_entries(conn)?;
    let tags = db::list_tags(conn)?;
    if let Err(e) = crate::index_md::rebuild_index_md(wiki_dir, "知识库索引", &entries, &tags) {
        tracing::warn!(error = %e, "failed to rebuild index.md");
    }

    tracing::info!(indexed = count, "wiki full index rebuild completed");
    Ok(())
}

/// 同步单个条目：从 MD 文件更新 SQLite 索引。
pub fn sync_entry(conn: &Connection, wiki_dir: &Path, entry_id: &str) -> Result<()> {
    // 在三个目录中查找该条目的文件
    let file_path = find_entry_file(wiki_dir, entry_id)?;

    if let Some(path) = file_path {
        let wiki_type = infer_type_from_path(&path)?;
        let entry = parse_entry_file(&path, wiki_type)?;

        if db::get_entry(conn, entry_id)?.is_some() {
            db::update_entry(conn, &entry)?;
        } else {
            db::insert_entry(conn, &entry)?;
        }
    } else {
        // 文件不存在，从索引删除
        db::delete_entry(conn, entry_id)?;
    }

    Ok(())
}

/// 删除条目（MD 文件 + DB 记录 + index.md 同步）。
pub fn delete_entry_full(conn: &Connection, wiki_dir: &Path, entry_id: &str) -> Result<()> {
    // 删除 MD 文件
    if let Some(path) = find_entry_file(wiki_dir, entry_id)? {
        std::fs::remove_file(&path)?;
    }

    // 删除 DB 记录
    db::delete_entry(conn, entry_id)?;

    // 清理孤立关系
    db::cleanup_orphan_relations(conn)?;

    Ok(())
}

/// 清理已删除文献相关的 wiki 条目。
///
/// 删除 source 字段指向该文献的 summary 页，并清理其他条目中指向该 summary 的关系
/// （包括 DB 中的 entry_relations 记录和 MD 文件中的 `[[]]` 链接）。
pub fn cleanup_for_deleted_reference(
    conn: &Connection,
    wiki_dir: &Path,
    reference_id: &str,
) -> Result<Vec<String>> {
    let mut deleted_ids = Vec::new();

    // 找到该文献对应的 summary 页
    if let Some(summary_entry) = db::find_by_ref_id(conn, reference_id)? {
        let summary_id = summary_entry.id.clone();
        deleted_ids.push(summary_id.clone());

        // 在删除前，找出所有关联到该 summary 的条目（用于后续 MD 文件清理）。
        // 必须在 delete_entry 之前查询，因为 delete_entry 会级联清除 entry_relations。
        let entries_to_clean: Vec<WikiEntry> = db::list_entries(conn)?
            .into_iter()
            .filter(|e| e.relations.contains(&summary_id))
            .collect();

        // 删除 summary 页文件
        let summary_path = wiki_dir
            .parent()
            .unwrap_or(Path::new("."))
            .join(&summary_entry.file_path);
        if summary_path.exists() {
            let _ = std::fs::remove_file(&summary_path);
        }

        // 删除 DB 记录（级联清除 entry_tags / entry_relations / embeddings）
        db::delete_entry(conn, &summary_id)?;

        // 清理关联条目的 MD 文件中的 [[]] 链接，并更新 DB 中的 relations
        for entry in &entries_to_clean {
            let path = wiki_dir
                .parent()
                .unwrap_or(Path::new("."))
                .join(&entry.file_path);
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let (fm_str, body) = frontmatter::split_front_matter(&content);
                    let cleaned_body = markdown::remove_relations(body, &deleted_ids);
                    if let Ok(fm) = frontmatter::parse_front_matter(fm_str) {
                        let new_content = frontmatter::build_markdown(&fm, &cleaned_body)?;
                        let _ = atomic_write(&path, new_content.as_bytes());
                    }
                }
            }

            // 更新 DB 中的 relations（移除已删除的 ID）
            let mut updated = entry.clone();
            updated.relations.retain(|r| !deleted_ids.contains(r));
            updated.updated = chrono::Utc::now().to_rfc3339();
            let _ = db::update_entry(conn, &updated);
        }
    }

    // 清理孤立关系（兜底）
    db::cleanup_orphan_relations(conn)?;

    tracing::info!(
        reference_id = reference_id,
        deleted_entries = deleted_ids.len(),
        "cleaned up wiki for deleted reference"
    );

    Ok(deleted_ids)
}

/// 在三个目录中查找指定 wikiID 的条目文件。
fn find_entry_file(wiki_dir: &Path, entry_id: &str) -> Result<Option<std::path::PathBuf>> {
    for wiki_type in [WikiType::Concept, WikiType::Entity, WikiType::Summary] {
        let type_dir = wiki_dir.join(wiki_type.dir());
        if !type_dir.exists() {
            continue;
        }

        for entry in std::fs::read_dir(&type_dir)? {
            let entry = entry?;
            let filename = entry.file_name();
            let name = filename.to_string_lossy();

            if let Some(id) = id::extract_id_from_filename(&name) {
                if id == entry_id {
                    return Ok(Some(entry.path()));
                }
            }
        }
    }
    Ok(None)
}

/// 从文件路径推断条目类型。
fn infer_type_from_path(path: &Path) -> Result<WikiType> {
    let parent = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .ok_or_else(|| KnowledgeError::Invalid(format!("cannot infer type from path: {}", path.display())))?;

    WikiType::from_dir(parent)
        .ok_or_else(|| KnowledgeError::Invalid(format!("unknown wiki type directory: {}", parent)))
}

/// 解析条目 MD 文件为 WikiEntry。
fn parse_entry_file(path: &Path, wiki_type: WikiType) -> Result<WikiEntry> {
    let content = std::fs::read_to_string(path)?;

    let (fm_str, body) = frontmatter::split_front_matter(&content);
    let fm = frontmatter::parse_front_matter(fm_str)?;

    // 从文件名提取 wikiID
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| KnowledgeError::Invalid(format!("invalid filename: {}", path.display())))?;

    let entry_id = id::extract_id_from_filename(filename)
        .ok_or_else(|| KnowledgeError::Invalid(format!("cannot extract wikiID from filename: {}", filename)))?;

    // 提取正文中的 wiki 链接作为 relations
    let links = markdown::extract_wiki_links(body);
    let relations: Vec<String> = links
        .iter()
        .filter_map(|link| markdown::extract_wiki_id_from_link(link))
        .filter(|id| id != &entry_id)
        .collect();

    // 构造相对路径（相对 references/），统一使用 / 作为分隔符
    let file_path = path
        .strip_prefix(path.parent().and_then(|p| p.parent()).unwrap_or(Path::new(".")))
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");

    // 确保 file_path 以 wiki/ 开头
    let file_path = if file_path.starts_with("wiki/") {
        file_path
    } else {
        format!("wiki/{}", file_path)
    };

    Ok(WikiEntry {
        id: entry_id,
        wiki_type,
        title: fm.title,
        file_path,
        source: fm.source,
        authors: fm.authors,
        tags: fm.tags,
        relations,
        content: body.to_string(),
        created: fm.created,
        updated: fm.updated,
    })
}
