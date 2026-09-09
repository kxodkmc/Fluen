//! 知识库 MCP 桥接（KB 迁移 M2）。
//!
//! 进程内 MCP 直连：一侧 serve [`fluen_kb::mcp::KbServer`]（挂 `Kb` 句柄，
//! 含 embedding），另一侧持 rmcp Client，经 tokio duplex 内存传输对直连
//! （与 fluen-kb `tests/mcp_cases.rs` 的连接模式一致，无需 stdio 子进程）。
//!
//! - 桥接层只做转发：工具 schema 从 MCP `list_tools` 动态获取（单一真相源，
//!   与 SDK `mcp::tools::definitions()` 永不漂移）。
//! - [`McpKnowledgeTool`] 把 MCP 工具包装为 referee-ai `Tool` 注册进
//!   `ToolRegistry`；AI 产出正文在转发前经 `syntax::normalize` 规范化
//!   （T1 未闭合落地为显式闭合、T5 跨界嵌套折叠）。
//! - 连接生命周期：client drop → duplex 关闭 → server task 自然退出。

use std::sync::Arc;

use async_trait::async_trait;
use referee_ai::tool::{Tool, ToolContext, ToolError, ToolOutput};
use rmcp::model::{CallToolRequestParams, ClientInfo, ProtocolVersion};
use rmcp::service::{ClientLifecycleMode, RoleClient, RunningService};
use rmcp::{ClientServiceExt, ServiceExt};
use serde_json::Value;

use fluen_kb::mcp::KbServer;

use fluen_kb::KbResult;

/// 进程内 MCP 桥接：持有 rmcp client（server 运行于 background task）。
pub struct KbMcpBridge {
    client: RunningService<RoleClient, ClientInfo>,
    /// 连接建立时缓存的工具定义（name, description, schema），避免每次注册重复握手。
    tools: Vec<McpToolDef>,
}

/// MCP 工具定义快照。
#[derive(Debug, Clone)]
pub struct McpToolDef {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

impl KbMcpBridge {
    /// 连接到挂载 `kb` 的进程内 MCP server（含 embedding 注入后的句柄）。
    pub async fn connect(kb: fluen_kb::handle::Kb) -> KbResult<Self> {
        // `duplex` 返回一对互相连接的流：写一端、另一端可读。
        let (client_write, server_read) = tokio::io::duplex(64 * 1024);
        let (server_write, client_read) = tokio::io::duplex(64 * 1024);

        tokio::spawn(async move {
            match KbServer::new(kb).serve((server_read, server_write)).await {
                Ok(running) => {
                    let _ = running.waiting().await;
                }
                Err(e) => tracing::error!("kb mcp server 启动失败: {e}"),
            }
        });

        let client = ClientInfo::default()
            .serve_with_lifecycle(
                (client_read, client_write),
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                },
            )
            .await
            .map_err(|e| fluen_kb::KbError::Cancelled(format!("mcp client 握手失败: {e}")))?;

        let mut bridge = Self {
            client,
            tools: Vec::new(),
        };
        bridge.tools = bridge.fetch_tool_defs().await.map_err(|e| {
            fluen_kb::KbError::Cancelled(format!("mcp list_tools 失败: {e}"))
        })?;
        Ok(bridge)
    }

    async fn fetch_tool_defs(&self) -> Result<Vec<McpToolDef>, String> {
        let list = self
            .client
            .list_tools(None)
            .await
            .map_err(|e| format!("{e}"))?;
        Ok(list
            .tools
            .iter()
            .map(|t| McpToolDef {
                name: t.name.to_string(),
                description: t
                    .description
                    .as_ref()
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
                schema: Value::Object(t.input_schema.as_ref().clone()),
            })
            .collect())
    }

    /// 连接时缓存的工具定义。
    pub fn tool_defs(&self) -> &[McpToolDef] {
        &self.tools
    }

    /// 调用 MCP 工具，返回结构化 JSON 结果。
    pub async fn call_tool(&self, name: &str, args: Value) -> Result<Value, String> {
        let mut params = CallToolRequestParams::new(name.to_string());
        params.arguments = args.as_object().cloned();
        let response = self
            .client
            .call_tool(params)
            .await
            .map_err(|e| format!("MCP 调用 {name} 失败: {e}"))?;
        response
            .structured_content
            .ok_or_else(|| format!("MCP 工具 {name} 未返回结构化结果"))
    }
}

/// 桥接工具名集合：知识库构建所需（与旧 `KnowledgeToolProvider` 启用的 5 个一致）。
pub const KB_BUILD_TOOL_NAMES: [&str; 5] = [
    "knowledge_query",
    "knowledge_query_batch",
    "knowledge_create_entry",
    "knowledge_edit_entry",
    "knowledge_get_entry",
];

/// 把 MCP 工具包装为 referee-ai `Tool`（只做转发）。
///
/// `normalize_bodies`：转发前对 AI 产出的正文执行 `syntax::normalize`
/// （create_entry 的 `body`、edit_entry 的 `ops[].content/replace`），
/// 将 T1/T5 违例落地为显式闭合的规范形态。
pub struct McpKnowledgeTool {
    def: McpToolDef,
    bridge: Arc<KbMcpBridge>,
    normalize_bodies: bool,
}

