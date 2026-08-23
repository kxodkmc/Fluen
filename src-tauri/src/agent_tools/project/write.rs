//! `project_write` 工具——在论文项目内创建 / 整体替换文件。
//!
//! 单一职责：只做整文件写入。路径相对项目根（经 [`ProjectFs`](super::ProjectFs)
//! 校验，自动创建缺失父目录，拒绝触碰 `.git` 与论文正文），
//! 写入委托 referee [`WriteTool`]——同目录临时文件 + sync + rename 的
//! 原子发布，不会留下半写状态。本工具需经审批包装后注册。

use async_trait::async_trait;
use referee_ai::tool::{Tool, ToolCategory, ToolContext, ToolError, ToolOutput};
use serde_json::{json, Value};

use super::ProjectFs;

/// 工具名称。
pub const PROJECT_WRITE_TOOL_NAME: &str = "project_write";

/// 工具描述。
const DESCRIPTION: &str = "在论文项目内创建或整体替换文件：原子写入（临时文件 + rename，不留半写状态），自动创建缺失父目录。path 相对项目根，禁止越界与 .git；论文正文 manuscript/main.md 受保护——写正文请使用 manuscript 工具（自动格式校验与章节同步）。执行前会弹出确认框，需用户点击「应用」后才真正写入。";

/// 项目文件写入工具。
pub struct ProjectWriteTool {
    /// 输入参数 JSON Schema。
    parameters: Value,
    /// 路径安全层。
    fs: ProjectFs,
}

impl ProjectWriteTool {
    /// 构造工具（`project_path` 为论文项目根目录）。
    pub fn new(project_path: String) -> Self {
        let parameters = json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "相对项目根的文件路径，如 references/references-index.json、data/notes.csv。禁止绝对路径与 .."
                },
                "content": { "type": "string", "description": "完整文件内容" }
            },
            "required": ["path", "content"]
        });
        Self {
            parameters,
            fs: ProjectFs::new(project_path),
        }
    }
}

#[async_trait]
impl Tool for ProjectWriteTool {
    fn name(&self) -> &str {
        PROJECT_WRITE_TOOL_NAME
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

    /// 写入结果需同步反馈（LLM 等待落盘完成再继续生成）。
    fn default_wait(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        _ctx: ToolContext,
        args: Value,
    ) -> Result<ToolOutput, ToolError> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| ToolError::InvalidArguments("缺少 path 参数".into()))?;
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments("write 需要 content 参数".into()))?;

        // create_parent=true：保持「自动创建缺失父目录」的既有行为
        let abs = self.fs.resolve_for_write(path, true)?;

        // 委托 referee write：原子写（临时文件 + rename），返回 operation 元数据
        self.fs
            .write_tool()
            .execute(
                self.fs.ctx(),
                json!({
                    "file_path": abs.display().to_string(),
                    "content": content,
                }),
            )
            .await
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::test_support::make_fs;
    use super::*;
    use std::fs;

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

    #[tokio::test]
    async fn creates_file_and_reports_operation() {
        let (dir, _fs) = make_fs("write_create");
        let tool = ProjectWriteTool::new(dir.to_string_lossy().to_string());

        let out = tool
            .execute(
                ctx(),
                json!({ "path": "data/experiments/notes.csv", "content": "a,b\n1,2" }),
            )
            .await
            .unwrap();
        let v: Value = serde_json::from_str(&out.content).unwrap();
        assert_eq!(v["operation"], "create");
        assert_eq!(v["bytes_written"], 7);
        assert_eq!(
            fs::read_to_string(dir.join("data").join("experiments").join("notes.csv")).unwrap(),
            "a,b\n1,2"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn overwrites_existing_atomically() {
        let (dir, _fs) = make_fs("write_update");
        fs::write(dir.join("notes.txt"), "old").unwrap();
        let tool = ProjectWriteTool::new(dir.to_string_lossy().to_string());

        let out = tool
            .execute(ctx(), json!({ "path": "notes.txt", "content": "new" }))
            .await
            .unwrap();
        let v: Value = serde_json::from_str(&out.content).unwrap();
        assert_eq!(v["operation"], "update");
        assert_eq!(fs::read_to_string(dir.join("notes.txt")).unwrap(), "new");

        // 无临时文件残留（原子写清理验证）
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn main_md_and_git_and_traversal_rejected() {
        let (dir, _fs) = make_fs("write_bad");
        let tool = ProjectWriteTool::new(dir.to_string_lossy().to_string());

        // 论文正文旁路封死：提示走 manuscript
        let err = tool
            .execute(ctx(), json!({ "path": "manuscript/main.md", "content": "# x" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));
        assert!(err.to_string().contains("manuscript"));

        let err = tool
            .execute(ctx(), json!({ "path": ".git/hooks/x", "content": "y" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));

        let err = tool
            .execute(ctx(), json!({ "path": "../evil.txt", "content": "y" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));

        let err = tool.execute(ctx(), json!({ "path": "a.txt" })).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));
    }

    #[tokio::test]
    async fn oversized_content_rejected() {
        let (dir, _fs) = make_fs("write_big");
        let tool = ProjectWriteTool::new(dir.to_string_lossy().to_string());
        let big = "x".repeat(5 * 1024 * 1024 + 1);
        let err = tool
            .execute(ctx(), json!({ "path": "big.txt", "content": big }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));

        let _ = fs::remove_dir_all(&dir);
    }
}
