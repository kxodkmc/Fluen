//! `project_read` 工具——读取论文项目内的文件内容。
//!
//! 单一职责：只读。路径相对项目根（经 [`ProjectFs`](super::ProjectFs) 校验），
//! 文件读取委托 referee [`ReadTool`]——字符窗口按行/句边界对齐，
//! 返回 `offset` / `end` / `total_chars` / `truncated` 元数据，
//! 模型可据 `end` 续读，避免大文件刷爆上下文。

use async_trait::async_trait;
use referee_ai::tool::{Tool, ToolCategory, ToolContext, ToolError, ToolOutput};
use serde_json::{json, Value};

use super::ProjectFs;

/// 工具名称。
pub const PROJECT_READ_TOOL_NAME: &str = "project_read";

/// 工具描述。
const DESCRIPTION: &str = "读取论文项目内的文本文件：返回行/句边界对齐的字符窗口与实际区间（file_path/offset/end/total_chars/truncated/content），truncated=true 时用 offset=end 续读。path 相对项目根（如 references/references-index.json）；禁止访问 .git。";

/// 项目文件读取工具。
pub struct ProjectReadTool {
    /// 输入参数 JSON Schema。
    parameters: Value,
    /// 路径安全层。
    fs: ProjectFs,
}

impl ProjectReadTool {
    /// 构造工具（`project_path` 为论文项目根目录）。
    pub fn new(project_path: String) -> Self {
        let parameters = json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "相对项目根的文件路径，如 references/references-index.json、data/experiments/results.csv。禁止绝对路径与 .."
                },
                "offset": { "type": "integer", "minimum": 0, "description": "起始字符索引，默认 0" },
                "limit": { "type": "integer", "minimum": 1, "description": "窗口字符数上限，默认 3000" }
            },
            "required": ["path"]
        });
        Self {
            parameters,
            fs: ProjectFs::new(project_path),
        }
    }
}

#[async_trait]
impl Tool for ProjectReadTool {
    fn name(&self) -> &str {
        PROJECT_READ_TOOL_NAME
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
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| ToolError::InvalidArguments("缺少 path 参数".into()))?;
        let abs = self.fs.resolve_for_read(path)?;

        // offset / limit 可选：透传给 referee read 的字符窗口
        let mut read_args = json!({ "file_path": abs.display().to_string() });
        if let Some(offset) = args.get("offset") {
            read_args["offset"] = offset.clone();
        }
        if let Some(limit) = args.get("limit") {
            read_args["limit"] = limit.clone();
        }

        // 输出原样透传（referee read 已返回带续读元数据的结构化 JSON）
        self.fs.read_tool().execute(self.fs.ctx(), read_args).await
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
    async fn reads_file_with_window_metadata() {
        let (dir, _fs) = make_fs("read_basic");
        fs::create_dir_all(dir.join("references")).unwrap();
        fs::write(dir.join("references").join("index.json"), "{\"refs\": []}").unwrap();

        let tool = ProjectReadTool::new(dir.to_string_lossy().to_string());
        let out = tool
            .execute(ctx(), json!({ "path": "references/index.json" }))
            .await
            .unwrap();
        let v: Value = serde_json::from_str(&out.content).unwrap();
        assert_eq!(v["content"], "{\"refs\": []}");
        assert_eq!(v["truncated"], false);
        assert_eq!(v["total_chars"], 12);

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn windowed_read_paginates_via_offset() {
        let (dir, _fs) = make_fs("read_window");
        let text = "a".repeat(50);
        fs::write(dir.join("big.txt"), &text).unwrap();

        let tool = ProjectReadTool::new(dir.to_string_lossy().to_string());
        let first: Value = serde_json::from_str(
            &tool
                .execute(ctx(), json!({ "path": "big.txt", "limit": 20 }))
                .await
                .unwrap()
                .content,
        )
        .unwrap();
        assert_eq!(first["truncated"], true);
        assert_eq!(first["content"].as_str().unwrap().len(), 20);

        let second: Value = serde_json::from_str(
            &tool
                .execute(
                    ctx(),
                    json!({ "path": "big.txt", "offset": first["end"], "limit": 100 }),
                )
                .await
                .unwrap()
                .content,
        )
        .unwrap();
        assert_eq!(second["truncated"], false);
        assert_eq!(second["offset"], first["end"]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn missing_file_and_traversal_rejected() {
        let (dir, _fs) = make_fs("read_bad");
        let tool = ProjectReadTool::new(dir.to_string_lossy().to_string());

        let err = tool
            .execute(ctx(), json!({ "path": "ghost.md" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));

        let err = tool
            .execute(ctx(), json!({ "path": "../secret.txt" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));

        let err = tool.execute(ctx(), json!({})).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));
    }
}