impl McpKnowledgeTool {
    /// 从桥接的工具定义构造（按 `KB_BUILD_TOOL_NAMES` 过滤）。
    pub fn list_from(bridge: &Arc<KbMcpBridge>, names: &[&str]) -> Vec<Arc<dyn Tool>> {
        bridge
            .tool_defs()
            .iter()
            .filter(|def| names.contains(&def.name.as_str()))
            .map(|def| {
                let normalize = def.name == "knowledge_create_entry"
                    || def.name == "knowledge_edit_entry";
                Arc::new(McpKnowledgeTool {
                    def: def.clone(),
                    bridge: Arc::clone(bridge),
                    normalize_bodies: normalize,
                }) as Arc<dyn Tool>
            })
            .collect()
    }
}

#[async_trait]
impl Tool for McpKnowledgeTool {
    fn name(&self) -> &str {
        &self.def.name
    }

    fn description(&self) -> &str {
        &self.def.description
    }

    fn input_schema(&self) -> Value {
        self.def.schema.clone()
    }

    async fn execute(&self, _ctx: ToolContext, args: Value) -> Result<ToolOutput, ToolError> {
        let args = if self.normalize_bodies {
            normalize_ai_bodies(&self.def.name, args)
        } else {
            args
        };
        let result = self
            .bridge
            .call_tool(&self.def.name, args)
            .await
            .map_err(ToolError::Execution)?;
        Ok(ToolOutput::from_json(&result))
    }
}

/// 对 AI 产出的正文执行溯源语法规范化（T1/T5 → 显式闭合）。
fn normalize_ai_bodies(tool_name: &str, mut args: Value) -> Value {
    let normalize_str = |s: &str| fluen_kb::syntax::normalize(s);
    match tool_name {
        "knowledge_create_entry" => {
            if let Some(body) = args.get_mut("body").and_then(|v| v.as_str()) {
                let normalized = normalize_str(body);
                args["body"] = Value::String(normalized);
            }
        }
        "knowledge_edit_entry" => {
            if let Some(ops) = args.get_mut("ops").and_then(Value::as_array_mut) {
                for op in ops.iter_mut() {
                    for key in ["content", "replace"] {
                        if let Some(text) = op.get_mut(key).and_then(|v| v.as_str()) {
                            let normalized = normalize_str(text);
                            op[key] = Value::String(normalized);
                        }
                    }
                }
            }
        }
        _ => {}
    }
    args
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use referee_ai::tool::ToolContext;
    use serde_json::json;

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

    #[tokio::test]
    async fn bridge_create_and_query_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let kb = fluen_kb::KbBuilder::new(dir.path()).open().unwrap();
        let bridge = KbMcpBridge::connect(kb).await.unwrap();

        // create（经 MCP）
        let out = bridge
            .call_tool(
                "knowledge_create_entry",
                json!({
                    "type": "concept",
                    "title": "桥接测试",
                    "body": "<ref-1111111111111111>经由 MCP 桥接创建的正文。</ref-1111111111111111>"
                }),
            )
            .await
            .unwrap();
        assert!(out.get("created").is_some() || out.get("merged_into").is_some());

        // query（经 MCP，含正文）
        let out = bridge
            .call_tool(
                "knowledge_query",
                json!({"query": "桥接", "include_content": true}),
            )
            .await
            .unwrap();
        let hits = out.as_array().unwrap();
        assert!(!hits.is_empty());
        assert!(hits[0]["content"].as_str().unwrap().contains("经由 MCP 桥接创建"));
    }

    #[tokio::test]
    async fn mcp_tool_wraps_schema_and_normalizes_body() {
        let dir = tempfile::tempdir().unwrap();
        let kb = fluen_kb::KbBuilder::new(dir.path()).open().unwrap();
        let bridge = Arc::new(KbMcpBridge::connect(kb).await.unwrap());

        let tools = McpKnowledgeTool::list_from(&bridge, &["knowledge_create_entry"]);
        assert_eq!(tools.len(), 1);
        let tool = &tools[0];

        // schema 来自 MCP definitions（type/title/body 必填）
        let schema = tool.input_schema();
        assert_eq!(schema["required"][0], json!("type"));

        // 未闭合标签（T1）+ 跨界嵌套（T5）→ 转发前 normalize 落地为显式闭合
        let output = tool
            .execute(
                ctx(),
                json!({
                    "type": "concept",
                    "title": "normalize 测试",
                    "body": "<ref-1111111111111111>未闭合的正文片段"
                }),
            )
            .await
            .unwrap();
        let value: Value = serde_json::from_str(&output.content).unwrap();
        assert!(value.get("created").is_some() || value.get("merged_into").is_some());

        // 落库正文应为闭合形态（get_entry 验证）
        let id = value
            .get("created")
            .and_then(Value::as_str)
            .or_else(|| value.get("merged_into").and_then(Value::as_str))
            .unwrap()
            .to_string();
        let doc = bridge
            .call_tool("knowledge_get_entry", json!({"id": id}))
            .await
            .unwrap();
        let body = doc["body"].as_str().unwrap();
        assert!(body.contains("</ref-1111111111111111>"), "正文必须显式闭合: {body}");
    }
}
