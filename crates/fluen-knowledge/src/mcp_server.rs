//! MCP (Model Context Protocol) Server。
//!
//! 将知识库操作暴露为标准 MCP 工具，通过 JSON-RPC 2.0 over stdio
//! 与 MCP 客户端（如 Claude、Cursor）通信。
//!
//! # 协议
//!
//! - 传输：newline-delimited JSON-RPC 2.0 over stdin/stdout
//! - 握手：`initialize` → 返回 server info + capabilities
//! - 工具发现：`tools/list` → 返回工具定义列表
//! - 工具调用：`tools/call` → 按 name + arguments 执行
//!
//! # 暴露的工具
//!
//! | 工具短名 | 功能 |
//! |---------|------|
//! | `query` | 单条知识库查询 |
//! | `query_batch` | 批量查询 |
//! | `create_entry` | 新建条目 |
//! | `edit_entry` | 修改条目 |
//! | `meta` | 元信息查询 |
//! | `get_entry` | 获取条目详情 |
//! | `list_entries` | 列出所有条目 |
//! | `delete_entry` | 删除条目 |
//!
//! # 示例
//!
//! ```no_run
//! # use fluen_knowledge::async_kb::AsyncKnowledgeBase;
//! # use fluen_knowledge::config::KnowledgeConfig;
//! # use fluen_knowledge::mcp_server::KnowledgeMcpServer;
//! #
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let kb = AsyncKnowledgeBase::open("references")?;
//! let server = KnowledgeMcpServer::new(kb, KnowledgeConfig::default());
//! server.run_stdio().await?;
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::async_kb::{AsyncBatchQueryParams, AsyncKnowledgeBase, AsyncQueryParams};
use crate::config::{KnowledgeConfig, ALL_TOOL_SHORT_NAMES};
use crate::error::{KnowledgeError, Result};
use crate::types::MetaQueryType;
use crate::wiki::{CreateEntryParams, EditEntryParams};

// ════════════════════════════════════════════════════════════════
// JSON-RPC 2.0 类型
// ════════════════════════════════════════════════════════════════

/// JSON-RPC 2.0 请求。
#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

/// JSON-RPC 2.0 响应。
#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 错误对象。
#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl JsonRpcError {
    fn method_not_found(id: &Value) -> (i32, String) {
        let _ = id;
        (-32601, "Method not found".to_string())
    }

    fn invalid_params(msg: &str) -> (i32, String) {
        (-32602, format!("Invalid params: {msg}"))
    }

    fn internal(msg: &str) -> (i32, String) {
        (-32603, msg.to_string())
    }
}

/// MCP Tool 定义（用于 `tools/list` 响应）。
#[derive(Debug, Serialize)]
struct McpToolDef {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: Value,
}

/// MCP Tool 调用结果内容项。
#[derive(Debug, Serialize)]
struct McpContentItem {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
}

/// MCP Tool 调用结果。
#[derive(Debug, Serialize)]
struct McpCallResult {
    content: Vec<McpContentItem>,
    #[serde(rename = "isError")]
    is_error: bool,
}

// ════════════════════════════════════════════════════════════════
// MCP Server
// ════════════════════════════════════════════════════════════════

/// 知识库 MCP Server。
///
/// 持有 [`AsyncKnowledgeBase`] 和 [`KnowledgeConfig`]，
/// 通过 `run_stdio` 启动 JSON-RPC 消息循环。
pub struct KnowledgeMcpServer {
    kb: AsyncKnowledgeBase,
    config: KnowledgeConfig,
}

impl KnowledgeMcpServer {
    /// 构造 MCP server。
    pub fn new(kb: AsyncKnowledgeBase, config: KnowledgeConfig) -> Self {
        Self { kb, config }
    }

