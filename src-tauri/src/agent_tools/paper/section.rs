//! `paper_section` 工具——按标题引用提取当前论文的章节内容。
//!
//! 单一职责：只做章节定位与内容提取。实现 referee [`Tool`] trait，
//! 文件读取统一经 [`PaperReader`](super::PaperReader) 走 referee `read`。
//!
//! 标题引用规则（与 [`crate::agent_tools::parse::extract_section`] 一致）：
//! - `# 引言`：返回该一级章节全部内容（含其下 ##、### 子章节）
//! - `## 研究背景`：返回该二级章节及其子章节
//! - 省略 `#` 前缀：按标题文本匹配任意层级

use async_trait::async_trait;
use referee_ai::tool::{Tool, ToolCategory, ToolContext, ToolError, ToolOutput};
use serde_json::{json, Value};

use super::PaperReader;
use crate::agent_tools::parse::extract_section;

/// 工具名称。
pub const PAPER_SECTION_TOOL_NAME: &str = "paper_section";

/// 工具描述。
const DESCRIPTION: &str = "读取当前在写论文指定章节的内容：`# 引言` 返回该一级章节的全部内容（含其下 ##、### 子章节）；`## 背景` 返回该二级章节及其子章节；省略 # 前缀则按标题文本匹配任意层级。建议先调用 paper_outline 查看大纲再选择标题。";

/// 论文章节内容读取工具。
pub struct PaperSectionTool {
    /// 输入参数 JSON Schema。
    parameters: Value,
    /// 论文全文读取器（referee `read` 封装）。
    reader: PaperReader,
}

impl PaperSectionTool {
    /// 构造工具（`project_path` 为论文项目根目录）。
    pub fn new(project_path: String) -> Self {
        let parameters = json!({
            "type": "object",
            "properties": {
                "heading": {
                    "type": "string",
                    "description": "章节标题引用：'# 引言' 获取该一级章节及全部子章节；'## 背景' 获取该二级章节及其子章节；也可省略 # 前缀仅按标题文本匹配"
                }
            },
            "required": ["heading"]
        });
        Self {
            parameters,
            reader: PaperReader::new(project_path),
        }
    }
}

#[async_trait]
impl Tool for PaperSectionTool {
    fn name(&self) -> &str {
        PAPER_SECTION_TOOL_NAME
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
        args: Value,
    ) -> Result<ToolOutput, ToolError> {
        let heading = args
            .get("heading")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| ToolError::InvalidArguments("缺少 heading 参数（章节标题引用）".into()))?;

        let md = self.reader.read_full().await?;
        match extract_section(&md, heading) {
            Ok(section) => Ok(ToolOutput::from_json(&json!({
                "heading": format!(
                    "{} {}",
                    "#".repeat(section.heading.level as usize),
                    section.heading.text
                ),
                "content": section.content,
            }))),
            Err(e) => Err(ToolError::InvalidArguments(e.message)),
        }
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::test_support::{build_test_project, ctx};
    use super::*;
    use std::fs;

    /// 解析工具输出为 JSON。
    fn output_json(output: ToolOutput) -> Value {
        serde_json::from_str(&output.content).unwrap()
    }

    #[tokio::test]
    async fn h1_reference_returns_section_with_subsections() {
        let (storage, project_dir) = build_test_project("section_h1");
        let tool = PaperSectionTool::new(project_dir.to_string_lossy().to_string());

        let result = output_json(
            tool.execute(ctx(), json!({ "heading": "# 引言" }))
                .await
                .unwrap(),
        );
        assert_eq!(result["heading"], "# 引言");
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("## 研究背景"));
        assert!(content.contains("### 国内现状"));
        assert!(!content.contains("# 方法"));
        // 章节标记已剥离
        assert!(!content.contains("@sec_id"));

        let _ = fs::remove_dir_all(&storage);
    }

    #[tokio::test]
    async fn h2_reference_returns_subsection_scope() {
        let (storage, project_dir) = build_test_project("section_h2");
        let tool = PaperSectionTool::new(project_dir.to_string_lossy().to_string());

        let result = output_json(
            tool.execute(ctx(), json!({ "heading": "## 研究背景" }))
                .await
                .unwrap(),
        );
        assert_eq!(result["heading"], "## 研究背景");
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("### 国内现状"));
        assert!(!content.contains("## 研究意义") && !content.contains("# 方法"));

        let _ = fs::remove_dir_all(&storage);
    }

    #[tokio::test]
    async fn bare_text_matches_any_level() {
        let (storage, project_dir) = build_test_project("section_bare");
        let tool = PaperSectionTool::new(project_dir.to_string_lossy().to_string());

        let result = output_json(
            tool.execute(ctx(), json!({ "heading": "方法" }))
                .await
                .unwrap(),
        );
        assert_eq!(result["heading"], "# 方法");

        let _ = fs::remove_dir_all(&storage);
    }

    #[tokio::test]
    async fn not_found_lists_available_headings() {
        let (storage, project_dir) = build_test_project("section_missing");
        let tool = PaperSectionTool::new(project_dir.to_string_lossy().to_string());

        let err = tool
            .execute(ctx(), json!({ "heading": "# 不存在" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));
        assert!(err.to_string().contains("可用标题"));
        assert!(err.to_string().contains("引言"));

        let _ = fs::remove_dir_all(&storage);
    }

    #[tokio::test]
    async fn missing_heading_rejected() {
        let (storage, project_dir) = build_test_project("section_noarg");
        let tool = PaperSectionTool::new(project_dir.to_string_lossy().to_string());

        let err = tool.execute(ctx(), json!({})).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));

        let _ = fs::remove_dir_all(&storage);
    }

    #[tokio::test]
    async fn missing_project_rejected() {
        let ghost = super::super::test_support::temp_project_dir("section_ghost")
            .join("ghost")
            .display()
            .to_string();
        let tool = PaperSectionTool::new(ghost);
        let err = tool
            .execute(ctx(), json!({ "heading": "# 引言" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));
    }
}
