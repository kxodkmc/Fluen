//! 项目文件智能体工具——论文项目内通用文件的单一职责读写工具集。
//!
//! 对齐 referee「一工具一职责」规范，按能力拆分为三个工具：
//!
//! - [`read::ProjectReadTool`]（`project_read`）：读取文件（referee 字符窗口 + 续读元数据）
//! - [`write::ProjectWriteTool`]（`project_write`）：创建 / 整体替换文件（referee 原子写）
//! - [`edit::ProjectEditTool`]（`project_edit`）：精确替换字面文本（referee 唯一匹配编辑）
//!
//! 三者共享 [`ProjectFs`] 路径安全层，统一负责：
//!
//! - 相对路径解析（模型只需项目相对路径，不感知宿主机绝对路径）
//! - 拒绝绝对路径、空路径与 `..` 穿越；父目录 canonicalize 防符号链接逃逸
//! - 保护 `.git` 等版本控制元数据（读写均拒绝）
//! - **写保护 `manuscript/main.md`**：论文正文的唯一写入通道是
//!   [`crate::agent_tools::manuscript::ManuscriptEditTool`]（fluen-markup 校验 +
//!   章节同步），通用写/编辑工具拒绝触碰正文，杜绝绕过校验的旁路
//!
//! 文件原语全部委托 referee `ReadTool` / `WriteTool` / `EditTool`
//! （二进制嗅探、有界读取、原子落盘、唯一匹配强制均复用其实现）。
//!
//! 防丢内容两道机制（写前必读门，装配于 `assemble.rs`）：
//!
//! - [`read_state::ReadTracker`]：跟踪各文件是否已被 `project_read` 完整读取
//!   且未变更（多智能体共享，挂在 `MotisChatState`）。
//! - [`read_gate::ReadGateGuard`]：装饰写工具，未完整读取即拒绝执行，
//!   且先于审批弹窗拦截。

pub mod edit;
pub mod read;
pub mod read_gate;
pub mod read_state;
pub mod write;

use std::path::{Component, Path, PathBuf};

use referee_ai::tool::{ToolContext, ToolError};
use referee_agent::tool::{EditTool, FsConfig, ReadTool, ReadToolConfig, WriteTool};

/// 受保护路径前缀（相对项目根，保护版本控制等元数据，读写均拒绝）。
const PROTECTED_PREFIXES: &[&str] = &[".git"];

/// 论文正文相对路径（写入保护目标：仅 `manuscript` 工具可写）。
const MAIN_MD: [&str; 2] = ["manuscript", "main.md"];

/// 单文件读写字节上限（5 MiB），防止意外读写超大文件。
const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;

/// 项目内路径安全层——所有项目文件工具的路径入口。
///
/// 构造时绑定论文项目根；`resolve_for_*` 将模型给出的项目相对路径
/// 校验并解析为绝对路径，供内部委托的 referee 文件工具使用。
pub struct ProjectFs {
    project_path: String,
}

impl ProjectFs {
    /// 构造安全层（`project_path` 为论文项目根目录）。
    pub fn new(project_path: String) -> Self {
        Self { project_path }
    }

    /// 校验并解析**读取**目标的相对路径（目标须存在或为目录项，不创建任何内容）。
    pub fn resolve_for_read(&self, rel: &str) -> Result<PathBuf, ToolError> {
        self.resolve(rel, false, false)
    }

    /// 校验并解析**写入 / 编辑**目标的相对路径。
    ///
    /// - 额外保护论文正文 `manuscript/main.md`；
    /// - `create_parent` 为 `true` 时自动创建缺失父目录（write 需要，
    ///   edit 的目标文件已存在故无需）。
    pub fn resolve_for_write(&self, rel: &str, create_parent: bool) -> Result<PathBuf, ToolError> {
        self.resolve(rel, true, create_parent)
    }

    /// 构造以项目根为约束的 referee 读工具（保留默认字符窗口语义）。
    pub(crate) fn read_tool(&self) -> ReadTool {
        ReadTool::new(ReadToolConfig {
            max_file_bytes: MAX_FILE_BYTES,
            root: Some(self.root()),
            ..ReadToolConfig::default()
        })
    }

    /// 构造以项目根为约束的 referee 写工具（原子写）。
    pub(crate) fn write_tool(&self) -> WriteTool {
        WriteTool::new(FsConfig {
            max_file_bytes: MAX_FILE_BYTES,
            root: Some(self.root()),
        })
    }

    /// 构造以项目根为约束的 referee 编辑工具（唯一匹配 + 原子写）。
    pub(crate) fn edit_tool(&self) -> EditTool {
        EditTool::new(FsConfig {
            max_file_bytes: MAX_FILE_BYTES,
            root: Some(self.root()),
        })
    }

