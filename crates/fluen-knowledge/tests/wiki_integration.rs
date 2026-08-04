//! 知识库端到端集成测试。
//!
//! 覆盖 wiki.md 规范的核心流程：
//! 1. 端到端：init → create → query → edit → meta → delete
//! 2. embedding 降级：无 embedding 配置时 hybrid/semantic → keyword
//! 3. 删除文献：cleanup_for_deleted_reference 清理 summary 页与关系
//!
//! 注意：FTS5 的 `unicode61` 分词器对 CJK 文本按整串作为一个 token，
//! 无法做中文部分匹配。为保证关键词检索测试的可靠性，本测试套件
//! 使用英文内容构造条目。

use fluen_knowledge::types::{
    EditOp, MetaQueryType, RetrievalMethod, RetrievalMethodUsed, WikiType,
};
use fluen_knowledge::wiki;
use tempfile::TempDir;

/// 构造测试用 references/ 目录（含 wiki/ 子结构）。
fn setup_references_dir() -> TempDir {
    let dir = TempDir::new().expect("failed to create temp dir");
    wiki::init_wiki(dir.path()).expect("failed to init wiki");
    dir
}

// ════════════════════════════════════════════════════════════════
// 8.1 端到端测试：init → create → query → edit → meta → delete
// ════════════════════════════════════════════════════════════════

