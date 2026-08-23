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
use super::project::write::ProjectWriteTool;

/// 注册论文读取工具：`paper_outline` + `paper_section`（只读直装）。
pub fn register_paper_readers(registry: &ToolRegistry, project_path: &str) -> Result<(), RegistryError> {
    registry.register(Arc::new(PaperOutlineTool::new(project_path.to_string())))?;
    registry.register(Arc::new(PaperSectionTool::new(project_path.to_string())))
}

/// 注册论文正文写入工具 `manuscript`（ApprovalGuard 包装；正文唯一写入通道）。
pub fn register_manuscript(
    registry: &ToolRegistry,
    project_path: &str,
    approver: Arc<dyn Approver>,
) -> Result<(), RegistryError> {
    registry.register(Arc::new(ApprovalGuard::new(
        Arc::new(ManuscriptEditTool::new(project_path.to_string())),
        approver,
    )))
}

/// 注册项目文件三件套：`project_read` 只读直装；`project_write` /
/// `project_edit` 经 referee 原语并由 ApprovalGuard 包装（正文 main.md 写保护）。
pub fn register_project_files(
    registry: &ToolRegistry,
    project_path: &str,
    approver: Arc<dyn Approver>,
) -> Result<(), RegistryError> {
    registry.register(Arc::new(ProjectReadTool::new(project_path.to_string())))?;
    registry.register(Arc::new(ApprovalGuard::new(
        Arc::new(ProjectWriteTool::new(project_path.to_string())),
        approver.clone(),
    )))?;
    registry.register(Arc::new(ApprovalGuard::new(
        Arc::new(ProjectEditTool::new(project_path.to_string())),
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

        register_paper_readers(&registry, dir.to_str().unwrap()).unwrap();
        register_manuscript(&registry, dir.to_str().unwrap(), approver.clone()).unwrap();
        register_project_files(&registry, dir.to_str().unwrap(), approver).unwrap();
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
    fn register_literature_search_when_kb_exists() {
        let dir = temp_dir("with_kb");
        AsyncKnowledgeBase::init(&dir.join("references")).unwrap();

        let registry = ToolRegistry::with_defaults();
        register_literature_search(&registry, dir.to_str().unwrap(), &LlmConfig::default()).unwrap();
        assert!(registry.get("literature_search").is_some());

        let _ = fs::remove_dir_all(&dir);
    }
}
