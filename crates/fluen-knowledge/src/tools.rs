//! referee-ai 工具适配层。
//!
//! 将知识库操作适配为 [`referee_ai::tool::Tool`]，
//! 可直接注册到智能体的 [`referee_ai::tool::ToolRegistry`] 中。
//!
//! # 设计
//!
//! - 每个知识库操作对应一个 `Tool` 实现，持有 [`AsyncKnowledgeBase`] 的克隆。
//! - [`KnowledgeToolProvider`] 聚合全部工具，提供 `list_tools()` 便捷方法
//!   供调用方逐个注册到 `ToolRegistry`。
//! - 工具 schema 的 JSON Schema 与 MCP server 保持一致，确保跨集成方式统一。
//! - 输出格式简洁：仅返回关键字段，content 按配置截断。
//!
//! # 示例
//!
//! ```no_run
//! # use fluen_knowledge::async_kb::AsyncKnowledgeBase;
//! # use fluen_knowledge::config::KnowledgeConfig;
//! # use fluen_knowledge::tools::KnowledgeToolProvider;
//! # use referee_ai::tool::ToolRegistry;
//! use std::sync::Arc;
//!
//! # fn example() -> anyhow::Result<()> {
//! let kb = AsyncKnowledgeBase::open("references")?;
//! let provider = KnowledgeToolProvider::new(kb, KnowledgeConfig::default());
//!
//! let registry = ToolRegistry::with_defaults();
//! for tool in provider.list_tools() {
//!     registry.register(tool)?;
//! }
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use referee_ai::tool::{Tool, ToolContext, ToolError, ToolOutput};
use serde_json::{json, Value};

use crate::async_kb::{AsyncBatchQueryParams, AsyncKnowledgeBase, AsyncQueryParams};
use crate::config::{KnowledgeConfig, ALL_TOOL_SHORT_NAMES};
use crate::types::MetaQueryType;
use crate::wiki::{CreateEntryParams, EditEntryParams};

// ════════════════════════════════════════════════════════════════
// KnowledgeToolProvider
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

    /// 列出全部已启用的工具，供调用方逐个注册到 `ToolRegistry`。
    pub fn list_tools(&self) -> Vec<Arc<dyn Tool>> {
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
    name: String,
    description: String,
    parameters: Value,
    config: KnowledgeConfig,
}