#[test]
fn test_end_to_end_flow() {
    let dir = setup_references_dir();
    let refs_dir = dir.path();
    let conn = wiki::open_wiki(refs_dir).expect("failed to open wiki db");

    // ── 1. 新建 concept 条目 ──
    let concept_id = {
        let result = wiki::create_entry(
            &conn,
            refs_dir,
            wiki::CreateEntryParams {
                wiki_type: WikiType::Concept,
                title: "Digital Intelligence".to_string(),
                content: "Digital intelligence is the fusion of data and AI.".to_string(),
                source: None,
                authors: vec![],
                tags: vec!["education technology".to_string(), "data science".to_string()],
                relations: vec![],
            },
        )
        .expect("failed to create concept entry");

        assert!(result.success);
        assert!(result.wiki_id.starts_with("wiki-"));
        assert!(result.file_path.starts_with("wiki/concepts/"));
        assert!(result.file_path.ends_with(".md"));
        assert_eq!(result.tags_generated.len(), 2);
        for mapping in &result.tags_generated {
            assert!(mapping.tag_id.starts_with("tag-"));
        }
        result.wiki_id
    };

    // ── 2. 新建 entity 条目并建立关系 ──
    let entity_id = {
        let result = wiki::create_entry(
            &conn,
            refs_dir,
            wiki::CreateEntryParams {
                wiki_type: WikiType::Entity,
                title: "Mou Zhijia".to_string(),
                content: "Professor at Jiangnan University, research: personalized learning.".to_string(),
                source: None,
                authors: vec![],
                tags: vec!["scholar".to_string()],
                relations: vec![concept_id.clone()],
            },
        )
        .expect("failed to create entity entry");
        result.wiki_id
    };

    // ── 3. 关键词查询（应命中 concept 条目）──
    // 使用 "fusion" 而非标题本身，避免触发名称精确匹配（DirectIdLookup）
    {
        let result = wiki::query(
            &conn,
            refs_dir,
            wiki::QueryEntryParams {
                query: "fusion",
                wiki_type: None,
                method: RetrievalMethod::Keyword,
                top_k: 10,
                include_content: false,
            },
            None,
        )
        .expect("failed to query by keyword");

        assert!(result.success);
        assert_eq!(result.retrieval_method_used, RetrievalMethodUsed::Keyword);
        assert!(!result.results.is_empty(), "keyword search should find entries");
        let found_concept = result
            .results
            .iter()
            .any(|m| m.wiki_id == concept_id);
        assert!(found_concept, "concept entry should be in keyword results");
    }

    // ── 4. ID 直查（应走 direct_id_lookup 路径）──
    {
        let result = wiki::query(
            &conn,
            refs_dir,
            wiki::QueryEntryParams {
                query: &entity_id,
                wiki_type: None,
                method: RetrievalMethod::Keyword,
                top_k: 10,
                include_content: true,
            },
            None,
        )
        .expect("failed to query by id");

        assert_eq!(result.retrieval_method_used, RetrievalMethodUsed::DirectIdLookup);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].wiki_id, entity_id);
        assert_eq!(result.results[0].score, 1.0);
        assert!(result.results[0].content.is_some());
        assert!(result.results[0]
            .content
            .as_ref()
            .unwrap()
            .contains("Jiangnan University"));
    }

    // ── 5. 名称精确匹配（应走 direct_id_lookup 路径）──
    {
        let result = wiki::query(
            &conn,
            refs_dir,
            wiki::QueryEntryParams {
                query: "Mou Zhijia",
                wiki_type: None,
                method: RetrievalMethod::Keyword,
                top_k: 10,
                include_content: false,
            },
            None,
        )
        .expect("failed to query by title");

        assert_eq!(result.retrieval_method_used, RetrievalMethodUsed::DirectIdLookup);
        assert!(result.results.iter().any(|m| m.wiki_id == entity_id));
    }

    // ── 6. 修改条目（search_replace + add_tags）──
    {
        let result = wiki::edit_entry(
            &conn,
            refs_dir,
            wiki::EditEntryParams {
                wiki_id: entity_id.clone(),
                edits: vec![EditOp::SearchReplace {
                    search: "personalized learning".to_string(),
                    replace: "personalized learning and learning analytics".to_string(),
                }],
                add_relations: vec![],
                add_tags: vec!["artificial intelligence".to_string()],
            },
        )
        .expect("failed to edit entry");

        assert!(result.success);
        assert_eq!(result.wiki_id, entity_id);
        // 至少包含 search_replace + add_tags 两项
        assert!(result.edit_results.len() >= 2);
        assert!(result.edit_results.iter().any(|r| r.edit_type == "search_replace" && r.success));
        assert!(result.edit_results.iter().any(|r| r.edit_type == "add_tags" && r.success));
    }

    // ── 7. 验证修改后的内容 ──
    {
        let entry = wiki::get_entry_full(&conn, refs_dir, &entity_id)
            .expect("failed to get entry")
            .expect("entry should exist");
        assert!(entry.entry.content.contains("learning analytics"), "edit should have applied");
        assert!(entry.entry.tags.len() >= 2, "should have at least 2 tags after add_tags");
    }

    // ── 8. 元信息查询（overview）──
    {
        let result = wiki::meta(&conn, MetaQueryType::Overview, 0)
            .expect("failed to get meta overview");
        assert!(result.success);
        assert_eq!(result.data.total_entries, 2, "should have 2 entries");
        assert!(result.data.total_tags >= 3, "should have at least 3 tags");
        assert!(!result.data.embedding_enabled, "no embeddings stored");
        assert!(result.data.tags.is_none());
        assert!(result.data.recent_entries.is_none());
    }

    // ── 9. 元信息查询（tags）──
    {
        let result = wiki::meta(&conn, MetaQueryType::Tags, 0)
            .expect("failed to get meta tags");
        assert!(result.success);
        let tags = result.data.tags.as_ref().expect("tags should be present");
        assert!(tags.len() >= 3, "should list all tags");
    }

    // ── 10. 元信息查询（recent）──
    {
        let result = wiki::meta(&conn, MetaQueryType::Recent, 5)
            .expect("failed to get meta recent");
        assert!(result.success);
        let recent = result
            .data
            .recent_entries
            .as_ref()
            .expect("recent_entries should be present");
        assert_eq!(recent.len(), 2, "should have 2 recent entries");
    }

    // ── 11. 删除条目 ──
    {
        wiki::delete_entry(&conn, refs_dir, &entity_id)
            .expect("failed to delete entry");
        let opt = wiki::get_entry_full(&conn, refs_dir, &entity_id)
            .expect("failed to get entry after delete");
        assert!(opt.is_none(), "entry should be deleted");
    }

    // ── 12. 验证删除后元信息 ──
    {
        let result = wiki::meta(&conn, MetaQueryType::Overview, 0)
            .expect("failed to get meta after delete");
        assert_eq!(result.data.total_entries, 1, "should have 1 entry after delete");
    }

    // ── 13. 验证 index.md 同步 ──
    {
        let index_path = refs_dir.join("wiki").join("index.md");
        assert!(index_path.exists(), "index.md should exist");
        let content = std::fs::read_to_string(&index_path).unwrap();
        assert!(content.contains("Digital Intelligence"), "index.md should contain remaining entry");
    }
}

