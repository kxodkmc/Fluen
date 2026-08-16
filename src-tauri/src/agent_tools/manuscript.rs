//! 论文正文写入工具——供智能体以**格式规范**的方式撰写论文内容。
//!
//! 与通用文件工具 `project_file` 不同，本工具感知 Fluen 章节结构：
//! 写入 `manuscript/main.md` 前经过 fluen-markup 校验（存在 `Severity::Error`
//! 硬错误时拒绝保存），保存后自动拆分同步各 `sec-{id}.md` 备份与
//! `sections.json`（章节 ID 按标记 / H1 标题稳定匹配）。
//!
//! 这是学术助手撰写正文的标准落盘通道；写操作同样纳入工具审批
//! （`motis_chat::approval::ToolApprovalExtension`），需用户点击「应用」才生效。

use std::sync::Arc;

use async_trait::async_trait;
use confluent::agent_runtime::{InvocationContext, Tool, ToolError, ToolProvider, ToolSchema};
use serde_json::{json, Value};

/// 工具名称。
pub const MANUSCRIPT_TOOL_NAME: &str = "manuscript";

/// 论文正文写入工具。
pub struct ManuscriptEditTool {
    schema: ToolSchema,
    /// 论文项目根路径（构造时注入）。
    project_path: String,
}

impl ManuscriptEditTool {
    /// 构造工具。
    pub fn new(project_path: String) -> Self {
        let parameters = json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["update"],
                    "description": "update：用给定全文替换论文正文。保存前自动做 fluen-markup 格式校验（硬错误会拒绝保存），保存后自动同步章节备份与 sections.json"
                },
                "content": {
                    "type": "string",
                    "description": "论文 main.md 完整内容。可含或不含 <!-- @sec_id:xxx --> 标记：含标记则按标记定位章节；缺失标记时按 H1 标题匹配旧章节（保持 ID 稳定）；新增章节自动生成新 ID"
                }
            },
            "required": ["action", "content"]
        });

        Self {
            schema: ToolSchema {
                name: MANUSCRIPT_TOOL_NAME.into(),
                description: "撰写/更新论文正文（格式规范）：整体替换 manuscript/main.md 内容，自动校验 fluen-markup 语法并同步章节备份与索引。写入前会弹出确认框，需用户点击「应用」后才真正保存。".into(),
                parameters,
            },
            project_path,
        }
    }
}

#[async_trait]
impl Tool for ManuscriptEditTool {
    fn schema(&self) -> &ToolSchema {
        &self.schema
    }

    async fn invoke(
        &self,
        input: Value,
        _ctx: &InvocationContext,
    ) -> Result<Value, ToolError> {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("缺少 action 参数".into()))?;
        let content = input
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("update 需要 content 参数".into()))?;

        match action {
            "update" => {
                let request = crate::project::model::SaveDocumentRequest {
                    project_path: self.project_path.clone(),
                    content: content.to_string(),
                };
                let result = crate::project::section::save_document(request).map_err(|e| {
                    ToolError::ExecutionFailed(format!("论文保存失败: {}", e))
                })?;

                let sections: Vec<Value> = result
                    .sections
                    .iter()
                    .map(|s| {
                        json!({
                            "id": s.id,
                            "title": s.title,
                            "order": s.order,
                        })
                    })
                    .collect();

                Ok(json!({
                    "path": "manuscript/main.md",
                    "saved": true,
                    "main_md_bytes": result.main_md.len(),
                    "section_count": sections.len(),
                    "sections": sections,
                }))
            }
            other => Err(ToolError::InvalidParams(format!(
                "未知 action: {}（可选值: update）",
                other
            ))),
        }
    }
}

/// 包装 [`ManuscriptEditTool`] 为 [`ToolProvider`]。
pub struct ManuscriptEditToolProvider {
    tool: Arc<ManuscriptEditTool>,
}

impl ManuscriptEditToolProvider {
    /// 构造提供者。
    pub fn new(project_path: String) -> Self {
        Self {
            tool: Arc::new(ManuscriptEditTool::new(project_path)),
        }
    }
}

