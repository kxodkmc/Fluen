//! confluent agent_runtime 工具适配层。
//!
//! 将知识库操作适配为 [`confluent::agent_runtime::Tool`] / [`confluent::agent_runtime::ToolProvider`]，
//! 可直接注册到智能体的 [`confluent::agent_runtime::ToolRegistry`] 中。
//!
//! # 设计
//!
//! - 每个知识库操作对应一个 `Tool` 实现，持有 [`AsyncKnowledgeBase`] 的克隆。
//! - [`KnowledgeToolProvider`] 聚合全部工具，实现 `ToolProvider` trait。
//! - 工具 schema 的 JSON Schema 与 MCP server 保持一致，确保跨集成方式统一。
//! - 输出格式简洁：仅返回关键字段，content 按配置截断。
//!
//! # 示例
//!
//! ```no_run
//! # use fluen_knowledge::async_kb::AsyncKnowledgeBase;
//! # use fluen_knowledge::config::KnowledgeConfig;
//! # use fluen_knowledge::tools::KnowledgeToolProvider;
//! # use confluent::agent_runtime::{ToolProvider, ToolRegistry};
//! use std::sync::Arc;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let kb = AsyncKnowledgeBase::open("references")?;
//! let provider = KnowledgeToolProvider::new(kb, KnowledgeConfig::default());
//!
//! let registry = ToolRegistry::new();
//! registry.register_provider(&provider).await;
//!
//! // 现在 registry 中已注册 knowledge_query / knowledge_create_entry 等工具
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use confluent::agent_runtime::{InvocationContext, Tool, ToolError, ToolProvider, ToolSchema};

use crate::async_kb::{AsyncBatchQueryParams, AsyncKnowledgeBase, AsyncQueryParams};
use crate::config::{KnowledgeConfig, ALL_TOOL_SHORT_NAMES};
use crate::types::MetaQueryType;
use crate::wiki::{CreateEntryParams, EditEntryParams};

// ════════════════════════════════════════════════════════════════
// ToolProvider
// ════════════════════════════════════════════════════════════════

/// 知识库工具提供者。
///
/// 聚合全部知识库操作为 `Tool` 列表，供 `ToolRegistry` 注册。
pub struct KnowledgeToolProvider {
    kb: AsyncKnowledgeBase,
    config: KnowledgeConfig,
}

impl KnowledgeToolProvider {
    /// 构造提供者。
    pub fn new(kb: AsyncKnowledgeBase, config: KnowledgeConfig) -> Self {
        Self { kb, config }
    }

    /// 构造提供者（使用默认配置）。
    pub fn with_defaults(kb: AsyncKnowledgeBase) -> Self {
        Self::new(kb, KnowledgeConfig::default())
    }
}

#[async_trait]
impl ToolProvider for KnowledgeToolProvider {
    async fn list_tools(&self) -> Vec<Arc<dyn Tool>> {
        let mut tools: Vec<Arc<dyn Tool>> = Vec::new();

        for &short_name in ALL_TOOL_SHORT_NAMES {
            if !self.config.is_tool_enabled(short_name) {
                continue;
            }
            if let Some(tool) = build_tool(short_name, self.kb.clone(), self.config.clone()) {
                tools.push(tool);
            }
        }

        tools
    }
}

/// 根据短名构造对应的工具实例。
fn build_tool(
    short_name: &str,
    kb: AsyncKnowledgeBase,
    config: KnowledgeConfig,
) -> Option<Arc<dyn Tool>> {
    let full_name = config.tool_name(short_name);
    let tool: Arc<dyn Tool> = match short_name {
        "query" => Arc::new(QueryTool::new(kb, full_name, config)),
        "query_batch" => Arc::new(QueryBatchTool::new(kb, full_name, config)),
        "create_entry" => Arc::new(CreateEntryTool::new(kb, full_name, config)),
        "edit_entry" => Arc::new(EditEntryTool::new(kb, full_name, config)),
        "meta" => Arc::new(MetaTool::new(kb, full_name, config)),
        "get_entry" => Arc::new(GetEntryTool::new(kb, full_name, config)),
        "list_entries" => Arc::new(ListEntriesTool::new(kb, full_name, config)),
        "delete_entry" => Arc::new(DeleteEntryTool::new(kb, full_name, config)),
        _ => return None,
    };
    Some(tool)
}