// ════════════════════════════════════════════════════════════════
// 8.2 embedding 降级测试
// ════════════════════════════════════════════════════════════════

#[test]
fn test_embedding_degradation_hybrid_to_keyword() {
    let dir = setup_references_dir();
    let refs_dir = dir.path();
    let conn = wiki::open_wiki(refs_dir).expect("failed to open wiki db");

    // 新建条目（不存储 embedding）
    wiki::create_entry(
        &conn,
        refs_dir,
        wiki::CreateEntryParams {
            wiki_type: WikiType::Concept,
            title: "Machine Learning".to_string(),
            content: "Machine learning is a branch of artificial intelligence.".to_string(),
            source: None,
            authors: vec![],
            tags: vec![],
            relations: vec![],
        },
    )
    .expect("failed to create entry");

    // hybrid 检索但 DB 中无 embedding 数据 → 应降级为 keyword
    // 使用 "branch" 而非标题，避免触发名称精确匹配
    let result = wiki::query(
        &conn,
        refs_dir,
        wiki::QueryEntryParams {
            query: "branch",
            wiki_type: None,
            method: RetrievalMethod::Hybrid,
            top_k: 10,
            include_content: false,
        },
        // 即使传入了 query_embedding，DB 中无 embedding 数据也应降级
        Some(&[0.1, 0.2, 0.3]),
    )
    .expect("failed to query hybrid");

    assert!(
        result.success,
        "hybrid query should succeed even without embeddings"
    );
    assert_eq!(
        result.retrieval_method_used,
        RetrievalMethodUsed::Keyword,
        "hybrid should degrade to keyword when no embeddings in DB"
    );
    assert!(!result.results.is_empty(), "should still return results via keyword");
}

#[test]
fn test_embedding_degradation_semantic_to_keyword() {
    let dir = setup_references_dir();
    let refs_dir = dir.path();
    let conn = wiki::open_wiki(refs_dir).expect("failed to open wiki db");

    wiki::create_entry(
        &conn,
        refs_dir,
        wiki::CreateEntryParams {
            wiki_type: WikiType::Concept,
            title: "Deep Learning".to_string(),
            content: "Deep learning uses multi-layer neural networks.".to_string(),
            source: None,
            authors: vec![],
            tags: vec![],
            relations: vec![],
        },
    )
    .expect("failed to create entry");

    // semantic 检索但 query_embedding 为 None → 应降级为 keyword
    let result = wiki::query(
        &conn,
        refs_dir,
        wiki::QueryEntryParams {
            query: "neural",
            wiki_type: None,
            method: RetrievalMethod::Semantic,
            top_k: 10,
            include_content: false,
        },
        None,
    )
    .expect("failed to query semantic");

    assert_eq!(
        result.retrieval_method_used,
        RetrievalMethodUsed::Keyword,
        "semantic should degrade to keyword when query_embedding is None"
    );
}