#[async_trait]
impl ToolProvider for ManuscriptEditToolProvider {
    async fn list_tools(&self) -> Vec<Arc<dyn Tool>> {
        vec![self.tool.clone()]
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::creator;
    use crate::project::model::{CreateProjectRequest, sanitize_project_name};
    use std::fs;

    fn temp_project_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluen_manuscript_test_{}_{:?}_{}",
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

    fn create_test_project(storage: &std::path::Path) -> std::path::PathBuf {
        let request = CreateProjectRequest {
            title: "测试文章".into(),
            author: "张三".into(),
            project_name: sanitize_project_name("测试文章"),
            storage_path: storage.to_string_lossy().to_string(),
            description: None,
        };
        let path = creator::create_project(request).unwrap();
        std::path::PathBuf::from(path)
    }

    fn ctx() -> InvocationContext {
        InvocationContext {
            run_id: "test-run".into(),
            agent_id: "test-agent".into(),
            tool_call_id: "test-call".into(),
            cancel_token: tokio_util::sync::CancellationToken::new(),
            timeout: std::time::Duration::from_secs(30),
        }
    }

    #[tokio::test]
    async fn update_writes_main_md_and_syncs_sections() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        let tool = ManuscriptEditTool::new(project_dir.to_string_lossy().to_string());

        let content = "# 引言\n\n引言正文\n\n# 方法\n\n方法正文";
        let result = tool
            .invoke(json!({ "action": "update", "content": content }), &ctx())
            .await
            .unwrap();

        assert_eq!(result["saved"], true);
        assert_eq!(result["section_count"], 2);

        // main.md 已写入且含标记（归一化重建）
        let main_file = fs::read_to_string(project_dir.join("manuscript/main.md")).unwrap();
        assert!(main_file.contains("# 引言"));
        assert!(main_file.contains("# 方法"));
        assert!(main_file.contains("<!-- @sec_id:sec-"));

        // 章节备份已生成（2 个 sec-*.md）
        let sections_dir = project_dir.join("manuscript/sections");
        let md_count = fs::read_dir(&sections_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.starts_with("sec-") && name.ends_with(".md")
            })
            .count();
        assert_eq!(md_count, 2);

        let _ = fs::remove_dir_all(&storage);
    }

    #[tokio::test]
    async fn update_keeps_ids_by_marker() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        let tool = ManuscriptEditTool::new(project_dir.to_string_lossy().to_string());

        // 首次写入两个章节
        let first = "# 引言\n\n引言正文";
        let result = tool
            .invoke(json!({ "action": "update", "content": first }), &ctx())
            .await
            .unwrap();
        let section_id = result["sections"][0]["id"].as_str().unwrap().to_string();

        // 第二次更新：标记保留、标题变更 → ID 稳定
        let main_file = fs::read_to_string(project_dir.join("manuscript/main.md")).unwrap();
        let updated = main_file.replace("# 引言", "# 绪论");
        let result2 = tool
            .invoke(json!({ "action": "update", "content": updated }), &ctx())
            .await
            .unwrap();
        assert_eq!(result2["sections"][0]["id"], section_id);
        assert_eq!(result2["sections"][0]["title"], "绪论");

        let _ = fs::remove_dir_all(&storage);
    }

    #[tokio::test]
    async fn update_rejects_lint_error() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        let tool = ManuscriptEditTool::new(project_dir.to_string_lossy().to_string());

        // 重复 id 触发 lint 硬错误 → 拒绝保存
        let bad = "# 引言\n\n<f-fig id=\"fig:a\" src=\"assets/a.png\">\n<f-caption>图一</f-caption>\n</f-fig>\n\n<f-fig id=\"fig:a\" src=\"assets/a.png\">\n<f-caption>图二</f-caption>\n</f-fig>";
        let err = tool
            .invoke(json!({ "action": "update", "content": bad }), &ctx())
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::ExecutionFailed(_)));
        assert!(err.to_string().contains("拒绝保存"));

        let _ = fs::remove_dir_all(&storage);
    }

    #[tokio::test]
    async fn missing_content_rejected() {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        let tool = ManuscriptEditTool::new(project_dir.to_string_lossy().to_string());

        let err = tool
            .invoke(json!({ "action": "update" }), &ctx())
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidParams(_)));

        let _ = fs::remove_dir_all(&storage);
    }
}