// ════════════════════════════════════════════════════════════════
// 通用工具基座
// ════════════════════════════════════════════════════════════════

/// 工具内部共享状态。
struct ToolBase {
    kb: AsyncKnowledgeBase,
    schema: ToolSchema,
    config: KnowledgeConfig,
}

impl ToolBase {
    fn new(kb: AsyncKnowledgeBase, name: String, description: String, parameters: Value, config: KnowledgeConfig) -> Self {
        Self {
            kb,
            schema: ToolSchema {
                name,
                description,
                parameters,
            },
            config,
        }
    }
}

// ════════════════════════════════════════════════════════════════
// 工具一：query
// ════════════════════════════════════════════════════════════════

struct QueryTool {
    base: ToolBase,
}

impl QueryTool {
    fn new(kb: AsyncKnowledgeBase, name: String, config: KnowledgeConfig) -> Self {
        let description = "Query the knowledge base. Supports keyword (FTS5), semantic (embedding), and hybrid retrieval."
            .to_string();
        let parameters = json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Search query"},
                "method": {"type": "string", "enum": ["keyword", "semantic", "hybrid"], "default": "hybrid"},
                "wiki_type": {"type": "string", "enum": ["summary", "concept", "entity"]},
                "top_k": {"type": "integer", "default": 10},
                "include_content": {"type": "boolean", "default": false}
            },
            "required": ["query"]
        });
        Self {
            base: ToolBase::new(kb, name, description, parameters, config),
        }
    }
}

#[async_trait]
impl Tool for QueryTool {
    fn schema(&self) -> &ToolSchema {
        &self.base.schema
    }

    async fn invoke(
        &self,
        input: Value,
        _ctx: &InvocationContext,
    ) -> Result<Value, ToolError> {
        let params: AsyncQueryParams = serde_json::from_value(input)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let mut result = self
            .base
            .kb
            .query(params)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        // 截断 content
        if self.base.config.max_content_length > 0 {
            for m in &mut result.results {
                if let Some(content) = &m.content {
                    m.content = Some(self.base.config.truncate_content(content));
                }
            }
        }

        Ok(json!({
            "success": result.success,
            "method": result.retrieval_method_used.as_str(),
            "count": result.results.len(),
            "results": result.results.iter().map(|m| json!({
                "id": m.wiki_id,
                "type": m.wiki_type,
                "title": m.title,
                "score": format!("{:.3}", m.score),
                "content": m.content,
            })).collect::<Vec<_>>()
        }))
    }
}

// ════════════════════════════════════════════════════════════════
// 工具二：query_batch
// ════════════════════════════════════════════════════════════════

struct QueryBatchTool {
    base: ToolBase,
}

impl QueryBatchTool {
    fn new(kb: AsyncKnowledgeBase, name: String, config: KnowledgeConfig) -> Self {
        let description = "Batch query the knowledge base with multiple queries.".to_string();
        let parameters = json!({
            "type": "object",
            "properties": {
                "queries": {"type": "array", "items": {"type": "string"}},
                "method": {"type": "string", "enum": ["keyword", "semantic", "hybrid"], "default": "hybrid"},
                "wiki_type": {"type": "string", "enum": ["summary", "concept", "entity"]},
                "top_k": {"type": "integer", "default": 10},
                "include_content": {"type": "boolean", "default": false}
            },
            "required": ["queries"]
        });
        Self {
            base: ToolBase::new(kb, name, description, parameters, config),
        }
    }
}

#[async_trait]
impl Tool for QueryBatchTool {
    fn schema(&self) -> &ToolSchema {
        &self.base.schema
    }

