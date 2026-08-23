//! # 子智能体注册表
//!
//! 定义 Motis（总督角色）可调度的子智能体清单与构建逻辑。
//!
//! Motis 自身不负责编写、计算等具体任务，而是理解用户意图后
//! 派发给合适的子智能体执行，最后汇总结果。
//!
//! ## 子智能体清单
//!
//! | ID | 名称 | 职责 | 工具集 |
//! |----|------|------|--------|
//! | `academic_writer` | 学术撰写助手 | 撰写格式规范的论文正文 | paper_outline / paper_section / literature_search / manuscript / project_read / project_write / project_edit |
//! | `knowledge_builder` | 知识库构建助手 | 构建与查询文献知识库 | literature_search / project_read / project_write / project_edit |
//! | `data_analyst` | 数据分析助手 | 统计分析与数据可视化 | project_read / project_write / project_edit（数据分析工具为独立 Tauri 命令通道，未注册为智能体工具） |
//!
//! ## 扩展
//!
//! 新增子智能体只需在 [`all_agent_defs`] 中追加一条 [`AgentDef`]，
//! 并实现对应的 `build_runtime` 闭包即可，无需修改 Motis 核心逻辑。

use std::sync::Arc;

use referee_ai::tool::ToolRegistry;

use crate::agent_runtime::approval::Approver;
use crate::agent_runtime::observability::{observe_registry, ToolEventSink};
use crate::agent_runtime::{FluenRuntime, FluenRuntimeBuilder};
use crate::agent_tools;
use crate::llm_chat;
use crate::llm_config::model::LlmConfig;

use super::timeouts::{motis_engine_config, LLM_HTTP_TIMEOUT, SUBAGENT_TOOL_TIMEOUT};

/// 子智能体 ID（固定字符串，前端配置与后端注册一致）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentId {
    /// 学术撰写助手——撰写格式规范的论文正文。
    AcademicWriter,
    /// 知识库构建助手——构建与查询文献知识库。
    KnowledgeBuilder,
    /// 数据分析助手——统计分析与数据可视化。
    DataAnalyst,
}

impl AgentId {
    /// 返回用于工具名与配置文件的字符串 ID。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AcademicWriter => "academic_writer",
            Self::KnowledgeBuilder => "knowledge_builder",
            Self::DataAnalyst => "data_analyst",
        }
    }

    /// 从字符串解析（配置文件中存储为字符串）。
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "academic_writer" => Some(Self::AcademicWriter),
            "knowledge_builder" => Some(Self::KnowledgeBuilder),
            "data_analyst" => Some(Self::DataAnalyst),
            _ => None,
        }
    }

    /// 返回全部已知子智能体 ID。
    pub fn all() -> &'static [AgentId] {
        &[
            Self::AcademicWriter,
            Self::KnowledgeBuilder,
            Self::DataAnalyst,
        ]
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 子智能体定义（静态元数据 + 构建函数）。
pub struct AgentDef {
    /// 智能体 ID。
    pub id: AgentId,
    /// 人类可读名称（前端展示）。
    pub name: &'static str,
    /// 职责描述（前端展示 + Motis 提示词注入）。
    pub description: &'static str,
    /// 系统提示词构建函数。
    pub build_prompt: fn() -> String,
    /// 工具注册函数（在给定项目路径 / LLM 配置 / 审批器时构建工具集）。
    pub build_tools: fn(
        project_path: &str,
        llm: &LlmConfig,
        approver: Arc<dyn Approver>,
    ) -> Result<ToolRegistry, AgentBuildError>,
}

/// 子智能体运行时构建错误。
#[derive(Debug, thiserror::Error)]
pub enum AgentBuildError {
    /// LLM 配置错误。
    #[error("LLM 配置错误: {0}")]
    LlmConfig(String),
    /// 工具注册失败。
    #[error("工具注册失败: {0}")]
    ToolRegistry(String),
    /// 联邦内核装配失败（扩展注册等）。
    #[error("联邦装配失败: {0}")]
    Kernel(String),
}

/// 返回全部子智能体定义。
pub fn all_agent_defs() -> &'static [AgentDef] {
    &[
        AgentDef {
            id: AgentId::AcademicWriter,
            name: "学术撰写助手",
            description: "撰写格式规范的论文正文（遵循 fluen-markup 规范），可读取论文内容、检索文献、写入正文。",
            build_prompt: build_academic_writer_prompt,
            build_tools: build_academic_writer_tools,
        },
        AgentDef {
            id: AgentId::KnowledgeBuilder,
            name: "知识库构建助手",
            description: "构建与查询文献知识库，检索文献综述 / 概念 / 实体条目。",
            build_prompt: build_knowledge_builder_prompt,
            build_tools: build_knowledge_builder_tools,
        },
        AgentDef {
            id: AgentId::DataAnalyst,
            name: "数据分析助手",
            description: "执行统计分析与数据可视化（描述性统计、假设检验、回归分析等）。",
            build_prompt: build_data_analyst_prompt,
            build_tools: build_data_analyst_tools,
        },
    ]
}

