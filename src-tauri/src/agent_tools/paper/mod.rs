//! 论文类智能体工具——读取当前在写论文的单一职责工具集。
//!
//! 对齐 referee「一工具一职责」规范，按只读能力拆分为两个工具：
//!
//! - [`outline::PaperOutlineTool`]（`paper_outline`）：论文大纲（H1-H6 标题树）
//! - [`section::PaperSectionTool`]（`paper_section`）：按标题引用提取章节内容
//!
//! 两者共享 [`PaperReader`]：以 referee
//! [`ReadTool`](referee_agent::tool::ReadTool) 为唯一文件读取后端，
//! 复用其二进制嗅探、有界读取、项目根约束与字符窗口能力，
//! 并自动翻页拼接全文。

pub mod outline;
pub mod section;

use std::path::PathBuf;

use referee_ai::tool::{Tool, ToolContext, ToolError, ToolOutput};
use referee_agent::tool::{ReadTool, ReadToolConfig};
use serde_json::{json, Value};

/// 论文源文件相对路径（`manuscript/main.md` 为唯一权威正文，
/// 章节备份 `sec-*.md` 由保存流程同步生成，不直接读取）。
const PAPER_SOURCE_REL: &str = r"manuscript/main.md";

/// 单页窗口字符数（一次 `read` 调用读取的字符上限）。
///
/// 取大窗口减少翻页次数；超出时由 [`PaperReader::read_full`] 自动续页。
const WINDOW_CHARS: usize = 200_000;

/// 翻页次数上限（防御性约束，正常论文远达不到）。
const MAX_PAGES: usize = 32;

/// 基于 referee `read` 的论文全文读取器。
///
/// - 源文件缺失时触发旧版项目迁移（`open_project` 会从章节备份拼装并落盘
///   `manuscript/main.md`），保证后续读取总有磁盘源文件；
/// - 以绝对路径 + 项目根约束调用 referee `read`，按 `end`/`truncated`
///   元数据自动翻页拼接。
pub struct PaperReader {
    project_path: String,
    read: ReadTool,
}

impl PaperReader {
    /// 构造读取器（`project_path` 为论文项目根目录）。
    pub fn new(project_path: String) -> Self {
        let root = PathBuf::from(&project_path);
        Self {
            project_path,
            read: ReadTool::new(ReadToolConfig {
                default_limit_chars: WINDOW_CHARS,
                max_limit_chars: WINDOW_CHARS,
                root: Some(root),
                ..ReadToolConfig::default()
            }),
        }
    }

    /// 论文项目根目录（供章节记账定位备份文件等派生路径）。
    pub(crate) fn project_path(&self) -> &str {
        &self.project_path
    }

    /// 确保磁盘上存在可读的 `manuscript/main.md`。
    ///
    /// 旧版项目（仅有章节备份、无 main.md）经 `open_project` 迁移落盘；
    /// 同时完成 config.yaml / sections.json 的结构校验。
    fn ensure_source(&self) -> Result<(), ToolError> {
        let project_dir = PathBuf::from(&self.project_path);
        if !project_dir.is_dir() {
            return Err(ToolError::Execution(format!(
                "论文项目不存在或不是目录: {}",
                self.project_path
            )));
        }
        if !project_dir.join(PAPER_SOURCE_REL).is_file() {
            crate::project::loader::open_project(&self.project_path).map_err(|e| {
                ToolError::Execution(format!("论文项目加载失败（旧版迁移未完成）: {}", e))
            })?;
        }
        Ok(())
    }

    /// 经 referee `read` 分页读取论文全文（含章节标记，由解析层过滤）。
    pub async fn read_full(&self) -> Result<String, ToolError> {
        self.ensure_source()?;
        let file_path = PathBuf::from(&self.project_path)
            .join(PAPER_SOURCE_REL)
            .display()
            .to_string();

        let mut full = String::new();
        let mut offset = 0usize;
        for _ in 0..MAX_PAGES {
            let out = self
                .read
                .execute(
                    reader_ctx(),
                    json!({ "file_path": file_path, "offset": offset }),
                )
                .await?;
            let page: Value = serde_json::from_str(&out.content)
                .map_err(|e| ToolError::Execution(format!("read 输出解析失败: {e}")))?;
            full.push_str(page["content"].as_str().unwrap_or(""));

            if !page["truncated"].as_bool().unwrap_or(false) {
                return Ok(full);
            }
            offset = page["end"].as_u64().unwrap_or(0) as usize;
        }
        Err(ToolError::Execution(
            "论文过长，读取页数超出上限".into(),
        ))
    }
}