    async fn invoke(&self, input: Value, _ctx: &InvocationContext) -> Result<Value, ToolError> {
        let params: AsyncBatchQueryParams = serde_json::from_value(input)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let result = self
            .base
            .kb
            .query_batch(params)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        Ok(json!({
            "success": result.success,
            "results": result.results.iter().map(|item| json!({
                "query": item.query,
                "method": item.retrieval_method_used.as_str(),
                "count": item.matches.len(),
                "matches": item.matches.iter().map(|m| json!({
                    "id": m.wiki_id,
                    "type": m.wiki_type,
                    "title": m.title,
                    "score": format!("{:.3}", m.score),
                })).collect::<Vec<_>>()
            })).collect::<Vec<_>>()
        }))
    }
}

// ════════════════════════════════════════════════════════════════
// 工具三：create_entry
// ════════════════════════════════════════════════════════════════

struct CreateEntryTool {
    base: ToolBase,
}

impl CreateEntryTool {
    fn new(kb: AsyncKnowledgeBase, name: String, config: KnowledgeConfig) -> Self {
        let description = "Create a new knowledge base entry. Auto-deduplicates.".to_string();
        let parameters = json!({
            "type": "object",
            "properties": {
                "wiki_type": {"type": "string", "enum": ["summary", "concept", "entity"]},
                "title": {"type": "string"},
                "content": {"type": "string"},
                "source": {"type": "string"},
                "authors": {"type": "array", "items": {"type": "string"}},
                "tags": {"type": "array", "items": {"type": "string"}},
                "relations": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["wiki_type", "title", "content"]
        });
        Self {
            base: ToolBase::new(kb, name, description, parameters, config),
        }
    }
}

#[async_trait]
impl Tool for CreateEntryTool {
    fn schema(&self) -> &ToolSchema {
        &self.base.schema
    }

    async fn invoke(&self, input: Value, _ctx: &InvocationContext) -> Result<Value, ToolError> {
        let params: CreateEntryParams = serde_json::from_value(input)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let result = self
            .base
            .kb
            .create_entry(params)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        Ok(json!({
            "success": result.success,
            "wiki_id": result.wiki_id,
            "file_path": result.file_path,
            "merged": result.merged,
            "tags": result.tags_generated.iter().map(|t| json!({
                "name": t.name, "tag_id": t.tag_id
            })).collect::<Vec<_>>()
        }))
    }
}

// ════════════════════════════════════════════════════════════════
// 工具四：edit_entry
// ════════════════════════════════════════════════════════════════

struct EditEntryTool {
    base: ToolBase,
}

impl EditEntryTool {
    fn new(kb: AsyncKnowledgeBase, name: String, config: KnowledgeConfig) -> Self {
        let description = "Edit an existing knowledge base entry.".to_string();
        let parameters = json!({
            "type": "object",
            "properties": {
                "wiki_id": {"type": "string"},
                "edits": {"type": "array", "items": {"type": "object"}},
                "add_relations": {"type": "array", "items": {"type": "string"}},
                "add_tags": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["wiki_id"]
        });
        Self {
            base: ToolBase::new(kb, name, description, parameters, config),
        }
    }
}

#[async_trait]
impl Tool for EditEntryTool {
    fn schema(&self) -> &ToolSchema {
        &self.base.schema
    }

    async fn invoke(&self, input: Value, _ctx: &InvocationContext) -> Result<Value, ToolError> {
        let params: EditEntryParams = serde_json::from_value(input)
            .map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let result = self
            .base
            .kb
            .edit_entry(params)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        Ok(json!({
            "success": result.success,
            "wiki_id": result.wiki_id,
            "file_path": result.file_path,
            "updated_time": result.updated_time,
            "edit_results": result.edit_results.iter().map(|r| json!({
                "type": r.edit_type, "success": r.success
            })).collect::<Vec<_>>()
        }))
    }
}

// ════════════════════════════════════════════════════════════════
// 工具五：meta
// ════════════════════════════════════════════════════════════════

struct MetaTool {
    base: ToolBase,
}

impl MetaTool {
    fn new(kb: AsyncKnowledgeBase, name: String, config: KnowledgeConfig) -> Self {
        let description = "Query knowledge base metadata (overview, tags, or recent entries)."
            .to_string();
        let parameters = json!({
            "type": "object",
            "properties": {
                "query_type": {"type": "string", "enum": ["overview", "tags", "recent"], "default": "overview"},
                "limit": {"type": "integer", "default": 20}
            }
        });
        Self {
            base: ToolBase::new(kb, name, description, parameters, config),
        }
    }
}

#[async_trait]
impl Tool for MetaTool {
    fn schema(&self) -> &ToolSchema {
        &self.base.schema
    }

