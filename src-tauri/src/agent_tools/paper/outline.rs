//! `paper_outline` 工具——读取当前在写论文的大纲（标题树）。
//!
//! 单一职责：只返回结构，不含正文。实现 referee [`Tool`] trait，
//! 文件读取统一经 [`PaperReader`](super::PaperReader) 走 referee `read`。

use async_trait::async_trait;
use referee_ai::tool::{Tool, ToolCategory, ToolContext, ToolError, ToolOutput};
use serde_json::{json, Value};

use super::PaperReader;
use crate::agent_tools::parse::build_outline;

/// 工具名称。
pub const PAPER_OUTLINE_TOOL_NAME: &str = "paper_outline";

/// 工具描述。
const DESCRIPTION: &str = "获取当前在写论文的大纲：返回由 H1-H6 标题组成的嵌套树（含层级与标题文本，不含正文）。撰写或修改章节前先调用本工具了解论文结构，再用 paper_section 读取具体章节。";

/// 论文大纲读取工具。
pub struct PaperOutlineTool {
    /// 输入参数 JSON Schema。
    parameters: Value,
    /// 论文全文读取器（referee `read` 封装）。
    reader: PaperReader,
}

impl PaperOutlineTool {
    /// 构造工具（`project_path` 为论文项目根目录）。
    pub fn new(project_path: String) -> Self {
        Self {
            parameters: json!({"type": "object"}),
            reader: PaperReader::new(project_path),
        }
    }
}

#[async_trait]
impl Tool for PaperOutlineTool {
    fn name(&self) -> &str {
        PAPER_OUTLINE_TOOL_NAME
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn input_schema(&self) -> Value {
        self.parameters.clone()
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Remote
    }

    /// 只读查询工具，默认同步返回结果（LLM 等待本轮调用完成）。
    fn default_wait(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        _ctx: ToolContext,
        _args: Value,
    ) -> Result<ToolOutput, ToolError> {
        let md = self.reader.read_full().await?;
        let outline = build_outline(&md);

        Ok(ToolOutput::from_json(&json!({
            "heading_count": count_nodes(&outline),
            "outline": outline,
        })))
    }
}

/// 递归统计大纲树节点数。
fn count_nodes(nodes: &[crate::agent_tools::parse::OutlineNode]) -> usize {
    nodes
        .iter()
        .map(|n| 1 + count_nodes(&n.children))
        .sum()
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::test_support::{build_test_project, ctx};
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn outline_returns_heading_tree() {
        let (storage, project_dir) = build_test_project("outline_tree");
        let tool = PaperOutlineTool::new(project_dir.to_string_lossy().to_string());

        let result = super::super::output_json(
            tool.execute(ctx(), json!({})).await.unwrap(),
        );
        assert_eq!(result["heading_count"], 4);

        let outline = result["outline"].as_array().unwrap();
        assert_eq!(outline.len(), 2);
        assert_eq!(outline[0]["level"], 1);
        assert_eq!(outline[0]["text"], "引言");
        assert_eq!(outline[0]["children"][0]["text"], "研究背景");
        assert_eq!(
            outline[0]["children"][0]["children"][0]["text"],
            "国内现状"
        );
        assert_eq!(outline[1]["text"], "方法");
        assert!(outline[1]["children"].as_array().unwrap().is_empty());

        let _ = fs::remove_dir_all(&storage);
    }

    #[tokio::test]
    async fn outline_of_empty_paper_is_empty_array() {
        // 新建项目 main.md 为空 → 大纲为空数组而非错误
        use crate::project::creator;
        use crate::project::model::{CreateProjectRequest, sanitize_project_name};

        let storage =
            super::super::test_support::temp_project_dir("outline_empty");
        let request = CreateProjectRequest {
            title: "空文章".into(),
            author: "张三".into(),
            project_name: sanitize_project_name("空文章"),
            storage_path: storage.to_string_lossy().to_string(),
            description: None,
        };
        let project_dir = creator::create_project(request).unwrap();

        let tool = PaperOutlineTool::new(project_dir);
        let result =
            super::super::output_json(tool.execute(ctx(), json!({})).await.unwrap());
        assert_eq!(result["heading_count"], 0);
        assert!(result["outline"].as_array().unwrap().is_empty());

        let _ = fs::remove_dir_all(&storage);
    }

    #[tokio::test]
    async fn outline_rejects_missing_project() {
        let ghost = super::super::test_support::temp_project_dir("outline_ghost")
            .join("ghost")
            .display()
            .to_string();
        let tool = PaperOutlineTool::new(ghost);
        let err = tool.execute(ctx(), json!({})).await.unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));
    }
}
