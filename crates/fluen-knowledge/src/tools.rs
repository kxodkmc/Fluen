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

/// 保留三位小数的数值评分（score 输出统一为数值而非格式化字符串）。
fn rounded_score(score: f64) -> f64 {
    (score * 1000.0).round() / 1000.0
}

// ════════════════════════════════════════════════════════════════
// 工具一：query
// ════════════════════════════════════════════════════════════════

struct QueryTool {
    base: ToolBase,
}

impl QueryTool {
    fn new(kb: AsyncKnowledgeBase, name: String, config: KnowledgeConfig) -> Self {
        let description = "检索知识库：支持关键词（FTS5）、语义向量与混合三种检索方式，返回最相关的知识条目列表。"
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
                "score": rounded_score(m.score),
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
        let description = "批量检索知识库：以多个查询文本一次调用，逐个返回各自的最相关条目。".to_string();
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
                    "score": rounded_score(m.score),
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
        let description = "编辑已有知识库条目：对正文执行精确替换或锚点插入，并可追加关联与标签。".to_string();
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
        // schema 表达不了「按 type 条件必填」，这里前置校验给出明确报错，
        // 避免参数错误流入执行层产生难定位的失败。
        validate_edits(&args)?;

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

/// 校验 `edits[]` 的条件必填约束：
///
/// - `search_replace` 必须提供字符串字段 `search` 与 `replace`；
/// - `insert_after` 必须提供字符串字段 `anchor` 与 `content`；
/// - `type` 只允许上述两种取值。
fn validate_edits(args: &Value) -> Result<(), ToolError> {
    let Some(edits) = args.get("edits").and_then(|v| v.as_array()) else {
        return Ok(());
    };
    for (i, edit) in edits.iter().enumerate() {
        let ty = edit.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let required = match ty {
            "search_replace" => ["search", "replace"].as_slice(),
            "insert_after" => ["anchor", "content"].as_slice(),
            other => {
                return Err(ToolError::InvalidArguments(format!(
                    "edits[{i}].type 取值非法: {other:?}（可选 search_replace / insert_after）"
                )));
            }
        };
        for field in required {
            if edit.get(field).and_then(|v| v.as_str()).is_none() {
                return Err(ToolError::InvalidArguments(format!(
                    "edits[{i}] 类型为 {ty} 时必须提供字符串字段 '{field}'"
                )));
            }
        }
    }
    Ok(())
}

// ════════════════════════════════════════════════════════════════
// 工具五：meta
// ════════════════════════════════════════════════════════════════

struct MetaTool {
    base: ToolBase,
}

impl MetaTool {
    fn new(kb: AsyncKnowledgeBase, name: String, config: KnowledgeConfig) -> Self {
        let description = "查询知识库元信息：overview=总览统计，tags=全部标签，recent=最近条目。".to_string();
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

        // 按 query_type 只返回对应字段，避免全量冗余
        let data = match query_type {
            MetaQueryType::Tags => json!({ "tags": result.data.tags }),
            MetaQueryType::Recent => json!({ "recent_entries": result.data.recent_entries }),
            MetaQueryType::Overview => json!({
                "total_entries": result.data.total_entries,
                "total_tags": result.data.total_tags,
                "embedding_enabled": result.data.embedding_enabled,
            }),
        };

        let output = json!({
            "success": result.success,
            "query_type": query_type_str,
            "data": data
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
        let description = "按 wikiID 获取条目完整详情（含正文、标签与关联的标题）。".to_string();
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
                    "found": true,
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
        let description = "列出知识库条目（仅元信息，不含正文），支持 limit 限制返回数量。用于在 Planning 阶段查询已有条目以避免重复创建。".to_string();
        let parameters = json!({
            "type": "object",
            "properties": {
                "limit": {"type": "integer", "default": 50, "description": "返回条目数上限（防止大知识库刷爆上下文）；超出时 truncated=true，可用更大的 limit 分页查看"}
            },
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

    async fn execute(&self, _ctx: ToolContext, args: Value) -> Result<ToolOutput, ToolError> {
        // limit 缺省 50；非法值（非正整数）按缺省处理
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .filter(|&v| v > 0)
            .map(|v| v as usize)
            .unwrap_or(50);

        let entries = self
            .base
            .kb
            .list_entries()
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        let total = entries.len();
        let truncated = total > limit;
        let entries = &entries[..limit.min(total)];

        let output = json!({
            "success": true,
            "count": entries.len(),
            "total": total,
            "truncated": truncated,
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
        let description = "按 wikiID 删除知识库条目（含其文件与索引记录）。".to_string();
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

        // 与 get_entry 的未找到语义统一：条目不存在时返回 found:false 而非报错
        if self
            .base
            .kb
            .get_entry(wiki_id.clone())
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?
            .is_none()
        {
            return Ok(ToolOutput::from_json(&json!({ "found": false })));
        }

        self.base
            .kb
            .delete_entry(wiki_id.clone())
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        let output = json!({"success": true, "wiki_id": wiki_id});
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

    // ── 输出规范测试 ──────────────────────────────────────────

    fn ctx() -> ToolContext {
        ToolContext {
            tool_call_id: "test".into(),
            session_id: uuid::Uuid::new_v4(),
            turn_id: 0,
            kernel: None,
            store: None,
            wait: true,
            peer_depth: 0,
        }
    }

    /// 解析工具输出为 JSON。
    fn output_json(output: ToolOutput) -> Value {
        serde_json::from_str(&output.content).unwrap()
    }

    async fn make_provider_with_entries(count: usize) -> KnowledgeToolProvider {
        let dir = tempfile::TempDir::new().unwrap();
        let kb = AsyncKnowledgeBase::init(dir.path()).unwrap();
        for i in 0..count {
            kb.create_entry(CreateEntryParams {
                wiki_type: crate::types::WikiType::Concept,
                title: format!("条目{i}"),
                content: format!("测试条目内容 {i}，包含关键词 alpha。"),
                source: None,
                authors: vec![],
                tags: vec![],
                relations: vec![],
            })
            .await
            .unwrap();
        }
        KnowledgeToolProvider::with_defaults(kb)
    }

    #[tokio::test]
    async fn query_score_is_number_not_string() {
        let provider = make_provider_with_entries(1).await;
        let tool = provider.list_tools().into_iter()
            .find(|t| t.name() == "knowledge_query")
            .unwrap();

        let out = output_json(
            tool.execute(ctx(), json!({ "query": "alpha", "method": "keyword" }))
                .await
                .unwrap(),
        );
        let first = &out["results"][0];
        assert!(first["score"].is_number(), "score 应为数值: {}", first["score"]);
    }

    #[tokio::test]
    async fn list_entries_limit_and_truncated() {
        let provider = make_provider_with_entries(3).await;
        let tool = provider.list_tools().into_iter()
            .find(|t| t.name() == "knowledge_list_entries")
            .unwrap();

        // limit=2：返回 2 条，total=3，truncated=true
        let out = output_json(tool.execute(ctx(), json!({ "limit": 2 })).await.unwrap());
        assert_eq!(out["success"], true);
        assert_eq!(out["count"], 2);
        assert_eq!(out["total"], 3);
        assert_eq!(out["truncated"], true);

        // 缺省：全部返回，truncated=false
        let out = output_json(tool.execute(ctx(), json!({})).await.unwrap());
        assert_eq!(out["count"], 3);
        assert_eq!(out["truncated"], false);
    }

    #[tokio::test]
    async fn meta_returns_only_fields_for_query_type() {
        let provider = make_provider_with_entries(0).await;
        let tool = provider.list_tools().into_iter()
            .find(|t| t.name() == "knowledge_meta")
            .unwrap();

        // overview：只含统计字段
        let out = output_json(tool.execute(ctx(), json!({ "query_type": "overview" })).await.unwrap());
        assert_eq!(out["query_type"], "overview");
        assert!(out["data"]["total_entries"].is_u64());
        assert!(out["data"].get("tags").is_none(), "overview 不应返回 tags");
        assert!(out["data"].get("recent_entries").is_none());

        // tags：只含标签列表
        let out = output_json(tool.execute(ctx(), json!({ "query_type": "tags" })).await.unwrap());
        assert!(out["data"].get("tags").is_some());
        assert!(out["data"].get("total_entries").is_none());
    }

    #[tokio::test]
    async fn edit_entry_validates_conditional_required_fields() {
        let provider = make_provider_with_entries(1).await;
        let tool = provider.list_tools().into_iter()
            .find(|t| t.name() == "knowledge_edit_entry")
            .unwrap();

        // search_replace 缺 replace → 前置校验报错
        let err = tool
            .execute(ctx(), json!({
                "wiki_id": "wiki-0000000000000000",
                "edits": [{ "type": "search_replace", "search": "alpha" }]
            }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)), "{err:?}");
        assert!(err.to_string().contains("'replace'"));

        // 非法 type → 前置校验报错
        let err = tool
            .execute(ctx(), json!({
                "wiki_id": "wiki-0000000000000000",
                "edits": [{ "type": "bogus" }]
            }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));
    }

    #[tokio::test]
    async fn delete_entry_unifies_not_found_semantics() {
        let provider = make_provider_with_entries(1).await;
        let tool = provider.list_tools().into_iter()
            .find(|t| t.name() == "knowledge_delete_entry")
            .unwrap();

        // 不存在：found:false（与 get_entry 一致），不报错
        let out = output_json(
            tool.execute(ctx(), json!({ "wiki_id": "wiki-doesnotexist0000" }))
                .await
                .unwrap(),
        );
        assert_eq!(out["found"], false);

        // 存在：删除成功并回显 wiki_id
        let wiki_id = {
            // 通过 list 拿一个真实 wiki_id
            let list_tool = provider.list_tools().into_iter()
                .find(|t| t.name() == "knowledge_list_entries")
                .unwrap();
            let out = output_json(list_tool.execute(ctx(), json!({})).await.unwrap());
            out["entries"][0]["id"].as_str().unwrap().to_string()
        };
        let out = output_json(
            tool.execute(ctx(), json!({ "wiki_id": wiki_id })).await.unwrap(),
        );
        assert_eq!(out["success"], true);
        assert_eq!(out["wiki_id"], wiki_id);
    }
}