    async fn invoke(&self, input: Value, _ctx: &InvocationContext) -> Result<Value, ToolError> {
        let query_type_str = input
            .get("query_type")
            .and_then(|v| v.as_str())
            .unwrap_or("overview");
        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(self.base.config.default_recent_limit as u64) as usize;

        let query_type = match query_type_str {
            "tags" => MetaQueryType::Tags,
            "recent" => MetaQueryType::Recent,
            _ => MetaQueryType::Overview,
        };

        let result = self
            .base
            .kb
            .meta(query_type, limit)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        Ok(json!({
            "success": result.success,
            "data": {
                "total_entries": result.data.total_entries,
                "total_tags": result.data.total_tags,
                "embedding_enabled": result.data.embedding_enabled,
                "tags": result.data.tags,
                "recent_entries": result.data.recent_entries,
            }
        }))
    }
}

// ════════════════════════════════════════════════════════════════
// 工具六：get_entry
// ════════════════════════════════════════════════════════════════

struct GetEntryTool {
    base: ToolBase,
}

impl GetEntryTool {
    fn new(kb: AsyncKnowledgeBase, name: String, config: KnowledgeConfig) -> Self {
        let description = "Get full entry details by wiki ID.".to_string();
        let parameters = json!({
            "type": "object",
            "properties": {
                "wiki_id": {"type": "string"}
            },
            "required": ["wiki_id"]
        });
        Self {
            base: ToolBase::new(kb, name, description, parameters, config),
        }
    }
}

#[async_trait]
impl Tool for GetEntryTool {
    fn schema(&self) -> &ToolSchema {
        &self.base.schema
    }

    async fn invoke(&self, input: Value, _ctx: &InvocationContext) -> Result<Value, ToolError> {
        let wiki_id = input
            .get("wiki_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("missing 'wiki_id'".into()))?
            .to_string();

        let entry = self
            .base
            .kb
            .get_entry(wiki_id)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        match entry {
            Some(detail) => {
                let mut entry = detail.entry;
                if self.base.config.max_content_length > 0 && !entry.content.is_empty() {
                    entry.content = self.base.config.truncate_content(&entry.content);
                }
                Ok(json!({
                    "entry": entry,
                    "tag_titles": detail.tag_titles,
                    "relation_titles": detail.relation_titles,
                }))
            }
            None => Ok(json!({"found": false})),
        }
    }
}

// ════════════════════════════════════════════════════════════════
// 工具七：list_entries
// ════════════════════════════════════════════════════════════════

struct ListEntriesTool {
    base: ToolBase,
}

impl ListEntriesTool {
    fn new(kb: AsyncKnowledgeBase, name: String, config: KnowledgeConfig) -> Self {
        let description = "List all knowledge base entries (without content).".to_string();
        let parameters = json!({"type": "object", "properties": {}});
        Self {
            base: ToolBase::new(kb, name, description, parameters, config),
        }
    }
}

#[async_trait]
impl Tool for ListEntriesTool {
    fn schema(&self) -> &ToolSchema {
        &self.base.schema
    }

