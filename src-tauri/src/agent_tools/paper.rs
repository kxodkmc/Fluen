//! 论文内容工具——供智能体读取当前论文的全文、大纲与指定章节。
//!
//! 装配为 confluent [`Tool`] + [`ToolProvider`]，捕获 `project_path` 于构造时
//! （与知识库 `KnowledgeToolProvider` 相同的注入方式），`invoke` 时实时读取
//! 项目文件（`project::loader::open_project`），保证内容为磁盘上的最新持久化状态。
//!
//! ## 功能
//!
//! 通过 `action` 参数选择：
//! - `full`：论文全文（内容较长，不推荐，优先使用 outline / section）
//! - `outline`：论文大纲（标题树）
//! - `section`：指定章节内容（含全部子章节），需传 `heading`（如 `# 引言` / `## 背景`）
//!
//! ## 设计
//!
//! - 解析逻辑全部下沉到 [`crate::agent_tools::parse`] 纯函数层，工具层仅做参数校验与组装。
//! - 输出格式简单结构化，便于 LLM 消费；章节未找到时返回可读错误并附可用标题列表。

use std::sync::Arc;

use async_trait::async_trait;
use confluent::agent_runtime::{InvocationContext, Tool, ToolError, ToolProvider, ToolSchema};
use serde_json::{json, Value};

use super::parse::{build_outline, extract_section};

/// 工具名称。
pub const PAPER_CONTENT_TOOL_NAME: &str = "paper_content";

/// 读取论文内容的智能体工具。
pub struct PaperContentTool {
    schema: ToolSchema,
    /// 论文项目根路径（构造时注入，与当前打开的项目绑定）。
    project_path: String,
}

impl PaperContentTool {
    /// 构造工具。
    pub fn new(project_path: String) -> Self {
        let parameters = json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["full", "outline", "section"],
                    "description": "获取论文内容的方式：full=全文（内容较长，不推荐）；outline=大纲（标题树）；section=指定章节内容"
                },
                "heading": {
                    "type": "string",
                    "description": "章节标题，仅 action=section 时必填。例如 '# 引言' 获取该一级章节及其全部子章节；'## 背景' 获取该二级章节及其子章节。也可省略 # 前缀按标题文本匹配"
                }
            },
            "required": ["action"]
        });

        Self {
            schema: ToolSchema {
                name: PAPER_CONTENT_TOOL_NAME.into(),
                description: "读取当前论文内容：可获取全文、大纲或指定章节（含子章节）。论文内容较长时优先使用 outline / section 而非 full。".into(),
                parameters,
            },
            project_path,
        }
    }

    /// 读取项目的最新持久化内容（main.md 全文）。
    fn load_paper(&self) -> Result<String, ToolError> {
        crate::project::loader::open_project(&self.project_path)
            .map(|result| result.main_md)
            .map_err(|e| ToolError::ExecutionFailed(format!("读取论文失败: {}", e)))
    }
}

#[async_trait]
impl Tool for PaperContentTool {
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

        let md = self.load_paper()?;

        match action {
            "full" => Ok(json!({ "content": md })),
            "outline" => Ok(json!({ "outline": build_outline(&md) })),
            "section" => {
                let heading = input
                    .get("heading")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ToolError::InvalidParams("action=section 时必须提供 heading 参数".into())
                    })?;

                match extract_section(&md, heading) {
                    Ok(section) => Ok(json!({
                        "heading": format!("{} {}", "#".repeat(section.heading.level as usize), section.heading.text),
                        "content": section.content,
                    })),
                    Err(e) => Err(ToolError::InvalidParams(e.message)),
                }
            }
            other => Err(ToolError::InvalidParams(format!(
                "未知 action: {}（可选值: full / outline / section）",
                other
            ))),
        }
    }
}

/// 包装 [`PaperContentTool`] 为 [`ToolProvider`]。
pub struct PaperContentToolProvider {
    tool: Arc<PaperContentTool>,
}

impl PaperContentToolProvider {
    /// 构造提供者。
    pub fn new(project_path: String) -> Self {
        Self {
            tool: Arc::new(PaperContentTool::new(project_path)),
        }
    }
}

#[async_trait]
impl ToolProvider for PaperContentToolProvider {
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

    /// 创建临时项目目录。
    fn temp_project_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluen_paper_tool_test_{}_{:?}_{}",
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

    /// 创建测试用项目。
    fn create_test_project(storage: &std::path::Path) -> std::path::PathBuf {
        let request = CreateProjectRequest {
            title: "测试文章".into(),
            author: "张三".into(),
            project_name: sanitize_project_name("测试文章"),
            storage_path: storage.to_string_lossy().to_string(),
            description: Some("测试描述".into()),
        };
        let path = creator::create_project(request).unwrap();
        std::path::PathBuf::from(path)
    }

