//! 文献知识库搜索工具——学术助手检索文献知识库的入口。
//!
//! 基于 `fluen-knowledge` 的混合检索能力：
//! - **关键词**：SQLite FTS5（trigram 分词，中文子串匹配）+ BM25 排序
//! - **语义**：查询向量（EmbeddingRouter）与条目向量做余弦相似度
//! - **混合**：动态加权融合（关键词 0.7~0.9 + 向量 0.3~0.1），embedding 不可用时自动降级关键词
//!
//! 工具固定使用 **Hybrid 混合检索**，**最多返回 top4** 条最相关条目
//! （文献综述页 / 概念页 / 实体页），每条含 id、标题、类型与相关性评分。

use async_trait::async_trait;
use fluen_knowledge::async_kb::{AsyncKnowledgeBase, AsyncQueryParams};
use fluen_knowledge::types::{RetrievalMethod, WikiType};
use referee_ai::tool::{Tool, ToolContext, ToolError, ToolOutput};
use serde_json::{json, Value};

/// 工具名称。
pub const LITERATURE_SEARCH_TOOL_NAME: &str = "literature_search";

/// 工具描述。
const DESCRIPTION: &str = "搜索文献知识库：默认混合检索（关键词 + 语义向量），最多返回 4 条最相关的知识条目（文献综述 / 概念 / 实体），每条含 id、标题、类型与相关性评分。搜索前可先调用 paper_content 了解论文主题。";

/// 默认返回结果数上限（最多返回 4 条）。
const DEFAULT_TOP_K: usize = 4;

/// 文献知识库搜索工具。
pub struct LiteratureSearchTool {
    /// 输入参数 JSON Schema。
    parameters: Value,
    kb: AsyncKnowledgeBase,
}

impl LiteratureSearchTool {
    /// 构造工具。
    pub fn new(kb: AsyncKnowledgeBase) -> Self {
        let parameters = json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "搜索查询文本：关键词、文献主题、概念或语义描述（默认混合检索：关键词 + 语义向量）"
                },
                "wiki_type": {
                    "type": "string",
                    "enum": ["summary", "concept", "entity"],
                    "description": "限定条目类型（可选，不填检索全部）：summary=文献综述页，concept=概念页，entity=实体页"
                },
                "include_content": {
                    "type": "boolean",
                    "default": false,
                    "description": "是否在结果中包含条目正文（会增大返回体积，按需开启）"
                }
            },
            "required": ["query"]
        });

        Self { parameters, kb }
    }
}

#[async_trait]
impl Tool for LiteratureSearchTool {
    fn name(&self) -> &str {
        LITERATURE_SEARCH_TOOL_NAME
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn input_schema(&self) -> Value {
        self.parameters.clone()
    }

    /// 检索结果需同步返回（LLM 等待本轮调用完成再继续生成）。
    fn default_wait(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        _ctx: ToolContext,
        args: Value,
    ) -> Result<ToolOutput, ToolError> {
        // 1. 必填：query
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| ToolError::InvalidArguments("缺少 query 参数".into()))?;

        // 2. 可选：wiki_type（不填 = 检索全部；填了非法值报错）
        let wiki_type = match args.get("wiki_type") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => match s.as_str() {
                "summary" => Some(WikiType::Summary),
                "concept" => Some(WikiType::Concept),
                "entity" => Some(WikiType::Entity),
                other => {
                    return Err(ToolError::InvalidArguments(format!(
                        "wiki_type 取值非法: {}（可选 summary / concept / entity）",
                        other
                    )));
                }
            },
            Some(_) => return Err(ToolError::InvalidArguments("wiki_type 必须是字符串".into())),
        };

        // 3. 可选：include_content
        let include_content = args
            .get("include_content")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // 4. 固定默认：Hybrid 混合检索 + 最多 top4
        let params = AsyncQueryParams {
            query: query.to_string(),
            wiki_type,
            method: RetrievalMethod::Hybrid,
            top_k: DEFAULT_TOP_K,
            include_content,
        };

        let result = self
            .kb
            .query(params)
            .await
            .map_err(|e| ToolError::Execution(format!("文献检索失败: {}", e)))?;