impl ToolBase {
    fn new(kb: AsyncKnowledgeBase, name: String, description: String, parameters: Value, config: KnowledgeConfig) -> Self {
        Self {
            kb,
            name,
            description,
            parameters,
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
                "query": {"type": "string", "description": "搜索查询文本（关键词、条目标题片段或语义描述）"},
                "method": {"type": "string", "enum": ["keyword", "semantic", "hybrid"], "default": "hybrid", "description": "检索方式：keyword=FTS5关键词，semantic=向量语义，hybrid=混合（默认）"},
                "wiki_type": {"type": "string", "enum": ["summary", "concept", "entity"], "description": "限定条目类型（可选，不填则检索全部类型）"},
                "top_k": {"type": "integer", "default": 10, "description": "返回结果数上限"},
                "include_content": {"type": "boolean", "default": false, "description": "是否在结果中包含条目正文"}
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
    fn name(&self) -> &str {
        &self.base.name
    }

    fn description(&self) -> &str {
        &self.base.description
    }

    fn input_schema(&self) -> Value {
        self.base.parameters.clone()
    }

    async fn execute(&self, _ctx: ToolContext, args: Value) -> Result<ToolOutput, ToolError> {
        let params: AsyncQueryParams = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let mut result = self
            .base
            .kb
            .query(params)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        // 截断 content
        if self.base.config.max_content_length > 0 {
            for m in &mut result.results {
                if let Some(content) = &m.content {
                    m.content = Some(self.base.config.truncate_content(content));
                }
            }
        }

        let output = json!({
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
        });
        Ok(ToolOutput::from_json(&output))
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
                "queries": {"type": "array", "items": {"type": "string"}, "description": "搜索查询文本列表"},
                "method": {"type": "string", "enum": ["keyword", "semantic", "hybrid"], "default": "hybrid", "description": "检索方式"},
                "wiki_type": {"type": "string", "enum": ["summary", "concept", "entity"], "description": "限定条目类型（可选）"},
                "top_k": {"type": "integer", "default": 10, "description": "每个查询返回结果数上限"},
                "include_content": {"type": "boolean", "default": false, "description": "是否包含正文"}
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
    fn name(&self) -> &str {
        &self.base.name
    }

    fn description(&self) -> &str {
        &self.base.description
    }

    fn input_schema(&self) -> Value {
        self.base.parameters.clone()
    }

    async fn execute(&self, _ctx: ToolContext, args: Value) -> Result<ToolOutput, ToolError> {
        let params: AsyncBatchQueryParams = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let result = self
            .base
            .kb
            .query_batch(params)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        let output = json!({
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
        });
        Ok(ToolOutput::from_json(&output))
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
                "wiki_type": {"type": "string", "enum": ["summary", "concept", "entity"], "description": "条目类型：summary=文献综述，concept=学术概念，entity=人物/机构/项目等实体"},
                "title": {"type": "string", "description": "条目标题（concept 用全称如\"数智化技术\"，entity 用全名）"},
                "content": {
                    "type": "string",
                    "description": "条目正文（Markdown）。**禁止包含 `## 关联页面` 区**——关联关系通过 relations 字段或 edit_entry 工具建立，不得在正文中手写。正文应包含概念定义/实体身份 + 在文献中的应用/贡献。"
                },
                "source": {"type": "string", "description": "仅 summary：源文献路径，格式 `raw/ref-xxxxxxxxxxxxxxxx.pdf`"},
                "authors": {"type": "array", "items": {"type": "string"}, "description": "仅 summary：作者 wikiID 列表"},
                "tags": {"type": "array", "items": {"type": "string"}, "description": "标签名称列表（非 ID，后端自动 upsert 为 tagID）"},
                "relations": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "关联条目的 wikiID 列表（格式 `wiki-xxxxxxxxxxxxxxxx`，**严禁使用标题或描述文本**）。若暂无关联请留空，后续由 edit_entry 工具添加。"
                }
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
    fn name(&self) -> &str {
        &self.base.name
    }

    fn description(&self) -> &str {
        &self.base.description
    }

    fn input_schema(&self) -> Value {
        self.base.parameters.clone()
    }

    async fn execute(&self, _ctx: ToolContext, args: Value) -> Result<ToolOutput, ToolError> {
        let params: CreateEntryParams = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let result = self
            .base
            .kb
            .create_entry(params)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        let output = json!({
            "success": result.success,
            "wiki_id": result.wiki_id,
            "file_path": result.file_path,
            "merged": result.merged,
            "tags": result.tags_generated.iter().map(|t| json!({
                "name": t.name, "tag_id": t.tag_id
            })).collect::<Vec<_>>()
        });
        Ok(ToolOutput::from_json(&output))
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
                "wiki_id": {"type": "string", "description": "待编辑条目的 wikiID（格式 `wiki-xxxxxxxxxxxxxxxx`）"},
                "edits": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "type": {"type": "string", "enum": ["search_replace", "insert_after"], "description": "操作类型"},
                            "search": {"type": "string", "description": "search_replace 模式：待查找的精确文本"},
                            "replace": {"type": "string", "description": "search_replace 模式：替换文本"},
                            "anchor": {"type": "string", "description": "insert_after 模式：锚点文本（在其后插入）"},
                            "content": {"type": "string", "description": "insert_after 模式：待插入的内容"}
                        },
                        "required": ["type"]
                    },
                    "description": "正文编辑操作列表。**禁止用 insert_after 插入 `## 关联页面` 区**——关联关系通过 add_relations 字段建立。"
                },
                "add_relations": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "追加到关联页面的 wikiID 列表（格式 `wiki-xxxxxxxxxxxxxxxx`，**严禁使用标题或描述文本**）。关联是双向的，需对双方分别调用 edit_entry。"
                },
                "add_tags": {"type": "array", "items": {"type": "string"}, "description": "追加的标签名称列表（非 ID，后端自动 upsert）"}
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
    fn name(&self) -> &str {
        &self.base.name
    }