    async fn invoke(&self, _input: Value, _ctx: &InvocationContext) -> Result<Value, ToolError> {
        let entries = self
            .base
            .kb
            .list_entries()
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        Ok(json!({
            "count": entries.len(),
            "entries": entries.iter().map(|e| json!({
                "id": e.id,
                "type": e.wiki_type,
                "title": e.title,
                "tags": e.tags,
                "updated": e.updated,
            })).collect::<Vec<_>>()
        }))
    }
}

// ════════════════════════════════════════════════════════════════
// 工具八：delete_entry
// ════════════════════════════════════════════════════════════════

struct DeleteEntryTool {
    base: ToolBase,
}

impl DeleteEntryTool {
    fn new(kb: AsyncKnowledgeBase, name: String, config: KnowledgeConfig) -> Self {
        let description = "Delete a knowledge base entry by wiki ID.".to_string();
        let parameters = json!({
            "type": "object",
            "properties": {
                "wiki_id": {"type": "string"}
            },
            "required": ["wiki_id"]
        });
        Self {
            base: ToolBase::new(kb, name, description, parameters, config),
        }
    }
}

#[async_trait]
impl Tool for DeleteEntryTool {
    fn schema(&self) -> &ToolSchema {
        &self.base.schema
    }

    async fn invoke(&self, input: Value, _ctx: &InvocationContext) -> Result<Value, ToolError> {
        let wiki_id = input
            .get("wiki_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("missing 'wiki_id'".into()))?
            .to_string();

        self.base
            .kb
            .delete_entry(wiki_id)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        Ok(json!({"success": true}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use confluent::agent_runtime::ToolRegistry;

    #[tokio::test]
    async fn test_tool_provider_registration() {
        let dir = tempfile::TempDir::new().unwrap();
        let kb = AsyncKnowledgeBase::init(dir.path()).unwrap();
        let provider = KnowledgeToolProvider::with_defaults(kb);

        let tools = provider.list_tools().await;
        assert_eq!(tools.len(), 8);

        let names: Vec<&str> = tools
            .iter()
            .map(|t| t.schema().name.as_str())
            .collect();
        assert!(names.contains(&"knowledge_query"));
        assert!(names.contains(&"knowledge_create_entry"));
        assert!(names.contains(&"knowledge_delete_entry"));
    }

    #[tokio::test]
    async fn test_tool_provider_config_filter() {
        let dir = tempfile::TempDir::new().unwrap();
        let kb = AsyncKnowledgeBase::init(dir.path()).unwrap();
        let config = KnowledgeConfig::builder()
            .enabled_tools(vec!["query".into(), "meta".into()])
            .build();
        let provider = KnowledgeToolProvider::new(kb, config);

        let tools = provider.list_tools().await;
        assert_eq!(tools.len(), 2);
    }

    #[tokio::test]
    async fn test_tool_registry_integration() {
        let dir = tempfile::TempDir::new().unwrap();
        let kb = AsyncKnowledgeBase::init(dir.path()).unwrap();
        let provider = KnowledgeToolProvider::with_defaults(kb);

        let registry = ToolRegistry::new();
        registry.register_provider(&provider).await;

        let schemas = registry.list_schemas();
        assert_eq!(schemas.len(), 8);

        // 验证能通过 registry 查找工具
        let query_tool = registry.get("knowledge_query");
        assert!(query_tool.is_some());

        let create_tool = registry.get("knowledge_create_entry");
        assert!(create_tool.is_some());
    }

    #[tokio::test]
    async fn test_tool_invoke_query() {
        let dir = tempfile::TempDir::new().unwrap();
        let kb = AsyncKnowledgeBase::init(dir.path()).unwrap();
        let provider = KnowledgeToolProvider::with_defaults(kb);

        let registry = ToolRegistry::new();
        registry.register_provider(&provider).await;

        // 先创建条目
        let ctx = InvocationContext {
            run_id: "test".into(),
            agent_id: "test".into(),
            tool_call_id: "test".into(),
            cancel_token: tokio_util::sync::CancellationToken::new(),
            timeout: std::time::Duration::from_secs(30),
        };

        let create_result = registry
            .invoke(
                "knowledge_create_entry",
                &json!({
                    "wiki_type": "concept",
                    "title": "Tool Test",
                    "content": "Testing confluent agent_runtime tool integration."
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(create_result["success"], true);

        // 查询
        let query_result = registry
            .invoke(
                "knowledge_query",
                &json!({
                    "query": "integration",
                    "method": "keyword"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(query_result["success"], true);
        assert!(query_result["count"].as_u64().unwrap() > 0);
    }
}