/// 按 ID 查找子智能体定义。
pub fn find_agent_def(id: &AgentId) -> Option<&'static AgentDef> {
    all_agent_defs().iter().find(|def| def.id == *id)
}

/// 返回已启用的子智能体描述文本（供 Motis 提示词注入）。
///
/// `enabled_agents` 为空列表时全部可用；非空时仅包含列表中的 ID。
pub fn enabled_agents_description(enabled_agents: &[String]) -> String {
    all_agent_defs()
        .iter()
        .filter(|def| {
            enabled_agents.is_empty()
                || enabled_agents.contains(&def.id.as_str().to_string())
        })
        .map(|def| format!("- `{}`: {}", def.id.as_str(), def.description))
        .collect::<Vec<_>>()
        .join("\n")
}

// ===========================================================================
// 系统提示词构建
// ===========================================================================

fn build_academic_writer_prompt() -> String {
    crate::ai_assistant::prompt::build_system_prompt()
}

fn build_knowledge_builder_prompt() -> String {
    "你是 Fluen 学术创作平台的**知识库构建助手**。你的核心职责是帮助用户查询和管理文献知识库。\n\n\
     你可以：\n\
     - 使用 literature_search 工具检索文献知识库（混合检索：关键词 + 语义向量，最多返回 4 条相关条目）\n\
     - 使用 project_read 读取项目内文件，project_write / project_edit 写入或编辑（如参考文献索引等；写操作需确认）\n\n\
     工作原则：\n\
     - 检索前先理解用户需求，选择合适的查询词和条目类型筛选\n\
     - 结果需结合上下文校验，工具可能返回过时信息\n\
     - 读写操作需用户确认后才执行\n"
        .to_string()
}

fn build_data_analyst_prompt() -> String {
    "你是 Fluen 学术创作平台的**数据分析助手**。你的核心职责是帮助用户进行统计分析与数据可视化。\n\n\
     你可以：\n\
     - 读取项目内的 CSV / 数据文件\n\
     - 基于数据提供分析建议\n\
     - 将分析结果写入项目文件\n\n\
     工作原则：\n\
     - 分析前先理解数据结构与用户的研究问题\n\
     - 提供可解释的统计结论，不替代用户做学术判断\n\
     - 写操作需用户确认后才执行\n"
        .to_string()
}

// ===========================================================================
// 工具注册函数
// ===========================================================================

/// 学术撰写助手工具集：paper_outline / paper_section / literature_search /
/// manuscript / project_read / project_write / project_edit。
fn build_academic_writer_tools(
    project_path: &str,
    llm: &LlmConfig,
    approver: Arc<dyn Approver>,
) -> Result<ToolRegistry, AgentBuildError> {
    let registry = ToolRegistry::with_defaults();
    let reg_err = |e: referee_ai::tool::RegistryError| {
        AgentBuildError::ToolRegistry(e.to_string())
    };

    // 只读：论文大纲与章节读取 + 文献知识库搜索
    agent_tools::assemble::register_paper_readers(&registry, project_path).map_err(reg_err)?;
    agent_tools::assemble::register_literature_search(&registry, project_path, llm)
        .map_err(reg_err)?;

    // 写操作：论文正文写入（ApprovalGuard 包装）
    agent_tools::assemble::register_manuscript(&registry, project_path, approver.clone())
        .map_err(reg_err)?;

    // 读写：项目内文件三件套（写/编辑需审批；正文 main.md 写保护）
    agent_tools::assemble::register_project_files(&registry, project_path, approver)
        .map_err(reg_err)?;

    Ok(registry)
}

/// 知识库构建助手工具集：literature_search + project_read / project_write /
/// project_edit。
fn build_knowledge_builder_tools(
    project_path: &str,
    llm: &LlmConfig,
    approver: Arc<dyn Approver>,
) -> Result<ToolRegistry, AgentBuildError> {
    let registry = ToolRegistry::with_defaults();
    let reg_err = |e: referee_ai::tool::RegistryError| {
        AgentBuildError::ToolRegistry(e.to_string())
    };

    // 只读：文献知识库搜索
    agent_tools::assemble::register_literature_search(&registry, project_path, llm)
        .map_err(reg_err)?;

    // 读写：项目内文件三件套（写/编辑需审批；正文 main.md 写保护）
    agent_tools::assemble::register_project_files(&registry, project_path, approver)
        .map_err(reg_err)?;

    Ok(registry)
}