    /// 向项目中添加一个章节（含 sections.json 更新）。
    fn add_section(
        project_dir: &std::path::Path,
        id: &str,
        order: u32,
        title: &str,
        body: &str,
    ) {
        let sections_dir = project_dir.join("manuscript").join("sections");
        let content = format!(
            "---\ntitle: \"{}\"\ntitle_html: \"\"\ncreated: 2026-01-01T00:00:00Z\nupdated: 2026-01-01T00:00:00Z\n---\n{}",
            title, body
        );
        fs::write(sections_dir.join(format!("{}.md", id)), content).unwrap();

        let json_path = sections_dir.join("sections.json");
        let mut sections: Vec<serde_json::Value> =
            serde_json::from_reader(fs::File::open(&json_path).unwrap()).unwrap();
        sections.push(serde_json::json!({
            "id": id,
            "order": order,
            "title": title,
            "references": []
        }));
        fs::write(&json_path, serde_json::to_string_pretty(&sections).unwrap()).unwrap();

        // 模拟旧版项目：无 main.md（creator 会创建空 main.md，此处移除以触发迁移拼装）
        let _ = fs::remove_file(project_dir.join("manuscript").join("main.md"));
    }

    fn build_test_project() -> (std::path::PathBuf, std::path::PathBuf) {
        let storage = temp_project_dir();
        let project_dir = create_test_project(&storage);
        add_section(
            &project_dir,
            "sec-aaa11111",
            0,
            "引言",
            "# 引言\n\n引言正文\n\n## 研究背景\n\n背景内容\n\n### 国内现状\n\n国内部分",
        );
        add_section(&project_dir, "sec-bbb22222", 1, "方法", "# 方法\n\n方法正文");
        (storage, project_dir)
    }

    #[tokio::test]
    async fn invoke_full_returns_whole_paper() {
        let (storage, project_dir) = build_test_project();
        let tool = PaperContentTool::new(project_dir.to_string_lossy().to_string());
        let result = tool
            .invoke(json!({ "action": "full" }), &ctx())
            .await
            .unwrap();
        assert!(result["content"].as_str().unwrap().contains("# 引言"));
        assert!(result["content"].as_str().unwrap().contains("# 方法"));
        let _ = fs::remove_dir_all(&storage);
    }

    #[tokio::test]
    async fn invoke_outline_returns_tree() {
        let (storage, project_dir) = build_test_project();
        let tool = PaperContentTool::new(project_dir.to_string_lossy().to_string());
        let result = tool
            .invoke(json!({ "action": "outline" }), &ctx())
            .await
            .unwrap();
        let outline = result["outline"].as_array().unwrap();
        assert_eq!(outline.len(), 2);
        assert_eq!(outline[0]["text"], "引言");
        assert_eq!(outline[0]["children"][0]["text"], "研究背景");
        let _ = fs::remove_dir_all(&storage);
    }

    #[tokio::test]
    async fn invoke_section_returns_subsections() {
        let (storage, project_dir) = build_test_project();
        let tool = PaperContentTool::new(project_dir.to_string_lossy().to_string());
        let result = tool
            .invoke(json!({ "action": "section", "heading": "# 引言" }), &ctx())
            .await
            .unwrap();
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("## 研究背景"));
        assert!(content.contains("### 国内现状"));
        assert!(!content.contains("# 方法"));
        let _ = fs::remove_dir_all(&storage);
    }

    #[tokio::test]
    async fn invoke_section_not_found_returns_error() {
        let (storage, project_dir) = build_test_project();
        let tool = PaperContentTool::new(project_dir.to_string_lossy().to_string());
        let err = tool
            .invoke(json!({ "action": "section", "heading": "# 不存在" }), &ctx())
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidParams(_)));
        assert!(err.to_string().contains("可用标题"));
        let _ = fs::remove_dir_all(&storage);
    }

    #[tokio::test]
    async fn invoke_missing_action_rejected() {
        let (storage, project_dir) = build_test_project();
        let tool = PaperContentTool::new(project_dir.to_string_lossy().to_string());
        let err = tool.invoke(json!({}), &ctx()).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidParams(_)));
        let _ = fs::remove_dir_all(&storage);
    }

    #[tokio::test]
    async fn invoke_unknown_action_rejected() {
        let (storage, project_dir) = build_test_project();
        let tool = PaperContentTool::new(project_dir.to_string_lossy().to_string());
        let err = tool
            .invoke(json!({ "action": "bogus" }), &ctx())
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidParams(_)));
        let _ = fs::remove_dir_all(&storage);
    }

    #[tokio::test]
    async fn invoke_section_without_heading_rejected() {
        let (storage, project_dir) = build_test_project();
        let tool = PaperContentTool::new(project_dir.to_string_lossy().to_string());
        let err = tool
            .invoke(json!({ "action": "section" }), &ctx())
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidParams(_)));
        let _ = fs::remove_dir_all(&storage);
    }

    #[tokio::test]
    async fn invoke_invalid_project_path_fails() {
        let tool = PaperContentTool::new("/nonexistent/fluen-project".into());
        let err = tool.invoke(json!({ "action": "full" }), &ctx()).await.unwrap_err();
        assert!(matches!(err, ToolError::ExecutionFailed(_)));
    }

    /// 构造最小可用的 InvocationContext。
    fn ctx() -> InvocationContext {
        InvocationContext {
            run_id: "test-run".into(),
            agent_id: "test-agent".into(),
            tool_call_id: "test-call".into(),
            cancel_token: tokio_util::sync::CancellationToken::new(),
            timeout: std::time::Duration::from_secs(30),
        }
    }
}