    /// 启动 stdio 消息循环。
    ///
    /// 从 stdin 逐行读取 JSON-RPC 请求，处理后写回 stdout。
    /// 遇到 EOF 或 `shutdown` 通知时退出。
    pub async fn run_stdio(&self) -> Result<()> {
        let stdin = tokio::io::stdin();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();

        let stdout = tokio::io::stdout();
        let mut writer = tokio::io::BufWriter::new(stdout);

        tracing::info!("knowledge MCP server started (stdio mode)");

        loop {
            let line = match lines.next_line().await {
                Ok(Some(line)) => line,
                Ok(None) => break, // EOF
                Err(e) => {
                    tracing::error!(error = %e, "stdin read error");
                    return Err(KnowledgeError::Io(e));
                }
            };

            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // 解析 JSON-RPC 请求
            let request: JsonRpcRequest = match serde_json::from_str(line) {
                Ok(r) => r,
                Err(e) => {
                    // 无法解析的请求，返回 parse error
                    let resp = JsonRpcResponse {
                        jsonrpc: "2.0".into(),
                        id: None,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32700,
                            message: format!("Parse error: {e}"),
                            data: None,
                        }),
                    };
                    Self::write_response(&mut writer, &resp).await?;
                    continue;
                }
            };

            // 处理请求
            let is_shutdown = request.method == "shutdown"
                || request.method == "exit";
            let response = self.handle_request(request).await;

            // 通知（无 id）不返回响应
            if response.id.is_none() && response.result.is_none() && response.error.is_none() {
                if is_shutdown {
                    break;
                }
                continue;
            }

            Self::write_response(&mut writer, &response).await?;

