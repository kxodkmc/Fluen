//! 项目文件操作工具——供智能体在论文项目目录内读写、编辑文件。
//!
//! 类似 AI IDE 的 Write / Edit 文件工具，但**作用域限定在论文项目根目录内**，
//! 实现 referee [`Tool`] trait，捕获 `project_path` 于构造时
//! （与论文内容 `PaperContentTool` 相同的注入方式）。
//!
//! ## 支持的操作（`action` 参数）
//!
//! - `read`：读取文件内容（限制单文件最大 5 MiB，防止刷爆上下文）
//! - `write`：整体写入（创建或覆盖，自动创建缺失的父目录）
//! - `edit`：`old_string` → `new_string` 精确替换（**必须唯一匹配**，防止误改）
//! - `append`：追加内容到文件末尾
//!
//! ## 安全边界
//!
//! - 所有 `path` 均为**相对项目根**的路径；拒绝绝对路径、空路径与 `..` 穿越
//! - 解析后对目标父目录做 `canonicalize`，确认仍在项目根内（防符号链接逃逸）
//! - 拒绝操作 `.git` 内部文件（保护版本控制元数据）
//! - **写操作需用户确认**：write / edit / append 由装配层的
//!   [`ApprovalGuard`](crate::agent_runtime::approval::ApprovalGuard)
//!   包装弹窗征求用户同意后才落盘（read 免审批）。
//!
//! ## 定位
//!
//! 本工具为**通用文件操作**，不感知 Fluen 章节结构（章节拆分 / sections.json
//! 同步由论文专用模块负责）。若需修改论文正文并同步章节索引，
//! 应使用论文内容工具或直接调用 `project::section` 模块。

use std::path::{Component, Path, PathBuf};

use async_trait::async_trait;
use referee_ai::tool::{Tool, ToolContext, ToolError, ToolOutput};
use serde_json::{json, Value};

/// 工具名称。
pub const PROJECT_FILE_TOOL_NAME: &str = "project_file";

/// 工具描述。
const DESCRIPTION: &str = "在论文项目内读写、编辑文件：可读取、整体写入（创建/覆盖）、精确替换、追加内容。所有路径相对项目根，禁止越界（.. / 绝对路径 / .git）。注意：write / edit / append 属于写操作，执行前会弹出确认框，需用户点击「应用」后才真正写入文件；read 无需确认。";

/// 受保护路径前缀（相对项目根，保护版本控制等元数据）。
const PROTECTED_PREFIXES: &[&str] = &[".git"];

/// 单文件最大字节数（5 MiB），防止智能体意外写入/读取超大文件。
const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;

/// 项目文件操作工具。
pub struct ProjectFileTool {
    /// 输入参数 JSON Schema。
    parameters: Value,
    /// 论文项目根路径（构造时注入，与当前打开的项目绑定）。
    project_path: PathBuf,
}