        Ok(ToolOutput::from_json(&json!({
            "method": result.retrieval_method_used.as_str(),
            "count": result.results.len(),
            "results": result.results.iter().map(|m| json!({
                "id": m.wiki_id,
                "type": m.wiki_type,
                "title": m.title,
                "score": format!("{:.3}", m.score),
                "content": m.content,
            })).collect::<Vec<_>>()
        })))
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use fluen_knowledge::wiki::CreateEntryParams;
    use std::fs;

    fn temp_kb_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluen_literature_test_{}_{:?}_{}",
            std::process::id(),
            std::thread::current().id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn ctx() -> ToolContext {
        ToolContext {
            tool_call_id: "test-call".into(),
            session_id: uuid::Uuid::new_v4(),
            turn_id: 0,
            kernel: None,
            store: None,
            wait: true,
            peer_depth: 0,
        }
    }

    /// 解析工具输出为 JSON（工具返回值均为 `ToolOutput::from_json` 构造）。
    fn output_json(output: ToolOutput) -> Value {
        serde_json::from_str(&output.content).unwrap()
    }

    #[tokio::test]
    async fn missing_query_rejected() {
        let dir = temp_kb_dir();
        let kb = AsyncKnowledgeBase::init(&dir).unwrap();
        let tool = LiteratureSearchTool::new(kb);

        let err = tool
            .execute(ctx(), json!({ "wiki_type": "summary" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn invalid_wiki_type_rejected() {
        let dir = temp_kb_dir();
        let kb = AsyncKnowledgeBase::init(&dir).unwrap();
        let tool = LiteratureSearchTool::new(kb);

        let err = tool
            .execute(ctx(), json!({ "query": "test", "wiki_type": "bogus" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn empty_kb_returns_empty_results() {
        let dir = temp_kb_dir();
        let kb = AsyncKnowledgeBase::init(&dir).unwrap();
        let tool = LiteratureSearchTool::new(kb);

        let result = output_json(tool.execute(ctx(), json!({ "query": "机器学习" })).await.unwrap());
        assert_eq!(result["count"], 0);
        assert!(result["results"].as_array().unwrap().is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn search_returns_at_most_top4() {
        let dir = temp_kb_dir();
        let kb = AsyncKnowledgeBase::init(&dir).unwrap();

        // 插入 6 个与"深度学习"相关的概念条目
        for i in 0..6 {
            let title = format!("深度学习概念{}", i);
            let content = format!("深度学习是机器学习的一个分支，概念 {}。", i);
            kb.create_entry(CreateEntryParams {
                wiki_type: WikiType::Concept,
                title,
                content,
                source: None,
                authors: vec![],
                tags: vec!["deep-learning".into()],
                relations: vec![],
            })
            .await
            .unwrap();
        }

        let tool = LiteratureSearchTool::new(kb);
        let result = output_json(tool.execute(ctx(), json!({ "query": "深度学习" })).await.unwrap());

        // 最多返回 4 条
        assert!(result["count"].as_u64().unwrap() <= 4);
        assert_eq!(result["count"], result["results"].as_array().unwrap().len() as u64);

        // 每条含 id / title / type / score
        for item in result["results"].as_array().unwrap() {
            assert!(item["id"].is_string());
            assert!(item["title"].is_string());
            assert!(item["type"].is_string());
            assert!(item["score"].is_string());
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn search_with_include_content() {
        let dir = temp_kb_dir();
        let kb = AsyncKnowledgeBase::init(&dir).unwrap();

        kb.create_entry(CreateEntryParams {
            wiki_type: WikiType::Summary,
            title: "Transformer 综述".into(),
            content: "Transformer 架构在自然语言处理中广泛应用。".into(),
            source: Some("raw/ref-transformer.pdf".into()),
            authors: vec![],
            tags: vec![],
            relations: vec![],
        })
        .await
        .unwrap();

        let tool = LiteratureSearchTool::new(kb);
        let result = output_json(
            tool.execute(ctx(), json!({ "query": "Transformer", "include_content": true }))
                .await
                .unwrap(),
        );
        assert!(result["count"].as_u64().unwrap() >= 1);

        let _ = fs::remove_dir_all(&dir);
    }
}
