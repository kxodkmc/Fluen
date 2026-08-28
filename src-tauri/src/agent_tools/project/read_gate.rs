//! 写前必读门（read-before-write gate）装饰器。
//!
//! 与 [`ApprovalGuard`](crate::agent_runtime::approval::ApprovalGuard) 同构的
//! 透传壳，包在写工具（`project_write` / `project_edit` / `manuscript`）外层：
//! 执行前检查目标文件是否已被 [`ReadTracker`] 记为「完整读取且未变更」，
//! 未满足则以错误反馈给 LLM（促其先用 `project_read` 读完再改），
//! 且**先于审批弹窗**拦截——不给用户弹一个注定丢内容的确认框。
//!
//! - 目标文件不存在（新建写入）→ 放行；
//! - 路径本身非法（越界 / `.git` / 正文旁路）→ 放行给内层工具报错；
//! - 写 / 编辑成功后刷新跟踪器指纹（模型对刚落盘的内容有完整认知）。

use std::sync::Arc;

use async_trait::async_trait;
use referee_ai::tool::{Tool, ToolCategory, ToolContext, ToolError, ToolOutput};
use serde_json::Value;

use crate::agent_tools::manuscript::MANUSCRIPT_TOOL_NAME;

use super::edit::PROJECT_EDIT_TOOL_NAME;
use super::read::PROJECT_READ_TOOL_NAME;
use super::read_state::ReadTracker;
use super::write::PROJECT_WRITE_TOOL_NAME;
use super::ProjectFs;

/// 论文正文相对路径（manuscript 工具的固定目标）。
const MANUSCRIPT_MAIN_MD: &str = "manuscript/main.md";

/// 写前必读门装饰器。
pub struct ReadGateGuard {
    /// 被包装的写工具。
    inner: Arc<dyn Tool>,
    /// 路径安全层（解析项目相对路径）。
    fs: ProjectFs,
    /// 读取状态跟踪器。
    tracker: Arc<ReadTracker>,
}

impl ReadGateGuard {
    /// 构造装饰器（`project_path` 为论文项目根目录）。
    pub fn new(
        inner: Arc<dyn Tool>,
        project_path: String,
        tracker: Arc<ReadTracker>,
    ) -> Self {
        Self {
            inner,
            fs: ProjectFs::new(project_path),
            tracker,
        }
    }

    /// 该工具调用的目标相对路径（owned，避免借住 args 阻碍其 move）；
    /// 非受门工具返回 `None`。
    fn target_rel(&self, args: &Value) -> Option<String> {
        let rel = match self.inner.name() {
            PROJECT_EDIT_TOOL_NAME | PROJECT_WRITE_TOOL_NAME => {
                args.get("path").and_then(|v| v.as_str())
            }
            MANUSCRIPT_TOOL_NAME => Some(MANUSCRIPT_MAIN_MD),
            _ => None,
        };
        rel.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    }
}

#[async_trait]
impl Tool for ReadGateGuard {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn input_schema(&self) -> Value {
        self.inner.input_schema()
    }

    fn category(&self) -> ToolCategory {
        self.inner.category()
    }

    fn default_wait(&self) -> bool {
        self.inner.default_wait()
    }

    fn depth_limited(&self) -> bool {
        self.inner.depth_limited()
    }

    async fn execute(&self, ctx: ToolContext, args: Value) -> Result<ToolOutput, ToolError> {
        // 门检查：目标文件已存在时，必须已被完整读取（新建文件不受约束）。
        // 路径非法（越界 / .git / 父目录缺失）时无法判定，放行给内层工具报错。
        let rel = self.target_rel(&args);
        let target = rel.as_ref().and_then(|r| self.fs.resolve_for_read(r).ok());

        if let Some(abs) = &target {
            if abs.is_file() && !self.tracker.is_fully_read(abs) {
                return Err(ToolError::Execution(format!(
                    "拒绝执行（本次调用未弹出确认框）：修改前必须先用 \
                     {PROJECT_READ_TOOL_NAME} 完整读取该文件的原文——从 offset 0 起逐窗口\
                     续读（offset = 上次返回的 end），直到 truncated=false，\
                     然后基于完整原文重新发起本次修改。"
                )));
            }
        }

        let output = self.inner.execute(ctx, args).await?;

        // 成功落盘后登记（覆盖写入 / 编辑 / 新建；嵌套新目录在写前无法解析，
        // 此时重解析），维持模型对文件内容的完整认知
        let recorded = target.or_else(|| rel.as_ref().and_then(|r| self.fs.resolve_for_read(r).ok()));
        if let Some(abs) = recorded.filter(|p| p.is_file()) {
            self.tracker.record_full(&abs);
        }
        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::test_support::temp_project_dir;
    use super::*;
    use crate::agent_tools::project::write::ProjectWriteTool;
    use serde_json::json;
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

    fn setup(tag: &str) -> (std::path::PathBuf, Arc<ReadTracker>) {
        let dir = temp_project_dir(tag);
        (dir, ReadTracker::new_arc())
    }

    #[tokio::test]
    async fn write_existing_without_read_is_blocked() {
        let (dir, tracker) = setup("gate_block");
        fs::write(dir.join("notes.txt"), "old\n").unwrap();
        let guard = ReadGateGuard::new(
            Arc::new(ProjectWriteTool::new(dir.to_string_lossy().to_string())),
            dir.to_string_lossy().to_string(),
            tracker.clone(),
        );

        let err = guard
            .execute(ctx(), json!({ "path": "notes.txt", "content": "new\n" }))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("project_read"));
        assert_eq!(fs::read_to_string(dir.join("notes.txt")).unwrap(), "old\n");

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn new_file_write_passes_and_records_full() {
        let (dir, tracker) = setup("gate_new");
        let guard = ReadGateGuard::new(
            Arc::new(ProjectWriteTool::new(dir.to_string_lossy().to_string())),
            dir.to_string_lossy().to_string(),
            tracker.clone(),
        );

        guard
            .execute(ctx(), json!({ "path": "fresh.md", "content": "# hi\n" }))
            .await
            .unwrap();
        // 新建后模型已知全文：再次覆盖不应被门拦截
        guard
            .execute(ctx(), json!({ "path": "fresh.md", "content": "# hi2\n" }))
            .await
            .unwrap();
        assert_eq!(fs::read_to_string(dir.join("fresh.md")).unwrap(), "# hi2\n");

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn read_then_write_passes() {
        let (dir, tracker) = setup("gate_read_first");
        fs::write(dir.join("a.md"), "# 委屈\n").unwrap();
        let abs = dir.join("a.md");
        let total = "# 委屈\n".chars().count();
        tracker.record_read(&abs, 0, total, total);

        let guard = ReadGateGuard::new(
            Arc::new(ProjectWriteTool::new(dir.to_string_lossy().to_string())),
            dir.to_string_lossy().to_string(),
            tracker.clone(),
        );
        guard
            .execute(ctx(), json!({ "path": "a.md", "content": "# 委屈改\n" }))
            .await
            .unwrap();
        assert_eq!(fs::read_to_string(&abs).unwrap(), "# 委屈改\n");
        assert!(tracker.is_fully_read(&abs), "写成功后应保持完整认知");

        let _ = fs::remove_dir_all(&dir);
    }
}