impl ProjectFileTool {
    /// 构造工具。
    pub fn new(project_path: String) -> Self {
        let parameters = json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["read", "write", "edit", "append"],
                    "description": "操作类型：read=读取文件；write=整体写入（创建/覆盖，自动创建父目录）；edit=精确替换（old_string 必须唯一匹配）；append=追加内容到文件末尾"
                },
                "path": {
                    "type": "string",
                    "description": "相对项目根的文件路径，如 manuscript/main.md、references/references-index.json。禁止绝对路径与 .."
                },
                "content": {
                    "type": "string",
                    "description": "write 时的完整新内容；append 时追加到末尾的内容（以 / 开头可避免粘连上一行）"
                },
                "old_string": {
                    "type": "string",
                    "description": "edit 时被替换的原文，必须唯一匹配（建议包含足够上下文以保证唯一）"
                },
                "new_string": {
                    "type": "string",
                    "description": "edit 时的替换文本"
                }
            },
            "required": ["action", "path"]
        });

        Self {
            parameters,
            project_path: PathBuf::from(project_path),
        }
    }

    /// 解析并校验相对路径，返回项目根内的完整路径。
    ///
    /// 词法层拒绝绝对路径 / 空路径 / `..`；随后对目标父目录 `canonicalize`
    /// 并确认在项目根内。`create_parent` 为 `true`（write）时自动创建缺失父目录。
    fn resolve_path(&self, rel: &str, create_parent: bool) -> Result<PathBuf, ToolError> {
        let rel = rel.trim();
        if rel.is_empty() {
            return Err(ToolError::InvalidArguments("path 不能为空".into()));
        }

        let rel_path = Path::new(rel);
        if rel_path.is_absolute() {
            return Err(ToolError::InvalidArguments(format!(
                "path 必须是相对项目根的路径: {}",
                rel
            )));
        }

        // 词法校验组件：仅允许 Normal / CurDir，拒绝 ParentDir 与其他（如 RootDir）
        let mut has_normal = false;
        for comp in rel_path.components() {
            match comp {
                Component::Normal(_) => has_normal = true,
                Component::CurDir => {}
                Component::ParentDir => {
                    return Err(ToolError::InvalidArguments(format!(
                        "path 不允许包含 '..': {}",
                        rel
                    )));
                }
                _ => {
                    return Err(ToolError::InvalidArguments(format!(
                        "path 包含非法组件: {}",
                        rel
                    )));
                }
            }
        }
        if !has_normal {
            return Err(ToolError::InvalidArguments("path 不能为空".into()));
        }

        let joined = self.project_path.join(rel_path);

        // 拒绝受保护路径（.git 等）
        for prefix in PROTECTED_PREFIXES {
            if joined.starts_with(self.project_path.join(prefix)) {
                return Err(ToolError::InvalidArguments(format!(
                    "禁止访问受保护路径: {}",
                    rel
                )));
            }
        }

        // 最终目标若为符号链接则拒绝（防读写逃逸到项目外）
        match std::fs::symlink_metadata(&joined) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err(ToolError::InvalidArguments(format!(
                        "拒绝操作符号链接（可能指向项目外）: {}",
                        rel
                    )));
                }
            }
            // 文件不存在：允许（write / append 可创建）
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(ToolError::Execution(format!(
                    "无法访问 {}: {}",
                    joined.display(),
                    e
                )));
            }
        }

        // 父目录 canonicalize 校验：确认仍在项目根内（防符号链接逃逸）
        let parent = joined
            .parent()
            .ok_or_else(|| ToolError::InvalidArguments(format!("path 无父目录: {}", rel)))?;
        if create_parent && !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ToolError::Execution(format!("创建目录失败 {}: {}", parent.display(), e))
            })?;
        }
        let canon_parent = parent.canonicalize().map_err(|e| {
            ToolError::Execution(format!("目录不存在或不可访问 {}: {}", parent.display(), e))
        })?;
        let canon_root = self.project_path.canonicalize().map_err(|e| {
            ToolError::Execution(format!("项目根不可访问 {}: {}", self.project_path.display(), e))
        })?;
        if !canon_parent.starts_with(&canon_root) {
            return Err(ToolError::InvalidArguments(format!(
                "path 超出项目根目录: {}",
                rel
            )));
        }

        Ok(joined)
    }

    /// 读取文件内容（带大小限制）。
    fn read_file_content(&self, full_path: &Path, rel: &str) -> Result<String, ToolError> {
        let metadata = std::fs::metadata(full_path).map_err(|e| {
            ToolError::Execution(format!("读取元信息失败 {}: {}", full_path.display(), e))
        })?;
        if metadata.len() > MAX_FILE_BYTES {
            return Err(ToolError::Execution(format!(
                "文件过大（{} 字节，上限 {}），拒绝读取: {}",
                metadata.len(),
                MAX_FILE_BYTES,
                rel
            )));
        }
        std::fs::read_to_string(full_path).map_err(|e| {
            ToolError::Execution(format!("读取文件失败 {}: {}", full_path.display(), e))
        })
    }

    /// 读取文件内容并包装为工具返回值。
    fn read_file(&self, full_path: &Path, rel: &str) -> Result<Value, ToolError> {
        let content = self.read_file_content(full_path, rel)?;
        Ok(json!({
            "path": rel,
            "content": content,
            "bytes": content.len(),
        }))
    }

    /// 写入文件（write / edit / append 共用；校验结果大小并确保父目录存在）。
    fn write_file(&self, full_path: &Path, content: &str, rel: &str) -> Result<Value, ToolError> {
        if content.len() as u64 > MAX_FILE_BYTES {
            return Err(ToolError::InvalidArguments(format!(
                "内容过大（{} 字节，上限 {}）: {}",
                content.len(),
                MAX_FILE_BYTES,
                rel
            )));
        }
        let parent = full_path
            .parent()
            .ok_or_else(|| ToolError::InvalidArguments(format!("path 无父目录: {}", rel)))?;
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ToolError::Execution(format!("创建目录失败 {}: {}", parent.display(), e))
            })?;
        }
        std::fs::write(full_path, content).map_err(|e| {
            ToolError::Execution(format!("写入文件失败 {}: {}", full_path.display(), e))
        })?;
        Ok(json!({
            "path": rel,
            "bytes_written": content.len(),
        }))
    }
}