            if is_shutdown {
                break;
            }
        }

        tracing::info!("knowledge MCP server stopped");
        Ok(())
    }

    /// 将 JSON-RPC 响应写入 stdout（单行 JSON + newline）。
    async fn write_response<W: AsyncWriteExt + Unpin>(
        writer: &mut W,
        response: &JsonRpcResponse,
    ) -> Result<()> {
        let json = serde_json::to_string(response)
            .map_err(|e| KnowledgeError::McpProtocol(format!("response serialize error: {e}")))?;
        writer
            .write_all(json.as_bytes())
            .await
            .map_err(KnowledgeError::Io)?;
        writer
            .write_all(b"\n")
            .await
            .map_err(KnowledgeError::Io)?;
        writer.flush().await.map_err(KnowledgeError::Io)?;
        Ok(())
    }

    /// 路由 JSON-RPC 请求到对应处理函数。
    async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let id = request.id.clone();
        let is_notification = id.is_none();

        let result = match request.method.as_str() {
            "initialize" => self.handle_initialize(&request.params),
            "initialized" => {
                // 通知，无需响应
                return JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id: None,
                    result: None,
                    error: None,
                };
            }
            "shutdown" | "exit" => Ok(json!({})),
            "tools/list" => self.handle_list_tools(),
            "tools/call" => self.handle_call_tool(&request.params).await,
            _ => {
                let (code, msg) = JsonRpcError::method_not_found(&request.id.clone().unwrap_or(json!(null)));
                return JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code,
                        message: msg,
                        data: None,
                    }),
                };
            }
        };

        match result {
            Ok(value) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: Some(value),
                error: None,
            },
            Err((code, msg)) => {
                if is_notification {
                    JsonRpcResponse {
                        jsonrpc: "2.0".into(),
                        id: None,
                        result: None,
                        error: None,
                    }
                } else {
                    JsonRpcResponse {
                        jsonrpc: "2.0".into(),
                        id,
                        result: None,
                        error: Some(JsonRpcError {
                            code,
                            message: msg,
                            data: None,
                        }),
                    }
                }
            }
        }
    }

    // ── JSON-RPC 方法处理 ──

    /// `initialize` 方法：返回 server info 和 capabilities。
    fn handle_initialize(&self, _params: &Value) -> std::result::Result<Value, (i32, String)> {
        Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {
                    "listChanged": false
                }
            },
            "serverInfo": {
                "name": "fluen-knowledge",
                "version": env!("CARGO_PKG_VERSION")
            }
        }))
    }

    /// `tools/list` 方法：返回工具定义列表。
    fn handle_list_tools(&self) -> std::result::Result<Value, (i32, String)> {
        let tools: Vec<McpToolDef> = ALL_TOOL_SHORT_NAMES
            .iter()
            .filter(|name| self.config.is_tool_enabled(name))
            .map(|name| self.tool_definition(name))
            .collect();

        Ok(json!({ "tools": tools }))
    }

    /// `tools/call` 方法：执行工具调用。
    async fn handle_call_tool(
        &self,
        params: &Value,
    ) -> std::result::Result<Value, (i32, String)> {
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JsonRpcError::invalid_params("missing 'name' field"))?;

        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        let result = self.call_tool(name, &arguments).await;

        Ok(serde_json::to_value(&result).map_err(|e| {
            JsonRpcError::internal(&format!("result serialize error: {e}"))
        })?)
    }

    // ── 工具定义 ──

    /// 获取工具的 MCP 定义。
    fn tool_definition(&self, short_name: &str) -> McpToolDef {
        let full_name = self.config.tool_name(short_name);
        let (description, input_schema) = match short_name {
            "query" => (
                "Query the knowledge base. Supports keyword (FTS5), semantic (embedding cosine similarity), and hybrid retrieval. "
                .to_string(),
                json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query (keyword, wiki ID, tag ID, ref ID, or entry title)"
                        },
                        "method": {
                            "type": "string",
                            "enum": ["keyword", "semantic", "hybrid"],
                            "default": "hybrid",
                            "description": "Retrieval method"
                        },
                        "wiki_type": {
                            "type": "string",
                            "enum": ["summary", "concept", "entity"],
                            "description": "Filter by entry type"
                        },
                        "top_k": {
                            "type": "integer",
                            "default": 10,
                            "description": "Max results to return"
                        },
                        "include_content": {
                            "type": "boolean",
                            "default": false,
                            "description": "Include entry content in results"
                        }
                    },
                    "required": ["query"]
                }),
            ),
            "query_batch" => (
                "Batch query the knowledge base with multiple queries.".to_string(),
                json!({
                    "type": "object",
                    "properties": {
                        "queries": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "List of search queries"
                        },
                        "method": {
                            "type": "string",
                            "enum": ["keyword", "semantic", "hybrid"],
                            "default": "hybrid"
                        },
                        "wiki_type": {
                            "type": "string",
                            "enum": ["summary", "concept", "entity"]
                        },
                        "top_k": {
                            "type": "integer",
                            "default": 10
                        },
                        "include_content": {
                            "type": "boolean",
                            "default": false
                        }
                    },
                    "required": ["queries"]
                }),
            ),
            "create_entry" => (
                "Create a new knowledge base entry. Automatically deduplicates (summary by source, concept/entity by title+type)."
                .to_string(),
                json!({
                    "type": "object",
                    "properties": {
                        "wiki_type": {
                            "type": "string",
                            "enum": ["summary", "concept", "entity"],
                            "description": "Entry type"
                        },
                        "title": {
                            "type": "string",
                            "description": "Entry title"
                        },
                        "content": {
                            "type": "string",
                            "description": "Markdown content (without frontmatter)"
                        },
                        "source": {
                            "type": "string",
                            "description": "Source file path (summaries only), e.g. raw/ref-xxx.pdf"
                        },
                        "authors": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Author wiki IDs (summaries only)"
                        },
                        "tags": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Tag names (auto-upserted to tag IDs)"
                        },
                        "relations": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Related wiki IDs"
                        }
                    },
                    "required": ["wiki_type", "title", "content"]
                }),
            ),
            "edit_entry" => (
                "Edit an existing knowledge base entry. Supports search-replace, insert-after, add tags and relations."
                .to_string(),
                json!({
                    "type": "object",
                    "properties": {
                        "wiki_id": {
                            "type": "string",
                            "description": "Wiki entry ID (wiki-xxxxxxxxxxxxxxxx)"
                        },
                        "edits": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "type": {
                                        "type": "string",
                                        "enum": ["search_replace", "insert_after"]
                                    },
                                    "search": {"type": "string"},
                                    "replace": {"type": "string"},
                                    "anchor": {"type": "string"},
                                    "content": {"type": "string"}
                                },
                                "required": ["type"]
                            },
                            "description": "Edit operations"
                        },
                        "add_relations": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Wiki IDs to add as relations"
                        },
                        "add_tags": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Tag names to add"
                        }
                    },
                    "required": ["wiki_id"]
                }),
            ),
            "meta" => (
                "Query knowledge base metadata. Types: overview (counts), tags (all tags), recent (recently updated entries)."
                .to_string(),
                json!({
                    "type": "object",
                    "properties": {
                        "query_type": {
                            "type": "string",
                            "enum": ["overview", "tags", "recent"],
                            "default": "overview"
                        },
                        "limit": {
                            "type": "integer",
                            "default": 20,
                            "description": "Max items for recent query"
                        }
                    }
                }),
            ),
            "get_entry" => (
                "Get full entry details by wiki ID (includes content, tag names, relation titles)."
                .to_string(),
                json!({
                    "type": "object",
                    "properties": {
                        "wiki_id": {
                            "type": "string",
                            "description": "Wiki entry ID"
                        }
                    },
                    "required": ["wiki_id"]
                }),
            ),
            "list_entries" => (
                "List all knowledge base entries (without content).".to_string(),
                json!({
                    "type": "object",
                    "properties": {}
                }),
            ),
            "delete_entry" => (
                "Delete a knowledge base entry by wiki ID. Removes MD file, DB record, and syncs index."
                .to_string(),
                json!({
                    "type": "object",
                    "properties": {
                        "wiki_id": {
                            "type": "string",
                            "description": "Wiki entry ID to delete"
                        }
                    },
                    "required": ["wiki_id"]
                }),
            ),
            _ => (
                "Unknown tool".to_string(),
                json!({"type": "object", "properties": {}}),
            ),
        };

        McpToolDef {
            name: full_name,
            description,
            input_schema,
        }
    }

    // ── 工具调用执行 ──

    /// 执行工具调用，返回 MCP 格式结果。
    async fn call_tool(&self, name: &str, arguments: &Value) -> McpCallResult {
        // 去除前缀，获取短名
        let short_name = name
            .strip_prefix(&self.config.tool_prefix)
            .unwrap_or(name);

        let result = match short_name {
            "query" => self.exec_query(arguments).await,
            "query_batch" => self.exec_query_batch(arguments).await,
            "create_entry" => self.exec_create_entry(arguments).await,
            "edit_entry" => self.exec_edit_entry(arguments).await,
            "meta" => self.exec_meta(arguments).await,
            "get_entry" => self.exec_get_entry(arguments).await,
            "list_entries" => self.exec_list_entries(arguments).await,
            "delete_entry" => self.exec_delete_entry(arguments).await,
            _ => {
                return McpCallResult {
                    content: vec![McpContentItem {
                        content_type: "text".into(),
                        text: format!("Unknown tool: {name}"),
                    }],
                    is_error: true,
                };
            }
        };

        match result {
            Ok(json_value) => McpCallResult {
                content: vec![McpContentItem {
                    content_type: "text".into(),
                    text: serde_json::to_string_pretty(&json_value).unwrap_or_default(),
                }],
                is_error: false,
            },
            Err(e) => McpCallResult {
                content: vec![McpContentItem {
                    content_type: "text".into(),
                    text: format!("Error: {e}"),
                }],
                is_error: true,
            },
        }
    }

    /// 执行 query 工具。
    async fn exec_query(&self, args: &Value) -> std::result::Result<Value, String> {
        let params: AsyncQueryParams = serde_json::from_value(args.clone())
            .map_err(|e| format!("invalid params: {e}"))?;

        let mut result = self.kb.query(params).await.map_err(|e| e.to_string())?;

        // 截断 content
        if self.config.max_content_length > 0 {
            for m in &mut result.results {
                if let Some(content) = &m.content {
                    m.content = Some(self.config.truncate_content(content));
                }
            }
        }

        // 简洁输出：仅返回关键字段
        Ok(json!({
            "success": result.success,
            "method": result.retrieval_method_used.as_str(),
            "count": result.results.len(),
            "results": result.results.iter().map(|m| {
                json!({
                    "id": m.wiki_id,
                    "type": m.wiki_type,
                    "title": m.title,
                    "score": format!("{:.3}", m.score),
                    "content": m.content,
                })
            }).collect::<Vec<_>>()
        }))
    }

    /// 执行 query_batch 工具。
    async fn exec_query_batch(&self, args: &Value) -> std::result::Result<Value, String> {
        let params: AsyncBatchQueryParams = serde_json::from_value(args.clone())
            .map_err(|e| format!("invalid params: {e}"))?;

        let result = self.kb.query_batch(params).await.map_err(|e| e.to_string())?;

        Ok(json!({
            "success": result.success,
            "results": result.results.iter().map(|item| {
                json!({
                    "query": item.query,
                    "method": item.retrieval_method_used.as_str(),
                    "count": item.matches.len(),
                    "matches": item.matches.iter().map(|m| {
                        json!({
                            "id": m.wiki_id,
                            "type": m.wiki_type,
                            "title": m.title,
                            "score": format!("{:.3}", m.score),
                        })
                    }).collect::<Vec<_>>()
                })
            }).collect::<Vec<_>>()
        }))
    }

    /// 执行 create_entry 工具。
    async fn exec_create_entry(&self, args: &Value) -> std::result::Result<Value, String> {
        let params: CreateEntryParams = serde_json::from_value(args.clone())
            .map_err(|e| format!("invalid params: {e}"))?;

        let result = self.kb.create_entry(params).await.map_err(|e| e.to_string())?;

        Ok(json!({
            "success": result.success,
            "wiki_id": result.wiki_id,
            "file_path": result.file_path,
            "merged": result.merged,
            "tags": result.tags_generated.iter().map(|t| {
                json!({"name": t.name, "tag_id": t.tag_id})
            }).collect::<Vec<_>>()
        }))
    }

    /// 执行 edit_entry 工具。
    async fn exec_edit_entry(&self, args: &Value) -> std::result::Result<Value, String> {
        let params: EditEntryParams = serde_json::from_value(args.clone())
            .map_err(|e| format!("invalid params: {e}"))?;

        let result = self.kb.edit_entry(params).await.map_err(|e| e.to_string())?;

        Ok(json!({
            "success": result.success,
            "wiki_id": result.wiki_id,
            "file_path": result.file_path,
            "updated_time": result.updated_time,
            "edit_results": result.edit_results.iter().map(|r| {
                json!({"type": r.edit_type, "success": r.success})
            }).collect::<Vec<_>>()
        }))
    }

    /// 执行 meta 工具。
    async fn exec_meta(&self, args: &Value) -> std::result::Result<Value, String> {
        let query_type_str = args
            .get("query_type")
            .and_then(|v| v.as_str())
            .unwrap_or("overview");
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(self.config.default_recent_limit as u64) as usize;

        let query_type = match query_type_str {
            "tags" => MetaQueryType::Tags,
            "recent" => MetaQueryType::Recent,
            _ => MetaQueryType::Overview,
        };

        let result = self
            .kb
            .meta(query_type, limit)
            .await
            .map_err(|e| e.to_string())?;

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

    /// 执行 get_entry 工具。
    async fn exec_get_entry(&self, args: &Value) -> std::result::Result<Value, String> {
        let wiki_id = args
            .get("wiki_id")
            .and_then(|v| v.as_str())
            .ok_or("missing 'wiki_id'")?
            .to_string();

        let entry = self
            .kb
            .get_entry(wiki_id)
            .await
            .map_err(|e| e.to_string())?;

        match entry {
            Some(detail) => {
                let mut entry = detail.entry;
                // 截断 content
                if self.config.max_content_length > 0 && !entry.content.is_empty() {
                    entry.content = self.config.truncate_content(&entry.content);
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

    /// 执行 list_entries 工具。
    async fn exec_list_entries(&self, _args: &Value) -> std::result::Result<Value, String> {
        let entries = self.kb.list_entries().await.map_err(|e| e.to_string())?;

        Ok(json!({
            "count": entries.len(),
            "entries": entries.iter().map(|e| {
                json!({
                    "id": e.id,
                    "type": e.wiki_type,
                    "title": e.title,
                    "tags": e.tags,
                    "updated": e.updated,
                })
            }).collect::<Vec<_>>()
        }))
    }

    /// 执行 delete_entry 工具。
    async fn exec_delete_entry(&self, args: &Value) -> std::result::Result<Value, String> {
        let wiki_id = args
            .get("wiki_id")
            .and_then(|v| v.as_str())
            .ok_or("missing 'wiki_id'")?
            .to_string();

        self.kb.delete_entry(wiki_id).await.map_err(|e| e.to_string())?;

        Ok(json!({"success": true}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mcp_tool_definitions() {
        let dir = tempfile::TempDir::new().unwrap();
        let kb = AsyncKnowledgeBase::init(dir.path()).unwrap();
        let server = KnowledgeMcpServer::new(kb, KnowledgeConfig::default());

        let tool_def = server.tool_definition("query");
        assert_eq!(tool_def.name, "knowledge_query");
        assert!(!tool_def.description.is_empty());

        let tool_def2 = server.tool_definition("create_entry");
        assert_eq!(tool_def2.name, "knowledge_create_entry");
    }

    #[tokio::test]
    async fn test_mcp_call_tool() {
        let dir = tempfile::TempDir::new().unwrap();
        let kb = AsyncKnowledgeBase::init(dir.path()).unwrap();
        let server = KnowledgeMcpServer::new(kb, KnowledgeConfig::default());

        // 创建条目
        let create_result = server
            .call_tool(
                "knowledge_create_entry",
                &json!({
                    "wiki_type": "concept",
                    "title": "MCP Test",
                    "content": "Testing MCP server tool call.",
                    "tags": ["mcp", "test"]
                }),
            )
            .await;
        assert!(!create_result.is_error);
        let create_json: Value = serde_json::from_str(&create_result.content[0].text).unwrap();
        assert_eq!(create_json["success"], true);

        // 查询
        let query_result = server
            .call_tool(
                "knowledge_query",
                &json!({
                    "query": "MCP",
                    "method": "keyword"
                }),
            )
            .await;
        assert!(!query_result.is_error);
        let query_json: Value = serde_json::from_str(&query_result.content[0].text).unwrap();
        assert_eq!(query_json["success"], true);
        assert!(query_json["count"].as_u64().unwrap() > 0);

        // 元信息
        let meta_result = server
            .call_tool("knowledge_meta", &json!({"query_type": "overview"}))
            .await;
        assert!(!meta_result.is_error);
        let meta_json: Value = serde_json::from_str(&meta_result.content[0].text).unwrap();
        assert_eq!(meta_json["data"]["total_entries"], 1);
    }

    #[tokio::test]
    async fn test_mcp_config_filter() {
        let dir = tempfile::TempDir::new().unwrap();
        let kb = AsyncKnowledgeBase::init(dir.path()).unwrap();
        let config = KnowledgeConfig::builder()
            .enabled_tools(vec!["query".into(), "meta".into()])
            .build();
        let server = KnowledgeMcpServer::new(kb, config);

        // tools/list 应只返回 query 和 meta
        let result = server.handle_list_tools().unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 2);
        assert!(tools.iter().any(|t| t["name"] == "knowledge_query"));
        assert!(tools.iter().any(|t| t["name"] == "knowledge_meta"));
        assert!(!tools.iter().any(|t| t["name"] == "knowledge_create_entry"));
    }
}