    fn description(&self) -> &str {
        &self.base.description
    }

    fn input_schema(&self) -> Value {
        self.base.parameters.clone()
    }

    async fn execute(&self, _ctx: ToolContext, args: Value) -> Result<ToolOutput, ToolError> {
        let params: EditEntryParams = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let result = self
            .base
            .kb
            .edit_entry(params)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        let output = json!({
            "success": result.success,
            "wiki_id": result.wiki_id,
            "file_path": result.file_path,
            "updated_time": result.updated_time,
            "edit_results": result.edit_results.iter().map(|r| json!({
                "type": r.edit_type, "success": r.success
            })).collect::<Vec<_>>()
        });
        Ok(ToolOutput::from_json(&output))
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
                "query_type": {"type": "string", "enum": ["overview", "tags", "recent"], "default": "overview", "description": "查询类型：overview=总览统计，tags=所有标签，recent=最近条目"},
                "limit": {"type": "integer", "default": 20, "description": "返回结果数上限（recent 时有效）"}
            }
        });
        Self {
            base: ToolBase::new(kb, name, description, parameters, config),
        }
    }
}

#[async_trait]
impl Tool for MetaTool {
    fn name(&self) -> &str {
        &self.base.name
    }

    fn description(&self) -> &str {
        &self.base.description
    }

    fn input_schema(&self) -> Value {
        self.base.parameters.clone()
    }