/// 数据分析助手工具集：project_read / project_write / project_edit（数据文件）。
fn build_data_analyst_tools(
    project_path: &str,
    _llm: &LlmConfig,
    approver: Arc<dyn Approver>,
) -> Result<ToolRegistry, AgentBuildError> {
    let registry = ToolRegistry::with_defaults();

    // 读写：项目内数据文件——只读直装；写/编辑经 referee 原语并 ApprovalGuard 包装
    agent_tools::assemble::register_project_files(&registry, project_path, approver)
        .map_err(|e| AgentBuildError::ToolRegistry(e.to_string()))?;

    Ok(registry)
}

// ===========================================================================
// 子智能体运行时构建
// ===========================================================================

/// 构建指定子智能体的运行时。
///
/// 使用 LLM 全局激活项（子智能体不覆盖 provider/model），
/// 装配该智能体专属的工具集与系统提示词；`sink` 非空时
/// 工具集整体经 [`observe_registry`] 包装，向调用方上报
/// 工具调用的开始/结束（子智能体运行可观测性）。
pub fn build_agent_runtime(
    agent_id: &AgentId,
    llm: &LlmConfig,
    project_path: &str,
    approver: Arc<dyn Approver>,
    sink: Option<Arc<dyn ToolEventSink>>,
) -> Result<(bool, FluenRuntime), AgentBuildError> {
    let def = find_agent_def(agent_id).ok_or_else(|| {
        AgentBuildError::LlmConfig(format!("未知子智能体: {}", agent_id))
    })?;

    // 解析 provider 与 model（子智能体使用全局激活项）
    let (provider, model_id) =
        llm_chat::resolve_provider_model(None, None, llm)
            .map_err(|e| AgentBuildError::LlmConfig(e.to_string()))?;

    let thinking_enabled = provider.model_supports_thinking(&model_id);

    // HTTP 兜底超时大于引擎单轮超时（见 timeouts 分层）
    let llm_provider = llm_chat::build_llm_provider_with_timeout(
        provider,
        &model_id,
        LLM_HTTP_TIMEOUT,
    )
    .map_err(|e| AgentBuildError::LlmConfig(e.to_string()))?;

    // 构建工具集（可选整体观测包装）
    let registry = (def.build_tools)(project_path, llm, approver)?;
    let registry = match sink {
        Some(sink) => observe_registry(&registry, sink),
        None => registry,
    };

    // 引擎单轮 LLM 超时放宽至 5 分钟（学术写作长生成），见 timeouts 分层
    let runtime = FluenRuntimeBuilder::new(llm_provider)
        .with_config(motis_engine_config())
        .with_tools(registry, crate::agent_runtime::approval::approval_executor(SUBAGENT_TOOL_TIMEOUT))
        .build();

    Ok((thinking_enabled, runtime))
}

/// 返回子智能体的系统提示词。
pub fn agent_system_prompt(agent_id: &AgentId) -> Option<String> {
    find_agent_def(agent_id).map(|def| (def.build_prompt)())
}

// ===========================================================================
// 测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_id_roundtrip() {
        for id in AgentId::all() {
            let s = id.as_str();
            assert_eq!(AgentId::from_str(s), Some(id.clone()));
        }
    }

    #[test]
    fn unknown_id_returns_none() {
        assert!(AgentId::from_str("unknown").is_none());
    }

    #[test]
    fn all_defs_cover_all_ids() {
        let defs = all_agent_defs();
        assert_eq!(defs.len(), AgentId::all().len());
        for id in AgentId::all() {
            assert!(defs.iter().any(|def| def.id == id.clone()), "missing def for {}", id);
        }
    }

    #[test]
    fn find_def_returns_correct_agent() {
        let def = find_agent_def(&AgentId::AcademicWriter).unwrap();
        assert_eq!(def.id, AgentId::AcademicWriter);
        assert!(!def.name.is_empty());
        assert!(!def.description.is_empty());
    }

    #[test]
    fn prompts_are_nonempty() {
        assert!(!build_academic_writer_prompt().is_empty());
        assert!(!build_knowledge_builder_prompt().is_empty());
        assert!(!build_data_analyst_prompt().is_empty());
    }
}