    /// 内部调用 referee 文件工具所需的最小 [`ToolContext`]。
    pub(crate) fn ctx(&self) -> ToolContext {
        ToolContext {
            tool_call_id: "project-fs".into(),
            session_id: uuid::Uuid::new_v4(),
            turn_id: 0,
            kernel: None,
            store: None,
            wait: true,
            peer_depth: 0,
        }
    }

    /// 项目根目录（供写前必读门定位章节索引 / 备份文件等派生路径）。
    pub(crate) fn root(&self) -> PathBuf {
        PathBuf::from(&self.project_path)
    }

    /// 路径校验与解析的共享实现。
    ///
    /// 1. 词法层：拒绝绝对路径 / 空路径 / `..` 与非法组件；
    /// 2. 保护层：拒绝 `.git` 等；变更类操作额外拒绝论文正文；
    /// 3. 链接层：目标为符号链接时拒绝（防读写逃逸到项目外）；
    /// 4. 容器层：父目录 canonicalize 后必须仍在项目根内。
    fn resolve(
        &self,
        rel: &str,
        mutating: bool,
        create_parent: bool,
    ) -> Result<PathBuf, ToolError> {
        let rel = rel.trim();
        if rel.is_empty() {
            return Err(ToolError::InvalidArguments("path 不能为空".into()));
        }
        let rel_path = Path::new(rel);
        if rel_path.is_absolute() {
            return Err(ToolError::InvalidArguments(format!(
                "path 必须是相对项目根的路径: {rel}"
            )));
        }

        // 词法校验组件：仅允许 Normal / CurDir，拒绝 ParentDir 与其他（如 RootDir）
        let mut normals: Vec<std::ffi::OsString> = Vec::new();
        for comp in rel_path.components() {
            match comp {
                Component::Normal(name) => normals.push(name.to_os_string()),
                Component::CurDir => {}
                Component::ParentDir => {
                    return Err(ToolError::InvalidArguments(format!(
                        "path 不允许包含 '..': {rel}"
                    )));
                }
                _ => {
                    return Err(ToolError::InvalidArguments(format!(
                        "path 包含非法组件: {rel}"
                    )));
                }
            }
        }
        if normals.is_empty() {
            return Err(ToolError::InvalidArguments("path 不能为空".into()));
        }

        let joined = self.root().join(rel_path);

        // 保护路径：.git 等元数据（读写均拒绝）
        for prefix in PROTECTED_PREFIXES {
            if joined.starts_with(self.root().join(prefix)) {
                return Err(ToolError::InvalidArguments(format!(
                    "禁止访问受保护路径: {rel}"
                )));
            }
        }
        // 论文正文写保护：唯一写入通道是 manuscript 工具
        if mutating && is_main_md(&normals) {
            return Err(ToolError::InvalidArguments(
                "manuscript/main.md 是论文正文，受格式校验与章节同步保护：\
                 请使用 manuscript 工具写入，而非本工具"
                    .into(),
            ));
        }

        // 最终目标若为符号链接则拒绝（防读写逃逸到项目外）
        match std::fs::symlink_metadata(&joined) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err(ToolError::InvalidArguments(format!(
                        "拒绝操作符号链接（可能指向项目外）: {rel}"
                    )));
                }
            }
            // 目标不存在：读取时报错由 referee 工具给出；写入允许创建
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(ToolError::Execution(format!(
                    "无法访问 {}: {e}",
                    joined.display()
                )));
            }
        }

        // 父目录 canonicalize 校验：确认仍在项目根内（防符号链接逃逸）
        let parent = joined
            .parent()
            .ok_or_else(|| ToolError::InvalidArguments(format!("path 无父目录: {rel}")))?;
        if create_parent && !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ToolError::Execution(format!("创建目录失败 {}: {e}", parent.display()))
            })?;
        }
        let canon_parent = parent.canonicalize().map_err(|e| {
            ToolError::Execution(format!("目录不存在或不可访问 {}: {e}", parent.display()))
        })?;
        let canon_root = self.root().canonicalize().map_err(|e| {
            ToolError::Execution(format!("项目根不可访问 {}: {e}", self.project_path))
        })?;
        if !canon_parent.starts_with(&canon_root) {
            return Err(ToolError::InvalidArguments(format!(
                "path 超出项目根目录: {rel}"
            )));
        }

        Ok(joined)
    }
}

/// 判断规范化后的相对组件是否指向论文正文 `manuscript/main.md`
///（Windows 文件系统大小写不敏感，按 ASCII 大小写折叠比较）。
fn is_main_md(normals: &[std::ffi::OsString]) -> bool {
    normals.len() == MAIN_MD.len()
        && normals
            .iter()
            .zip(MAIN_MD.iter())
            .all(|(a, b)| a.to_string_lossy().eq_ignore_ascii_case(b))
}