#[test]
fn test_embedding_enabled_after_storing_vector() {
    let dir = setup_references_dir();
    let refs_dir = dir.path();
    let conn = wiki::open_wiki(refs_dir).expect("failed to open wiki db");

    // 新建条目
    let create_result = wiki::create_entry(
        &conn,
        refs_dir,
        wiki::CreateEntryParams {
            wiki_type: WikiType::Concept,
            title: "Vector Search".to_string(),
            content: "Vector search is based on embedding similarity.".to_string(),
            source: None,
            authors: vec![],
            tags: vec![],
            relations: vec![],
        },
    )
    .expect("failed to create entry");

    // 存储 embedding
    let vector = vec![0.1, 0.2, 0.3, 0.4];
    wiki::store_embedding(&conn, &create_result.wiki_id, &vector)
        .expect("failed to store embedding");

    // 元信息应显示 embedding_enabled = true
    let meta = wiki::meta(&conn, MetaQueryType::Overview, 0).expect("failed to get meta");
    assert!(
        meta.data.embedding_enabled,
        "embedding_enabled should be true after storing a vector"
    );

    // semantic 检索应正常工作（不再降级）
    // 使用 "similarity" 而非标题，避免触发名称精确匹配
    let result = wiki::query(
        &conn,
        refs_dir,
        wiki::QueryEntryParams {
            query: "similarity",
            wiki_type: None,
            method: RetrievalMethod::Semantic,
            top_k: 10,
            include_content: false,
        },
        Some(&[0.1, 0.2, 0.3, 0.4]),
    )
    .expect("failed to query semantic with embedding");

    assert_eq!(
        result.retrieval_method_used,
        RetrievalMethodUsed::Semantic,
        "semantic should work when embeddings are available"
    );
    assert!(!result.results.is_empty());
}

// ════════════════════════════════════════════════════════════════
// 8.3 删除文献测试：cleanup_for_deleted_reference
// ════════════════════════════════════════════════════════════════

#[test]
fn test_cleanup_for_deleted_reference() {
    let dir = setup_references_dir();
    let refs_dir = dir.path();
    let conn = wiki::open_wiki(refs_dir).expect("failed to open wiki db");

    // 生成一个 refID
    let ref_id = fluen_knowledge::id::generate_ref_id();

    // 新建 summary 条目，source 指向该文献
    let summary_id = {
        let result = wiki::create_entry(
            &conn,
            refs_dir,
            wiki::CreateEntryParams {
                wiki_type: WikiType::Summary,
                title: "Paper Summary".to_string(),
                content: "This is a summary of the referenced paper.".to_string(),
                source: Some(format!("raw/{}.pdf", ref_id)),
                authors: vec![],
                tags: vec![],
                relations: vec![],
            },
        )
        .expect("failed to create summary entry");
        result.wiki_id
    };

    // 新建 concept 条目，关联到 summary
    let concept_id = {
        let result = wiki::create_entry(
            &conn,
            refs_dir,
            wiki::CreateEntryParams {
                wiki_type: WikiType::Concept,
                title: "Related Concept".to_string(),
                content: "This concept is related to the above summary.".to_string(),
                source: None,
                authors: vec![],
                tags: vec![],
                relations: vec![summary_id.clone()],
            },
        )
        .expect("failed to create concept entry");
        result.wiki_id
    };

    // 验证初始状态
    {
        let meta = wiki::meta(&conn, MetaQueryType::Overview, 0).unwrap();
        assert_eq!(meta.data.total_entries, 2);
    }

    // 验证 concept 条目中包含指向 summary 的关系
    {
        let entry = wiki::get_entry_full(&conn, refs_dir, &concept_id)
            .unwrap()
            .unwrap();
        assert!(entry.entry.relations.contains(&summary_id));
        // MD 文件中应包含 [[]] 链接
        let file_path = refs_dir.join(&entry.entry.file_path);
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains(&format!("[[{}]]", summary_id)));
    }

    // 调用 cleanup_for_deleted_reference
    let deleted = wiki::cleanup_for_deleted_reference(&conn, refs_dir, &ref_id)
        .expect("failed to cleanup for deleted reference");

    // 应删除 summary 页
    assert_eq!(deleted.len(), 1, "should delete 1 summary entry");
    assert_eq!(deleted[0], summary_id);

    // 验证 summary 已从 DB 删除
    {
        let opt = wiki::get_entry_full(&conn, refs_dir, &summary_id).unwrap();
        assert!(opt.is_none(), "summary should be deleted");
    }

    // 验证 summary MD 文件已删除
    {
        let summary_path = refs_dir
            .join("wiki")
            .join("summaries");
        let entries: Vec<_> = std::fs::read_dir(&summary_path)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.starts_with("wiki-") && name.ends_with(".md")
            })
            .collect();
        assert!(
            entries.is_empty(),
            "summaries directory should be empty after cleanup"
        );
    }

    // 验证 concept 条目中的关系已被清理
    {
        let entry = wiki::get_entry_full(&conn, refs_dir, &concept_id)
            .unwrap()
            .expect("concept entry should still exist");
        assert!(
            !entry.entry.relations.contains(&summary_id),
            "relation to deleted summary should be removed from DB"
        );

        // MD 文件中的 [[]] 链接也应被清理
        let file_path = refs_dir.join(&entry.entry.file_path);
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(
            !content.contains(&format!("[[{}]]", summary_id)),
            "wiki link to deleted summary should be removed from MD"
        );
    }

    // 验证最终元信息
    {
        let meta = wiki::meta(&conn, MetaQueryType::Overview, 0).unwrap();
        assert_eq!(meta.data.total_entries, 1, "only concept entry should remain");
    }
}

