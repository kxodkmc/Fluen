//! 运行时装配辅助——各智能体运行时（学术助手 / Motis 总督 / 子代理）共享的
//! 工具注册组合。
//!
//! 只做三件事：实例化工具、按需 [`ApprovalGuard`] 包装、注册进注册表；
//! 工具本身的职责与安全语义见各自模块文档。运行时侧只需按角色挑选组合，
//! 并将 [`RegistryError`] 映射为各自的配置错误类型。

use std::sync::Arc;

use referee_ai::tool::{RegistryError, ToolRegistry};

use crate::agent_runtime::approval::{ApprovalGuard, Approver};
use crate::llm_config::model::LlmConfig;

use super::literature::{self, LiteratureSearchTool};
use super::manuscript::ManuscriptEditTool;
use super::paper::outline::PaperOutlineTool;
use super::paper::section::PaperSectionTool;
use super::project::edit::ProjectEditTool;
use super::project::read::ProjectReadTool;
use super::project::read_gate::ReadGateGuard;
use super::project::read_state::ReadTracker;
use super::project::write::ProjectWriteTool;

/// 注册论文读取工具：`paper_outline` + `paper_section`（只读直装）。
pub fn register_paper_readers(registry: &ToolRegistry, project_path: &str) -> Result<(), RegistryError> {
    registry.register(Arc::new(PaperOutlineTool::new(project_path.to_string())))?;
    registry.register(Arc::new(PaperSectionTool::new(project_path.to_string())))
}

/// 注册论文正文写入工具 `manuscript`（ApprovalGuard 包装；正文唯一写入通道）。
///
/// 内层套 [`ReadGateGuard`]：更新已有正文前必须已完整读取
/// `manuscript/main.md`（先于审批弹窗拦截）。
pub fn register_manuscript(
    registry: &ToolRegistry,
    project_path: &str,
    approver: Arc<dyn Approver>,
    tracker: Arc<ReadTracker>,
) -> Result<(), RegistryError> {
    registry.register(Arc::new(ApprovalGuard::new(
        Arc::new(ReadGateGuard::new(
            Arc::new(ManuscriptEditTool::new(project_path.to_string())),
            project_path.to_string(),
            tracker,
        )),
        approver,
    )))
}

/// 注册只读项目文件读取工具 `project_read`（讨论/审核类角色的只读取用）。
///
/// 读取成功后向 `tracker` 记账，供写前必读门判定「完整读取」。
pub fn register_project_read(
    registry: &ToolRegistry,
    project_path: &str,
    tracker: Arc<ReadTracker>,
) -> Result<(), RegistryError> {
    registry.register(Arc::new(ProjectReadTool::with_tracker(
        project_path.to_string(),
        Some(tracker),
    )))
}

/// 注册项目文件三件套：`project_read` 只读直装（带读取记账）；
/// `project_write` / `project_edit` 经写前必读门（[`ReadGateGuard`]）
/// 与 ApprovalGuard 双层包装（正文 main.md 写保护）。
pub fn register_project_files(
    registry: &ToolRegistry,
    project_path: &str,
    approver: Arc<dyn Approver>,
    tracker: Arc<ReadTracker>,
) -> Result<(), RegistryError> {
    register_project_read(registry, project_path, tracker.clone())?;
    registry.register(Arc::new(ApprovalGuard::new(
        Arc::new(ReadGateGuard::new(
            Arc::new(ProjectWriteTool::new(project_path.to_string())),
            project_path.to_string(),
            tracker.clone(),
        )),
        approver.clone(),
    )))?;
    registry.register(Arc::new(ApprovalGuard::new(
        Arc::new(ReadGateGuard::new(
            Arc::new(ProjectEditTool::new(project_path.to_string())),
            project_path.to_string(),
            tracker,
        )),
        approver,
    )))
}

/// 知识库存在时注册 `literature_search`；缺失或打开失败时告警降级跳过
/// （详见 [`literature::open_kb`]）。
pub fn register_literature_search(
    registry: &ToolRegistry,
    project_path: &str,
    llm: &LlmConfig,
) -> Result<(), RegistryError> {
    let Some(kb) = literature::open_kb(project_path, llm) else {
        return Ok(());
    };
    registry.register(Arc::new(LiteratureSearchTool::new(kb)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::approval::Approver;
    use fluen_knowledge::async_kb::AsyncKnowledgeBase;
    use std::fs;
    use std::path::PathBuf;

    struct NoopApprover;

    #[async_trait::async_trait]
    impl Approver for NoopApprover {
        async fn approve(&self, _tool_name: &str, _input: &serde_json::Value) -> Result<(), referee_ai::tool::ToolError> {
            Ok(())
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluen_assemble_{tag}_{}_{:?}_{}",
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

    #[test]
    fn register_all_helpers_without_kb() {
        let dir = temp_dir("no_kb");
        let registry = ToolRegistry::with_defaults();
        let approver: Arc<dyn Approver> = Arc::new(NoopApprover);
        let tracker = ReadTracker::new_arc();

        register_paper_readers(&registry, dir.to_str().unwrap()).unwrap();
        register_manuscript(&registry, dir.to_str().unwrap(), approver.clone(), tracker.clone()).unwrap();
        register_project_files(&registry, dir.to_str().unwrap(), approver, tracker).unwrap();
        // 知识库缺失：静默降级
        register_literature_search(&registry, dir.to_str().unwrap(), &LlmConfig::default()).unwrap();

        for name in [
            "paper_outline",
            "paper_section",
            "manuscript",
            "project_read",
            "project_write",
            "project_edit",
        ] {
            assert!(registry.get(name).is_some(), "缺少工具 {name}");
        }
        assert!(registry.get("literature_search").is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn register_project_read_only_excludes_write_tools() {
        let dir = temp_dir("readonly");
        let registry = ToolRegistry::with_defaults();
        register_project_read(&registry, dir.to_str().unwrap(), ReadTracker::new_arc()).unwrap();

        assert!(registry.get("project_read").is_some());
        assert!(registry.get("project_write").is_none());
        assert!(registry.get("project_edit").is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn register_literature_search_when_kb_exists() {
        let dir = temp_dir("with_kb");
        AsyncKnowledgeBase::init(&dir.join("references")).unwrap();

        let registry = ToolRegistry::with_defaults();
        register_literature_search(&registry, dir.to_str().unwrap(), &LlmConfig::default()).unwrap();
        assert!(registry.get("literature_search").is_some());

        let _ = fs::remove_dir_all(&dir);
    }
}