#[async_trait]
impl Tool for ProjectFileTool {
    fn name(&self) -> &str {
        PROJECT_FILE_TOOL_NAME
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn input_schema(&self) -> Value {
        self.parameters.clone()
    }

    /// 文件读写需同步拿到结果（LLM 等待本轮调用完成再继续生成）。
    fn default_wait(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        _ctx: ToolContext,
        args: Value,
    ) -> Result<ToolOutput, ToolError> {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments("缺少 action 参数".into()))?;
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| ToolError::InvalidArguments("缺少 path 参数".into()))?;

        let result = match action {
            "read" => {
                let full_path = self.resolve_path(path, false)?;
                self.read_file(&full_path, path)?
            }
            "write" => {
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments("write 需要 content 参数".into()))?;
                let full_path = self.resolve_path(path, true)?;
                self.write_file(&full_path, content, path)?
            }
            "append" => {
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments("append 需要 content 参数".into()))?;
                let full_path = self.resolve_path(path, true)?;
                let existing = if full_path.exists() {
                    self.read_file_content(&full_path, path)?
                } else {
                    String::new()
                };
                let merged = format!("{}{}", existing, content);
                self.write_file(&full_path, &merged, path)?;
                // 报告实际追加的字节数（content 参数长度）
                json!({
                    "path": path,
                    "bytes_appended": content.len(),
                })
            }
            "edit" => {
                let old_string = args
                    .get("old_string")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments("edit 需要 old_string 参数".into()))?;
                let new_string = args
                    .get("new_string")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments("edit 需要 new_string 参数".into()))?;
                if old_string.is_empty() {
                    return Err(ToolError::InvalidArguments("old_string 不能为空".into()));
                }
                let full_path = self.resolve_path(path, false)?;
                let content = self.read_file_content(&full_path, path)?;

                // 统计精确子串匹配次数（要求恰好一次）
                let mut count = 0usize;
                let mut search_from = 0usize;
                while let Some(idx) = content[search_from..].find(old_string) {
                    count += 1;
                    search_from += idx + old_string.len();
                }

                match count {
                    0 => {
                        return Err(ToolError::InvalidArguments(format!(
                            "old_string 未找到（文件共 {} 字节）: {}",
                            content.len(),
                            path
                        )))
                    }
                    1 => {
                        let new_content = content.replacen(old_string, new_string, 1);
                        self.write_file(&full_path, &new_content, path)?
                    }
                    n => {
                        return Err(ToolError::InvalidArguments(format!(
                            "old_string 匹配 {} 次，请提供更多上下文使其唯一: {}",
                            n, path
                        )))
                    }
                }
            }
            other => {
                return Err(ToolError::InvalidArguments(format!(
                    "未知 action: {}（可选值: read / write / edit / append）",
                    other
                )))
            }
        };

        Ok(ToolOutput::from_json(&result))
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 创建临时项目目录。
    fn temp_project_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluen_file_tool_test_{}_{:?}_{}",
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

    /// 构造最小可用的 ToolContext。
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

    /// 解析工具输出为 JSON（工具返回值均为 `ToolOutput::from_json` 构造）。
    fn output_json(output: ToolOutput) -> Value {
        serde_json::from_str(&output.content).unwrap()
    }

    fn make_tool() -> (PathBuf, ProjectFileTool) {
        let dir = temp_project_dir();
        let tool = ProjectFileTool::new(dir.to_string_lossy().to_string());
        (dir, tool)
    }

    #[tokio::test]
    async fn write_creates_file_and_reads_back() {
        let (dir, tool) = make_tool();

        let result = output_json(
            tool.execute(
                ctx(),
                json!({
                    "action": "write",
                    "path": "notes/scratch.md",
                    "content": "# 草稿\n\n正文内容"
                }),
            )
            .await
            .unwrap(),
        );
        assert_eq!(result["path"], "notes/scratch.md");
        assert!(dir.join("notes").join("scratch.md").exists());

        // read 读回
        let read = output_json(
            tool.execute(ctx(), json!({ "action": "read", "path": "notes/scratch.md" }))
                .await
                .unwrap(),
        );
        assert_eq!(read["content"], "# 草稿\n\n正文内容");

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn write_overwrites_existing() {
        let (dir, tool) = make_tool();
        fs::write(dir.join("a.txt"), "old").unwrap();

        let result = output_json(
            tool.execute(
                ctx(),
                json!({ "action": "write", "path": "a.txt", "content": "new" }),
            )
            .await
            .unwrap(),
        );
        assert_eq!(result["bytes_written"], 3);
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "new");

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn edit_unique_match_succeeds() {
        let (dir, tool) = make_tool();
        fs::write(dir.join("main.md"), "# 引言\n\n引言正文\n\n## 背景\n\n背景正文").unwrap();

        let result = output_json(
            tool.execute(
                ctx(),
                json!({
                    "action": "edit",
                    "path": "main.md",
                    "old_string": "# 引言",
                    "new_string": "# 绪论"
                }),
            )
            .await
            .unwrap(),
        );
        assert_eq!(result["path"], "main.md");
        let content = fs::read_to_string(dir.join("main.md")).unwrap();
        assert!(content.contains("# 绪论"));
        assert!(!content.contains("# 引言"));
        assert!(content.contains("## 背景")); // 其余内容不变

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn edit_not_found_errors() {
        let (dir, tool) = make_tool();
        fs::write(dir.join("main.md"), "# 引言\n\n正文").unwrap();

        let err = tool
            .execute(
                ctx(),
                json!({
                    "action": "edit",
                    "path": "main.md",
                    "old_string": "# 不存在",
                    "new_string": "# 新"
                }),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));
        assert!(err.to_string().contains("未找到"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn edit_ambiguous_match_errors() {
        let (dir, tool) = make_tool();
        // "背景" 出现两次 → 拒绝编辑
        fs::write(dir.join("main.md"), "# 引言\n\n## 背景\n\n背景正文").unwrap();

        let err = tool
            .execute(
                ctx(),
                json!({
                    "action": "edit",
                    "path": "main.md",
                    "old_string": "背景",
                    "new_string": "研究背景"
                }),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));
        assert!(err.to_string().contains("匹配 2 次"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn append_adds_to_end() {
        let (dir, tool) = make_tool();
        fs::write(dir.join("log.txt"), "第一行").unwrap();

        let result = output_json(
            tool.execute(
                ctx(),
                json!({
                    "action": "append",
                    "path": "log.txt",
                    "content": "\n第二行"
                }),
            )
            .await
            .unwrap(),
        );
        // "\n第二行" = 1 + 9 字节（UTF-8）
        assert_eq!(result["bytes_appended"], 10);
        assert_eq!(fs::read_to_string(dir.join("log.txt")).unwrap(), "第一行\n第二行");

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn append_creates_file_when_missing() {
        let (dir, tool) = make_tool();

        let result = output_json(
            tool.execute(
                ctx(),
                json!({ "action": "append", "path": "new.txt", "content": "hello" }),
            )
            .await
            .unwrap(),
        );
        assert_eq!(result["bytes_appended"], 5);
        assert_eq!(fs::read_to_string(dir.join("new.txt")).unwrap(), "hello");

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn path_traversal_rejected() {
        let (dir, tool) = make_tool();

        // 绝对路径
        let err = tool
            .execute(ctx(), json!({ "action": "read", "path": "/etc/passwd" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));

        // .. 穿越
        let err = tool
            .execute(ctx(), json!({ "action": "read", "path": "../secret.txt" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));

        // 嵌套 .. 穿越
        let err = tool
            .execute(ctx(), json!({ "action": "read", "path": "a/../../secret.txt" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn protected_git_path_rejected() {
        let (dir, tool) = make_tool();
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::write(dir.join(".git").join("config"), "[core]").unwrap();

        let err = tool
            .execute(ctx(), json!({ "action": "read", "path": ".git/config" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));
        assert!(err.to_string().contains("受保护"));

        // 嵌套在 .git 下的路径同样拒绝
        let err = tool
            .execute(ctx(), json!({ "action": "read", "path": ".git/objects/aa/bb" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn read_missing_file_errors() {
        let (dir, tool) = make_tool();

        let err = tool
            .execute(ctx(), json!({ "action": "read", "path": "ghost.md" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn missing_action_or_path_rejected() {
        let (_, tool) = make_tool();

        let err = tool.execute(ctx(), json!({ "path": "a.txt" })).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));

        let err = tool.execute(ctx(), json!({ "action": "read" })).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));

        let err = tool
            .execute(ctx(), json!({ "action": "bogus", "path": "a.txt" }))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments(_)));
    }

    #[tokio::test]
    async fn symlink_escape_rejected() {
        // 项目内符号链接指向外部目录时，write/read 均不得逃逸（仅 Unix 可建符号链接）
        let (dir, tool) = make_tool();
        let outside = temp_project_dir();
        fs::write(outside.join("secret.txt"), "secret").unwrap();

        #[cfg(unix)]
        {
            // 目录符号链接：父目录 canonicalize 校验拦截
            std::os::unix::fs::symlink(&outside, dir.join("link")).unwrap();
            let err = tool
                .execute(ctx(), json!({ "action": "read", "path": "link/secret.txt" }))
                .await
                .unwrap_err();
            assert!(matches!(err, ToolError::InvalidArguments(_)), "目录符号链接逃逸应被拒绝");

            // 文件符号链接：最终目标检查拦截
            std::os::unix::fs::symlink(outside.join("secret.txt"), dir.join("secret_link.txt"))
                .unwrap();
            let err = tool
                .execute(ctx(), json!({ "action": "read", "path": "secret_link.txt" }))
                .await
                .unwrap_err();
            assert!(matches!(err, ToolError::InvalidArguments(_)), "文件符号链接读取应被拒绝");
            let err = tool
                .execute(
                    ctx(),
                    json!({ "action": "write", "path": "secret_link.txt", "content": "evil" }),
                )
                .await
                .unwrap_err();
            assert!(matches!(err, ToolError::InvalidArguments(_)), "文件符号链接写入应被拒绝");
        }
        #[cfg(not(unix))]
        {
            // Windows 无权限时跳过；有权限时同样应被 canonicalize / symlink 检查拦截
            if std::os::windows::fs::symlink_dir(&outside, dir.join("link")).is_ok() {
                let err = tool
                    .execute(ctx(), json!({ "action": "read", "path": "link/secret.txt" }))
                    .await
                    .unwrap_err();
                assert!(matches!(err, ToolError::InvalidArguments(_)), "符号链接逃逸应被拒绝");
            }
        }

        let _ = fs::remove_dir_all(&outside);
        let _ = fs::remove_dir_all(&dir);
    }
}