#[test]
fn test_cleanup_for_nonexistent_reference() {
    let dir = setup_references_dir();
    let refs_dir = dir.path();
    let conn = wiki::open_wiki(refs_dir).expect("failed to open wiki db");

    // 不存在的 refID，应返回空列表且不报错
    let deleted = wiki::cleanup_for_deleted_reference(&conn, refs_dir, "ref-nonexistent1234")
        .expect("cleanup for nonexistent reference should not error");
    assert!(deleted.is_empty(), "no entries should be deleted");
}

// ════════════════════════════════════════════════════════════════
// 补充：批量查询测试
// ════════════════════════════════════════════════════════════════

#[test]
fn test_batch_query() {
    let dir = setup_references_dir();
    let refs_dir = dir.path();
    let conn = wiki::open_wiki(refs_dir).expect("failed to open wiki db");

    // 新建两个条目
    wiki::create_entry(
        &conn,
        refs_dir,
        wiki::CreateEntryParams {
            wiki_type: WikiType::Concept,
            title: "Natural Language Processing".to_string(),
            content: "NLP is an important branch of artificial intelligence.".to_string(),
            source: None,
            authors: vec![],
            tags: vec![],
            relations: vec![],
        },
    )
    .unwrap();

    wiki::create_entry(
        &conn,
        refs_dir,
        wiki::CreateEntryParams {
            wiki_type: WikiType::Concept,
            title: "Computer Vision".to_string(),
            content: "CV processes image and video data.".to_string(),
            source: None,
            authors: vec![],
            tags: vec![],
            relations: vec![],
        },
    )
    .unwrap();

    // 批量查询（使用内容中的词，避免触发名称精确匹配）
    let result = wiki::query_batch(
        &conn,
        refs_dir,
        wiki::BatchQueryParams {
            queries: vec!["important".to_string(), "image".to_string()],
            wiki_type: None,
            method: RetrievalMethod::Keyword,
            top_k: 5,
            include_content: false,
        },
        vec![None, None],
    )
    .expect("failed to batch query");

    assert!(result.success);
    assert_eq!(result.results.len(), 2, "should have 2 batch results");
    for item in &result.results {
        assert!(!item.matches.is_empty(), "each query should find results");
    }
}

// ════════════════════════════════════════════════════════════════
// 补充：rebuild_index 测试
// ════════════════════════════════════════════════════════════════

#[test]
fn test_rebuild_index() {
    let dir = setup_references_dir();
    let refs_dir = dir.path();
    let conn = wiki::open_wiki(refs_dir).expect("failed to open wiki db");

    // 新建条目
    wiki::create_entry(
        &conn,
        refs_dir,
        wiki::CreateEntryParams {
            wiki_type: WikiType::Concept,
            title: "Rebuild Index Test".to_string(),
            content: "Testing the full index rebuild functionality.".to_string(),
            source: None,
            authors: vec![],
            tags: vec!["testing".to_string()],
            relations: vec![],
        },
    )
    .unwrap();

    // 全量重建索引
    let new_conn = wiki::rebuild_index(refs_dir).expect("failed to rebuild index");

    // 验证重建后数据完整
    let meta = wiki::meta(&new_conn, MetaQueryType::Overview, 0).unwrap();
    assert_eq!(meta.data.total_entries, 1, "should have 1 entry after rebuild");
    assert!(meta.data.total_tags >= 1, "should have at least 1 tag after rebuild");
}