#[cfg(test)]
pub(crate) fn output_json(output: referee_ai::tool::ToolOutput) -> serde_json::Value {
    serde_json::from_str(&output.content).unwrap()
}

#[cfg(test)]
pub(crate) mod test_support {
    //! 项目文件工具测试共享辅助。

    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::ProjectFs;

    /// 创建临时项目目录（进程 + 线程 + 时间戳隔离，避免并行冲突）。
    pub fn temp_project_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluen_project_{tag}_{}_{:?}_{}",
            std::process::id(),
            std::thread::current().id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 构造绑定临时项目的安全层。
    pub fn make_fs(tag: &str) -> (PathBuf, ProjectFs) {
        let dir = temp_project_dir(tag);
        let fs = ProjectFs::new(dir.to_string_lossy().to_string());
        (dir, fs)
    }
}

// ---------------------------------------------------------------------------
// 单元测试：路径安全层
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::test_support::make_fs;
    use super::*;
    use std::fs;

    #[test]
    fn read_rejects_absolute_and_traversal_and_empty() {
        let (_dir, fs) = make_fs("guard_lexical");

        for bad in ["/etc/passwd", r"C:\Windows\evil", "../secret.txt", "a/../../b.txt", "", "   ", "."] {
            let err = fs.resolve_for_read(bad).unwrap_err();
            assert!(matches!(err, ToolError::InvalidArguments(_)), "bad={bad}");
        }
    }

    #[test]
    fn protected_git_paths_rejected_for_read_and_write() {
        let (dir, fs) = make_fs("guard_git");
        fs::create_dir_all(dir.join(".git")).unwrap();

        for op in [
            |fs: &ProjectFs| fs.resolve_for_read(".git/config"),
            |fs: &ProjectFs| fs.resolve_for_write(".git/config", true),
            |fs: &ProjectFs| fs.resolve_for_write(".git/objects/aa/bb", false),
        ] {
            let err = op(&fs).unwrap_err();
            assert!(matches!(err, ToolError::InvalidArguments(_)));
            assert!(err.to_string().contains("受保护"));
        }
    }

    #[test]
    fn main_md_write_blocked_but_read_allowed() {
        let (dir, fs) = make_fs("guard_main_md");
        fs::create_dir_all(dir.join("manuscript")).unwrap();
        fs::write(dir.join("manuscript").join("main.md"), "# 正文").unwrap();

        // 读不受限（结构化访问另有 paper_outline / paper_section）
        assert!(fs.resolve_for_read("manuscript/main.md").is_ok());

        for op in [
            |fs: &ProjectFs| fs.resolve_for_write("manuscript/main.md", true),
            |fs: &ProjectFs| fs.resolve_for_write("Manuscript/Main.MD", false),
            |fs: &ProjectFs| fs.resolve_for_write(r"manuscript\main.md", true),
        ] {
            let err = op(&fs).unwrap_err();
            assert!(matches!(err, ToolError::InvalidArguments(_)), "正文写应被拒绝");
            assert!(err.to_string().contains("manuscript"), "{err}");
        }
    }

    #[test]
    fn sibling_files_named_like_main_md_are_allowed() {
        let (_dir, fs) = make_fs("guard_sibling");
        // notes/manuscript/main.md 不是受保护的正文路径
        assert!(fs.resolve_for_write("notes/manuscript/main.md", true).is_ok());
        // manuscript/main.md.bak 同理
        assert!(fs.resolve_for_write("manuscript/main.md.bak", true).is_ok());
    }

    #[test]
    fn symlink_escape_rejected() {
        let (dir, fs) = make_fs("guard_symlink");
        let outside = test_support::temp_project_dir("guard_symlink_outside");
        fs::write(outside.join("secret.txt"), "secret").unwrap();

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, dir.join("link")).unwrap();
            let err = fs.resolve_for_read("link/secret.txt").unwrap_err();
            assert!(matches!(err, ToolError::InvalidArguments(_)));

            std::os::unix::fs::symlink(outside.join("secret.txt"), dir.join("leak.txt")).unwrap();
            assert!(fs.resolve_for_read("leak.txt").is_err());
            assert!(fs.resolve_for_write("leak.txt", false).is_err());
        }
        #[cfg(not(unix))]
        {
            if std::os::windows::fs::symlink_dir(&outside, dir.join("link")).is_ok() {
                let err = fs.resolve_for_read("link/secret.txt").unwrap_err();
                assert!(matches!(err, ToolError::InvalidArguments(_)));
            }
        }

        let _ = fs::remove_dir_all(&outside);
    }
}
