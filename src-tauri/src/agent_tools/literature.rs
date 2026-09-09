//! 文献知识库搜索工具——学术助手检索文献知识库的入口。
//!
//! 基于 `fluen-kb` 的混合检索能力：
//! - **关键词**：SQLite FTS5（trigram 分词，中文子串匹配）+ BM25 排序
//! - **语义**：查询向量（EmbeddingRouter）与条目向量做余弦相似度
//! - **混合**：动态加权融合（关键词 0.7~0.9 + 向量 0.3~0.1），embedding 不可用时自动降级关键词
//!
//! 工具固定使用 **Hybrid 混合检索**，**最多返回 top4** 条最相关条目
//! （文献综述页 / 概念页 / 实体页），每条含 id、标题、类型与相关性评分。
//!
//! M1 过渡实现：内部直调 fluen-kb（M2 将切换为 MCP `knowledge_query`）。

use async_trait::async_trait;
use crate::knowledge_mcp_bridge::KbMcpBridge;
use fluen_kb::WikiType;
use referee_ai::tool::{Tool, ToolContext, ToolError, ToolOutput};
use serde_json::{json, Value};

use crate::llm_config::model::LlmConfig;

/// 工具名称。
pub const LITERATURE_SEARCH_TOOL_NAME: &str = "literature_search";

/// 工具描述。
const DESCRIPTION: &str = "搜索文献知识库：默认混合检索（关键词 + 语义向量），最多返回 4 条最相关的知识条目（文献综述 / 概念 / 实体），每条含 id、标题、类型与相关性评分。搜索前可先调用 paper_outline 了解论文主题。";

/// 默认返回结果数上限（最多返回 4 条）。
const DEFAULT_TOP_K: usize = 4;

/// 文献知识库搜索工具。
///
/// 内部经进程内 MCP 桥接调用 `knowledge_query`（M2：Agent 面统一走 MCP），
/// 首次执行时懒建立连接（`open_kb` 为同步装配路径，无法直接 async 连接）。
pub struct LiteratureSearchTool {
    /// 输入参数 JSON Schema。
    parameters: Value,
    kb: fluen_kb::handle::Kb,
    bridge: tokio::sync::OnceCell<KbMcpBridge>,
}

impl LiteratureSearchTool {
    /// 构造工具。
    pub fn new(kb: fluen_kb::handle::Kb) -> Self {
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

        Self {
            parameters,
            kb,
            bridge: tokio::sync::OnceCell::new(),
        }
    }

    /// 懒建立 MCP 桥接（首次执行时连接，后续复用）。
    async fn bridge(&self) -> Result<&KbMcpBridge, ToolError> {
        self.bridge
            .get_or_try_init(|| KbMcpBridge::connect(self.kb.clone()))
            .await
            .map_err(|e| ToolError::Execution(format!("知识库 MCP 桥接失败: {e}")))
    }
}

