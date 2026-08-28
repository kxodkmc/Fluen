//! `project_edit` 工具——精确替换论文项目内文件中的字面文本。
//!
//! 单一职责：只做文本精确编辑。路径相对项目根（经 [`ProjectFs`](super::ProjectFs)
//! 校验，拒绝触碰 `.git` 与论文正文），编辑委托 referee [`EditTool`]——
//! `old_string` 必须恰好出现一次（多处需 `replace_all=true`），
//! 拒绝二进制 / 非 UTF-8 文件，写入为原子操作。本工具需经审批包装后注册。

use async_trait::async_trait;
use referee_ai::tool::{Tool, ToolCategory, ToolContext, ToolError, ToolOutput};
use serde_json::{json, Value};

use super::ProjectFs;

/// 工具名称。
pub const PROJECT_EDIT_TOOL_NAME: &str = "project_edit";

/// 工具描述。
const DESCRIPTION: &str = "精确替换论文项目内文件中的字面文本：old_string 必须恰好出现一次，多处需 replace_all=true；二进制文件拒绝编辑；写入为原子操作。path 相对项目根，禁止越界与 .git；论文正文 manuscript/main.md 受保护——修改正文请使用 manuscript 工具。执行前会弹出确认框，需用户点击「应用」后才真正保存。前置条件：必须先通过 project_read 完整读取目标文件原文（所有窗口覆盖全文且文件未变更），否则拒绝执行；禁止用 project_write 做局部修改，局部修改一律走本工具。";

/// 项目文件编辑工具。
pub struct ProjectEditTool {
    /// 输入参数 JSON Schema。
    parameters: Value,
    /// 路径安全层。
    fs: ProjectFs,
}

impl ProjectEditTool {
    /// 构造工具（`project_path` 为论文项目根目录）。
    pub fn new(project_path: String) -> Self {
        let parameters = json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "相对项目根的文件路径。禁止绝对路径与 .."
                },
                "old_string": {
                    "type": "string",
                    "description": "被替换的原文（非空），建议包含足够上下文以保证唯一匹配"
                },
                "new_string": {
                    "type": "string",
                    "description": "替换后的文本（空串 = 删除该段文本）"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "是否替换所有匹配，默认 false（此时 old_string 必须唯一）"
                }
            },
            "required": ["path", "old_string", "new_string"]
        });
        Self {
            parameters,
            fs: ProjectFs::new(project_path),
        }
    }
}

#[async_trait]
impl Tool for ProjectEditTool {
    fn name(&self) -> &str {
        PROJECT_EDIT_TOOL_NAME
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

    /// 编辑结果需同步反馈（LLM 等待落盘完成再继续生成）。
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
        // 其余参数校验由 referee edit 完成（old_string 非空、唯一匹配语义）
        let abs = self.fs.resolve_for_write(path, false)?;

        let mut edit_args = json!({ "file_path": abs.display().to_string() });
        for key in ["old_string", "new_string", "replace_all"] {
            if let Some(v) = args.get(key) {
                edit_args[key] = v.clone();
            }
        }

        // 委托 referee edit：唯一匹配强制 + 二进制拒绝 + 原子写
        self.fs.edit_tool().execute(self.fs.ctx(), edit_args).await
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
    async fn unique_replacement_applies() {
        let (dir, _fs) = make_fs("edit_unique");
        fs::write(dir.join("index.json"), "{\"note\": \"草稿\", \"n\": 1}").unwrap();
        let tool = ProjectEditTool::new(dir.to_string_lossy().to_string());

        let out = tool
            .execute(
                ctx(),
                json!({ "path": "index.json", "old_string": "草稿", "new_string": "定稿" }),
            )
            .await
            .unwrap();
        let v: Value = serde_json::from_str(&out.content).unwrap();
        assert_eq!(v["replacements"], 1);
        assert_eq!(
            fs::read_to_string(dir.join("index.json")).unwrap(),
            "{\"note\": \"定稿\", \"n\": 1}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn ambiguous_match_rejected_unless_replace_all() {
        let (dir, _fs) = make_fs("edit_ambiguous");
        fs::write(dir.join("log.txt"), "todo todo todo").unwrap();
        let tool = ProjectEditTool::new(dir.to_string_lossy().to_string());

        let err = tool
            .execute(ctx(), json!({ "path": "log.txt", "old_string": "todo", "new_string": "done" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));
        assert!(err.to_string().contains("replace_all"));

        let out = tool
            .execute(
                ctx(),
                json!({ "path": "log.txt", "old_string": "todo", "new_string": "done", "replace_all": true }),
            )
            .await
            .unwrap();
        assert_eq!(fs::read_to_string(dir.join("log.txt")).unwrap(), "done done done");
        let v: Value = serde_json::from_str(&out.content).unwrap();
        assert_eq!(v["replacements"], 3);

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn main_md_and_missing_args_rejected() {
        let (dir, _fs) = make_fs("edit_bad");
        std::fs::create_dir_all(dir.join("manuscript")).unwrap();
        std::fs::write(dir.join("manuscript").join("main.md"), "# 正文").unwrap();
        let tool = ProjectEditTool::new(dir.to_string_lossy().to_string());

        // 论文正文旁路封死：提示走 manuscript
        let err = tool
            .execute(
                ctx(),
                json!({ "path": "manuscript/main.md", "old_string": "# 正文", "new_string": "# 改" }),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));
        assert!(err.to_string().contains("manuscript"));

        let err = tool
            .execute(ctx(), json!({ "path": "a.txt" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn binary_file_rejected() {
        let (dir, _fs) = make_fs("edit_binary");
        fs::write(dir.join("blob.bin"), b"\x00\x01\x02 payload").unwrap();
        let tool = ProjectEditTool::new(dir.to_string_lossy().to_string());

        let err = tool
            .execute(ctx(), json!({ "path": "blob.bin", "old_string": "x", "new_string": "y" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));

        let _ = fs::remove_dir_all(&dir);
    }
}