    async fn execute(&self, _ctx: ToolContext, args: Value) -> Result<ToolOutput, ToolError> {
        let query_type_str = args
            .get("query_type")
            .and_then(|v| v.as_str())
            .unwrap_or("overview");
        let limit = args
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
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        let output = json!({
            "success": result.success,
            "data": {
                "total_entries": result.data.total_entries,
                "total_tags": result.data.total_tags,
                "embedding_enabled": result.data.embedding_enabled,
                "tags": result.data.tags,
                "recent_entries": result.data.recent_entries,
            }
        });
        Ok(ToolOutput::from_json(&output))
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
        let description = "Get full entry details by wiki id.".to_string();
        let parameters = json!({
            "type": "object",
            "properties": {
                "wiki_id": {"type": "string", "description": "条目 wikiID（格式 `wiki-xxxxxxxxxxxxxxxx`）"}
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
    fn name(&self) -> &str {
        &self.base.name
    }

    fn description(&self) -> &str {
        &self.base.description
    }

    fn input_schema(&self) -> Value {
        self.base.parameters.clone()
    }

    async fn execute(&self, _ctx: ToolContext, args: Value) -> Result<ToolOutput, ToolError> {
        let wiki_id = args
            .get("wiki_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments("missing 'wiki_id'".into()))?
            .to_string();

        let entry = self
            .base
            .kb
            .get_entry(wiki_id)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        match entry {
            Some(detail) => {
                let mut entry = detail.entry;
                if self.base.config.max_content_length > 0 && !entry.content.is_empty() {
                    entry.content = self.base.config.truncate_content(&entry.content);
                }
                let output = json!({
                    "entry": entry,
                    "tag_titles": detail.tag_titles,
                    "relation_titles": detail.relation_titles,
                });
                Ok(ToolOutput::from_json(&output))
            }
            None => {
                let output = json!({"found": false});
                Ok(ToolOutput::from_json(&output))
            }
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
        let parameters = json!({
            "type": "object",
            "properties": {},
            "description": "列出知识库中所有条目（仅元信息，不含正文）。用于在 Planning 阶段查询已有条目以避免重复创建。"
        });
        Self {
            base: ToolBase::new(kb, name, description, parameters, config),
        }
    }
}

#[async_trait]
impl Tool for ListEntriesTool {
    fn name(&self) -> &str {
        &self.base.name
    }

    fn description(&self) -> &str {
        &self.base.description
    }

    fn input_schema(&self) -> Value {
        self.base.parameters.clone()
    }

    async fn execute(&self, _ctx: ToolContext, _args: Value) -> Result<ToolOutput, ToolError> {
        let entries = self
            .base
            .kb
            .list_entries()
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        let output = json!({
            "count": entries.len(),
            "entries": entries.iter().map(|e| json!({
                "id": e.id,
                "type": e.wiki_type,
                "title": e.title,
                "tags": e.tags,
                "updated": e.updated,
            })).collect::<Vec<_>>()
        });
        Ok(ToolOutput::from_json(&output))
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
        let description = "Delete a knowledge base entry by wiki id.".to_string();
        let parameters = json!({
            "type": "object",
            "properties": {
                "wiki_id": {"type": "string", "description": "待删除条目的 wikiID（格式 `wiki-xxxxxxxxxxxxxxxx`）"}
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
    fn name(&self) -> &str {
        &self.base.name
    }

    fn description(&self) -> &str {
        &self.base.description
    }

    fn input_schema(&self) -> Value {
        self.base.parameters.clone()
    }

    async fn execute(&self, _ctx: ToolContext, args: Value) -> Result<ToolOutput, ToolError> {
        let wiki_id = args
            .get("wiki_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments("missing 'wiki_id'".into()))?
            .to_string();

        self.base
            .kb
            .delete_entry(wiki_id)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        let output = json!({"success": true});
        Ok(ToolOutput::from_json(&output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use referee_ai::tool::ToolRegistry;

    #[tokio::test]
    async fn test_tool_provider_registration() {
        let dir = tempfile::TempDir::new().unwrap();
        let kb = AsyncKnowledgeBase::init(dir.path()).unwrap();
        let provider = KnowledgeToolProvider::with_defaults(kb);

        let tools = provider.list_tools();
        assert_eq!(tools.len(), 8);

        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
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

        let tools = provider.list_tools();
        assert_eq!(tools.len(), 2);
    }

    #[tokio::test]
    async fn test_tool_registry_integration() {
        let dir = tempfile::TempDir::new().unwrap();
        let kb = AsyncKnowledgeBase::init(dir.path()).unwrap();
        let provider = KnowledgeToolProvider::with_defaults(kb);

        let registry = ToolRegistry::with_defaults();
        for tool in provider.list_tools() {
            registry.register(tool).unwrap();
        }

        // 验证能通过 registry 查找工具
        assert!(registry.get("knowledge_query").is_some());
        assert!(registry.get("knowledge_create_entry").is_some());
    }

    #[tokio::test]
    async fn test_tool_invoke_query() {
        let dir = tempfile::TempDir::new().unwrap();
        let kb = AsyncKnowledgeBase::init(dir.path()).unwrap();
        let provider = KnowledgeToolProvider::with_defaults(kb);

        let registry = ToolRegistry::with_defaults();
        for tool in provider.list_tools() {
            registry.register(tool).unwrap();
        }

        let ctx = ToolContext {
            tool_call_id: "test".into(),
            session_id: uuid::Uuid::new_v4(),
            turn_id: 0,
            kernel: None,
            store: None,
            wait: true,
            peer_depth: 0,
        };

        // 先创建条目
        let create_result = registry
            .get("knowledge_create_entry")
            .unwrap()
            .execute(
                ctx.clone(),
                json!({
                    "wiki_type": "concept",
                    "title": "Tool Test",
                    "content": "Testing referee-ai tool integration."
                }),
            )
            .await
            .unwrap();
        assert!(create_result.content.contains("success"));
        assert!(create_result.content.contains("true"));

        // 查询
        let query_result = registry
            .get("knowledge_query")
            .unwrap()
            .execute(
                ctx,
                json!({
                    "query": "integration",
                    "method": "keyword"
                }),
            )
            .await
            .unwrap();
        assert!(query_result.content.contains("success"));
        assert!(query_result.content.contains("true"));
    }
}