/// 打开项目文献知识库（供运行时装配使用）。
///
/// - `<project>/references/wiki/index.db` 不存在时返回 `None`（知识库尚未构建，
///   属正常状态，静默跳过）；
/// - 打开失败（索引损坏、旧版 schema 未迁移等）仅告警并返回 `None`，不阻断助手启动；
/// - 成功时注入 embedding router 启用语义检索（未配置 embedding 时由
///   fluen-kb 自动降级为关键词检索）。
pub fn open_kb(project_path: &str, llm: &LlmConfig) -> Option<fluen_kb::handle::Kb> {
    let references_dir = std::path::PathBuf::from(project_path).join("references");
    if !references_dir.join("wiki").join("index.db").is_file() {
        return None;
    }

    match crate::knowledge_builder::kb_adapter::open_kb(&references_dir, llm) {
        Ok(kb) => Some(kb),
        Err(e) => {
            tracing::warn!(
                references_dir = %references_dir.display(),
                "文献知识库打开失败，跳过 literature_search 工具: {e}"
            );
            None
        }
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

        // 4. 固定默认：Hybrid 混合检索 + 最多 top4（经 MCP knowledge_query 转发）
        let mut mcp_args = json!({
            "query": query,
            "method": "hybrid",
            "top_k": DEFAULT_TOP_K,
            "include_content": include_content,
        });
        if let Some(wt) = wiki_type {
            mcp_args["wiki_type"] = json!(wt.as_str());
        }

        let bridge = self.bridge().await?;
        let hits = bridge
            .call_tool("knowledge_query", mcp_args)
            .await
            .map_err(ToolError::Execution)?;

        // hits 为 MCP hit_json 数组；content 剥离溯源标签后返回
        let Some(results) = hits.as_array() else {
            return Err(ToolError::Execution("knowledge_query 返回格式异常".into()));
        };

        Ok(ToolOutput::from_json(&json!({
            "success": true,
            "method": "hybrid",
            "count": results.len(),
            "results": results.iter().map(|h| {
                let content = h.get("content").and_then(Value::as_str).map(|c| {
                    let (plain, _) = fluen_kb::syntax::strip(c);
                    plain
                });
                json!({
                    "id": h.get("id").cloned().unwrap_or(Value::Null),
                    "type": h.get("type").cloned().unwrap_or(Value::Null),
                    "title": h.get("title").cloned().unwrap_or(Value::Null),
                    "score": h.get("score").cloned().unwrap_or(Value::Null),
                    "content": content,
                })
            }).collect::<Vec<_>>()
        })))
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use fluen_kb::ids::SourceId;
    use std::fs;
    use std::path::PathBuf;

    fn temp_kb_dir() -> PathBuf {
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

    fn test_kb(dir: &PathBuf) -> fluen_kb::handle::Kb {
        fluen_kb::KbBuilder::new(dir).open().unwrap()
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
        let kb = test_kb(&dir);
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
        let kb = test_kb(&dir);
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
        let kb = test_kb(&dir);
        let tool = LiteratureSearchTool::new(kb);

        let result = output_json(tool.execute(ctx(), json!({ "query": "机器学习" })).await.unwrap());
        assert_eq!(result["success"], true);
        assert_eq!(result["count"], 0);
        assert!(result["results"].as_array().unwrap().is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn search_returns_at_most_top4() {
        let dir = temp_kb_dir();
        let kb = test_kb(&dir);

        // 插入 6 个与"深度学习"相关的概念条目
        for i in 0..6 {
            let title = format!("深度学习概念{}", i);
            let content = format!("深度学习是机器学习的一个分支，概念 {}。", i);
            kb.ops()
                .create(WikiType::Concept, &title, &content, &[], None, &[])
                .unwrap();
        }

        let tool = LiteratureSearchTool::new(kb);
        let result = output_json(tool.execute(ctx(), json!({ "query": "深度学习" })).await.unwrap());

        // 最多返回 4 条
        assert!(result["count"].as_u64().unwrap() <= 4);
        assert_eq!(result["count"], result["results"].as_array().unwrap().len() as u64);

        // 每条含 id / title / type / score（score 为数值，遵循设计约定）
        for item in result["results"].as_array().unwrap() {
            assert!(item["id"].is_string());
            assert!(item["title"].is_string());
            assert!(item["type"].is_string());
            assert!(item["score"].is_number(), "score 应为数值: {}", item["score"]);
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn search_with_include_content() {
        let dir = temp_kb_dir();
        let kb = test_kb(&dir);

        kb.ops()
            .create(
                WikiType::Summary,
                "Transformer 综述",
                "Transformer 架构在自然语言处理中广泛应用。",
                &[],
                Some(&SourceId::new("ref-0123456789abcdef").unwrap()),
                &[],
            )
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

    #[test]
    fn open_kb_missing_returns_none_and_init_succeeds() {
        let dir = temp_kb_dir();
        let llm = LlmConfig::default();

        // 知识库不存在：静默返回 None（正常状态）
        assert!(open_kb(dir.to_str().unwrap(), &llm).is_none());

        // init 后 wiki/index.db 落位，可正常打开
        let references = dir.join("references");
        fluen_kb::KbBuilder::new(&references).open().unwrap();
        assert!(open_kb(dir.to_str().unwrap(), &llm).is_some());

        let _ = fs::remove_dir_all(&dir);
    }
}