/// 内部调用 referee `read` 所需的最小 [`ToolContext`]。
fn reader_ctx() -> ToolContext {
    ToolContext {
        tool_call_id: "paper-reader".into(),
        session_id: uuid::Uuid::new_v4(),
        turn_id: 0,
        kernel: None,
        store: None,
        wait: true,
        peer_depth: 0,
    }
}

/// 解析工具输出为 JSON（工具返回值均为 `ToolOutput::from_json` 构造）。
#[cfg(test)]
pub(crate) fn output_json(output: ToolOutput) -> Value {
    serde_json::from_str(&output.content).unwrap()
}

#[cfg(test)]
pub(crate) mod test_support {
    //! 论文工具测试共享辅助：临时项目构建与最小 [`ToolContext`]。

    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use referee_ai::tool::ToolContext;

    /// 创建临时目录（进程 + 线程 + 时间戳隔离，避免并行冲突）。
    pub fn temp_project_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluen_paper_{tag}_{}_{:?}_{}",
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

    /// 构造最小可用的 [`ToolContext`]。
    pub fn ctx() -> ToolContext {
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

    /// 创建带两个章节的测试项目（引言含二级/三级子节；方法为独立一级章节）。
    pub fn build_test_project(tag: &str) -> (PathBuf, PathBuf) {
        use crate::project::creator;
        use crate::project::model::{CreateProjectRequest, sanitize_project_name};

        let storage = temp_project_dir(tag);
        let request = CreateProjectRequest {
            title: "测试文章".into(),
            author: "张三".into(),
            project_name: sanitize_project_name("测试文章"),
            storage_path: storage.to_string_lossy().to_string(),
            description: None,
        };
        let project_dir = PathBuf::from(creator::create_project(request).unwrap());

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

    /// 向项目中添加一个章节（写入 sec-*.md 并更新 sections.json，
    /// 随后删除 main.md 以模拟旧版项目，验证迁移拼装路径）。
    fn add_section(project_dir: &std::path::Path, id: &str, order: u32, title: &str, body: &str) {
        let sections_dir = project_dir.join("manuscript").join("sections");
        let content = format!(
            "---\ntitle: \"{title}\"\ntitle_html: \"\"\ncreated: 2026-01-01T00:00:00Z\nupdated: 2026-01-01T00:00:00Z\n---\n{body}"
        );
        fs::write(sections_dir.join(format!("{id}.md")), content).unwrap();

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

        // 移除 main.md，使下次 open_project / ensure_main_md 触发迁移拼装
        let _ = fs::remove_file(project_dir.join("manuscript").join("main.md"));
    }
}

// ---------------------------------------------------------------------------
// 单元测试：PaperReader 本体
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn read_full_returns_assembled_paper() {
        let (storage, project_dir) =
            test_support::build_test_project("reader_full");
        let reader = PaperReader::new(project_dir.to_string_lossy().to_string());

        let md = reader.read_full().await.unwrap();
        assert!(md.contains("# 引言"));
        assert!(md.contains("### 国内现状"));
        assert!(md.contains("# 方法"));

        let _ = fs::remove_dir_all(&storage);
    }

    #[tokio::test]
    async fn read_full_migrates_legacy_project_without_main_md() {
        let (storage, project_dir) =
            test_support::build_test_project("reader_migrate");
        let main_md = project_dir.join("manuscript").join("main.md");
        assert!(!main_md.exists(), "前置：main.md 已被移除以模拟旧版项目");

        let reader = PaperReader::new(project_dir.to_string_lossy().to_string());
        let md = reader.read_full().await.unwrap();
        assert!(md.contains("# 方法"));
        assert!(main_md.is_file(), "迁移后 main.md 应已落盘");

        let _ = fs::remove_dir_all(&storage);
    }

    #[tokio::test]
    async fn read_full_rejects_missing_project() {
        let ghost = test_support::temp_project_dir("reader_ghost")
            .join("ghost")
            .display()
            .to_string();
        let reader = PaperReader::new(ghost);
        let err = reader.read_full().await.unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));
    }
}
