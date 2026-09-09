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
//! | `essay_writing` | 论文撰写助手 | 撰写人类式、可通过检验的学术正文（LVRV1 写作规范） | paper_outline / paper_section / literature_search / manuscript / project_read / project_write / project_edit |
//! | `essay_review` | 论文审核助手 | 论文审核（功能开发中，暂不可用） | paper_outline / paper_section / literature_search / project_read |
//! | `essay_critique` | 论文思辨助手 | 引导论文思辨讨论，客观基于证据，不负责成文 | paper_outline / paper_section / literature_search / project_read |
//! | `knowledge_builder` | 知识库构建助手 | 构建与查询文献知识库 | literature_search / project_read / project_write / project_edit |
//! | `data_analyst` | 数据分析助手 | 统计分析与数据可视化 | project_read / project_write / project_edit / socstat MCP 统计工具族（经 `crate::mcp_host` 托管） |
//!
//! ## 扩展
//!
//! 新增子智能体只需在 [`all_agent_defs`] 中追加一条 [`AgentDef`]，
//! 并实现对应的 `build_runtime` 闭包即可，无需修改 Motis 核心逻辑。

use std::sync::Arc;

use referee_ai::tool::{Tool, ToolRegistry};

use crate::agent_runtime::approval::Approver;
use crate::agent_runtime::observability::observe_registry;
use crate::agent_runtime::{FluenRuntime, FluenRuntimeBuilder};
use crate::agent_tools;
use crate::agent_tools::project::read_state::ReadTracker;
use crate::llm_chat;
use crate::llm_config::model::LlmConfig;

use super::agent_reporter::AgentReporter;
use super::timeouts::{
    motis_engine_config, LLM_HTTP_TIMEOUT, SUBAGENT_AWAITING_TIMEOUT, SUBAGENT_TOOL_TIMEOUT,
};

/// 子智能体 ID（固定字符串，前端配置与后端注册一致）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentId {
    /// 论文撰写助手——撰写人类式、可通过检验的学术正文。
    EssayWriting,
    /// 论文审核助手——审核论文（功能开发中，占位）。
    EssayReview,
    /// 论文思辨助手——客观引导论文讨论，不负责成文。
    EssayCritique,
    /// 知识库构建助手——构建与查询文献知识库。
    KnowledgeBuilder,
    /// 数据分析助手——统计分析与数据可视化。
    DataAnalyst,
}

impl AgentId {
    /// 返回用于工具名与配置文件的字符串 ID。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EssayWriting => "essay_writing",
            Self::EssayReview => "essay_review",
            Self::EssayCritique => "essay_critique",
            Self::KnowledgeBuilder => "knowledge_builder",
            Self::DataAnalyst => "data_analyst",
        }
    }

    /// 从字符串解析（配置文件中存储为字符串）。
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "essay_writing" => Some(Self::EssayWriting),
            "essay_review" => Some(Self::EssayReview),
            "essay_critique" => Some(Self::EssayCritique),
            "knowledge_builder" => Some(Self::KnowledgeBuilder),
            "data_analyst" => Some(Self::DataAnalyst),
            _ => None,
        }
    }

    /// 返回全部已知子智能体 ID。
    pub fn all() -> &'static [AgentId] {
        &[
            Self::EssayWriting,
            Self::EssayReview,
            Self::EssayCritique,
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
    /// 工具注册函数（在给定项目路径 / LLM 配置 / 审批器 / 读取跟踪器时构建工具集）。
    pub build_tools: fn(
        project_path: &str,
        llm: &LlmConfig,
        approver: Arc<dyn Approver>,
        read_tracker: Arc<ReadTracker>,
    ) -> Result<ToolRegistry, AgentBuildError>,
    /// 工具预设：是否追加 socstat MCP 统计工具（数据分析通道，工具由
    /// 应用启动时待机的 `crate::mcp_host` 服务器提供）。
    pub uses_socstat_mcp: bool,
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
            id: AgentId::EssayWriting,
            name: "论文撰写助手",
            description: "撰写、编辑、修改论文正文的**专属**智能体（遵循 LVRV1 人类式写作规范与 fluen-markup）：写新章节、局部修改、增删段落、调整结构都由它执行——正文唯一写入通道（manuscript 工具）装配在它身上。",
            build_prompt: build_essay_writing_prompt,
            build_tools: build_essay_writing_tools,
            uses_socstat_mcp: false,
        },
        AgentDef {
            id: AgentId::EssayReview,
            name: "论文审核助手",
            description: "审核论文（功能开发中，暂不可用——收到审核任务时如实告知并引导至撰写/思辨助手）。",
            build_prompt: build_essay_review_prompt,
            build_tools: build_essay_review_tools,
            uses_socstat_mcp: false,
        },
        AgentDef {
            id: AgentId::EssayCritique,
            name: "论文思辨助手",
            description: "客观、基于现实证据地引导论文思辨讨论（梳理论点/结构/争议），不负责成文。",
            build_prompt: build_essay_critique_prompt,
            build_tools: build_essay_critique_tools,
            uses_socstat_mcp: false,
        },
        AgentDef {
            id: AgentId::KnowledgeBuilder,
            name: "知识库构建助手",
            description: "构建与查询文献知识库，检索文献综述 / 概念 / 实体条目。",
            build_prompt: build_knowledge_builder_prompt,
            build_tools: build_knowledge_builder_tools,
            uses_socstat_mcp: false,
        },
        AgentDef {
            id: AgentId::DataAnalyst,
            name: "数据分析助手",
            description: "执行统计分析与数据可视化（描述性统计、假设检验、回归分析等）。",
            build_prompt: build_data_analyst_prompt,
            build_tools: build_data_analyst_tools,
            uses_socstat_mcp: true,
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

fn build_essay_writing_prompt() -> String {
    crate::agent_prompts::writing::system()
}

fn build_essay_review_prompt() -> String {
    crate::agent_prompts::review::system()
}

fn build_essay_critique_prompt() -> String {
    crate::agent_prompts::critique::system()
}

fn build_knowledge_builder_prompt() -> String {
    "你是 Fluen 学术创作平台的**知识库构建助手**。你的核心职责是帮助用户查询和管理文献知识库。\n\n\
     你可以：\n\
     - 使用 literature_search 工具检索文献知识库（混合检索：关键词 + 语义向量，最多返回 4 条相关条目）\n\
     - 使用 project_read 读取项目内文件，project_write / project_edit 写入或编辑**非正文**文件（如参考文献索引等；写操作需确认）\n\n\
     工作原则：\n\
     - 检索前先理解用户需求，选择合适的查询词和条目类型筛选\n\
     - 结果需结合上下文校验，工具可能返回过时信息\n\
     - 读写操作需用户确认后才执行\n\
     - **不负责论文正文的撰写或编辑**——即使收到相关请求，也说明这超出职责范围，应由用户向 Motis 提出撰写/编辑需求（由论文撰写助手处理）\n"
        .to_string()
}

fn build_data_analyst_prompt() -> String {
    "你是 Fluen 学术创作平台的**数据分析助手**。你的核心职责是基于项目数据执行统计分析并解读结果。\n\n\
     分析通道——socstat 统计服务器（MCP 工具，应用启动时已就绪）：\n\
     - 先用 load_dataset 加载项目 data/ 目录下的数据文件（CSV / JSON / .sav），后续按数据集名引用；可用 list_datasets / preview 了解数据结构\n\
     - 数据整理（仅影响会话内数据，不改源文件）：recode / filter / sort / keep / compute / set_weight\n\
     - 按研究问题选择方法：描述与频数（descriptive / frequencies / crosstab）；组间与配对比较（independent_t_test / paired_t_test / one_way_anova / mann_whitney_u_test / wilcoxon_signed_rank_test / kruskal_wallis_test）；分类关联（chi_square_test / fisher_exact_test）；正态性（shapiro_wilk / ks_normality_test）；相关与回归（correlation_pair / correlation_matrix / partial_correlation / linear_regression / logistic_regression / vif）；多变量与信度（pca / reliability）；事后与析因（post_hoc / factorial_anova）\n\n\
     工作原则：\n\
     - 方法选择需符合数据类型与研究问题，注明统计前提与适用条件\n\
     - 结论给出统计量、p 值与效应解读；前提不满足或结果不确定时如实说明\n\
     - 不替代用户做学术判断；如需将结果写入项目文件，用 project_write / project_edit 写入**非正文**文件（需用户确认）\n\
     - **不负责论文正文的撰写或编辑**——正文相关请求说明超出职责范围，应由用户向 Motis 提出撰写/编辑需求（由论文撰写助手处理）\n"
        .to_string()
}

// ===========================================================================
// 工具注册函数
// ===========================================================================

/// 论文撰写助手工具集：paper_outline / paper_section / literature_search /
/// manuscript / project_read / project_write / project_edit。
fn build_essay_writing_tools(
    project_path: &str,
    llm: &LlmConfig,
    approver: Arc<dyn Approver>,
    read_tracker: Arc<ReadTracker>,
) -> Result<ToolRegistry, AgentBuildError> {
    let registry = ToolRegistry::with_defaults();
    let reg_err = |e: referee_ai::tool::RegistryError| {
        AgentBuildError::ToolRegistry(e.to_string())
    };

    // 只读：论文大纲与章节读取 + 文献知识库搜索
    agent_tools::assemble::register_paper_readers(
        &registry,
        project_path,
        read_tracker.clone(),
    )
    .map_err(reg_err)?;
    agent_tools::assemble::register_literature_search(&registry, project_path, llm)
        .map_err(reg_err)?;

    // 写操作：论文正文写入（写前必读门 + ApprovalGuard 包装）
    agent_tools::assemble::register_manuscript(&registry, project_path, approver.clone(), read_tracker.clone())
        .map_err(reg_err)?;

    // 读写：项目内文件三件套（写/编辑经读门与审批；正文 main.md 写保护）
    agent_tools::assemble::register_project_files(&registry, project_path, approver, read_tracker)
        .map_err(reg_err)?;

    Ok(registry)
}

/// 论文审核助手工具集（占位）：仅只读——paper_outline / paper_section /
/// literature_search / project_read，不装配任何写工具。
fn build_essay_review_tools(
    project_path: &str,
    llm: &LlmConfig,
    approver: Arc<dyn Approver>,
    read_tracker: Arc<ReadTracker>,
) -> Result<ToolRegistry, AgentBuildError> {
    build_readonly_essay_tools(project_path, llm, approver, read_tracker)
}

/// 论文思辨助手工具集（只读）：paper_outline / paper_section /
/// literature_search / project_read——不装配任何写工具，恪守"不负责写作"。
fn build_essay_critique_tools(
    project_path: &str,
    llm: &LlmConfig,
    approver: Arc<dyn Approver>,
    read_tracker: Arc<ReadTracker>,
) -> Result<ToolRegistry, AgentBuildError> {
    build_readonly_essay_tools(project_path, llm, approver, read_tracker)
}

/// 只读不写工具集公共装配：论文读取 + 文献检索 + 项目只读。
/// 供审核/思辨等不落盘角色复用，杜绝写工具误配。
fn build_readonly_essay_tools(
    project_path: &str,
    llm: &LlmConfig,
    _approver: Arc<dyn Approver>,
    read_tracker: Arc<ReadTracker>,
) -> Result<ToolRegistry, AgentBuildError> {
    let registry = ToolRegistry::with_defaults();
    let reg_err = |e: referee_ai::tool::RegistryError| {
        AgentBuildError::ToolRegistry(e.to_string())
    };

    agent_tools::assemble::register_paper_readers(
        &registry,
        project_path,
        read_tracker.clone(),
    )
    .map_err(reg_err)?;
    agent_tools::assemble::register_literature_search(&registry, project_path, llm)
        .map_err(reg_err)?;
    agent_tools::assemble::register_project_read(&registry, project_path, read_tracker)
        .map_err(reg_err)?;

    Ok(registry)
}

/// 知识库构建助手工具集：literature_search + project_read / project_write /
/// project_edit。
fn build_knowledge_builder_tools(
    project_path: &str,
    llm: &LlmConfig,
    approver: Arc<dyn Approver>,
    read_tracker: Arc<ReadTracker>,
) -> Result<ToolRegistry, AgentBuildError> {
    let registry = ToolRegistry::with_defaults();
    let reg_err = |e: referee_ai::tool::RegistryError| {
        AgentBuildError::ToolRegistry(e.to_string())
    };

    // 只读：文献知识库搜索
    agent_tools::assemble::register_literature_search(&registry, project_path, llm)
        .map_err(reg_err)?;

    // 读写：项目内文件三件套（写/编辑经读门与审批；正文 main.md 写保护）
    agent_tools::assemble::register_project_files(&registry, project_path, approver, read_tracker)
        .map_err(reg_err)?;

    Ok(registry)
}

/// 数据分析助手工具集：project_read / project_write / project_edit（数据文件）。
/// socstat 统计工具族由 [`build_agent_runtime`] 按 `uses_socstat_mcp`
/// 预设追加——统计计算经应用启动时待机的 MCP 服务器（`crate::mcp_host`）。
fn build_data_analyst_tools(
    project_path: &str,
    _llm: &LlmConfig,
    approver: Arc<dyn Approver>,
    read_tracker: Arc<ReadTracker>,
) -> Result<ToolRegistry, AgentBuildError> {
    let registry = ToolRegistry::with_defaults();

    // 读写：项目内数据文件——只读直装；写/编辑经读门 + referee 原语并 ApprovalGuard 包装
    agent_tools::assemble::register_project_files(&registry, project_path, approver, read_tracker)
        .map_err(|e| AgentBuildError::ToolRegistry(e.to_string()))?;

    Ok(registry)
}

// ===========================================================================
// 子智能体运行时构建
// ===========================================================================

/// 构建指定子智能体的运行时。
///
/// 使用 LLM 全局激活项（子智能体不覆盖 provider/model），
/// 装配该智能体专属的工具集与系统提示词；`reporter` 非空时：
/// - 工具集整体经 [`observe_registry`] 包装，上报工具调用的开始/结束；
/// - 引擎注入 [`EngineObserver`](referee_ai::EngineObserver)（即 reporter 本身），
///   透传子智能体 LLM 思考/文本增量，并兜底上报执行器折叠的工具失败。
///
/// `socstat_tools` 为 socstat MCP 统计工具快照（由 `crate::mcp_host`
/// 待机服务器提供；连接失败为空列表，即该通道降级）；仅
/// `uses_socstat_mcp` 预设为真的智能体（`data_analyst`）会注册它们。
#[allow(clippy::too_many_arguments)]
pub fn build_agent_runtime(
    agent_id: &AgentId,
    llm: &LlmConfig,
    project_path: &str,
    approver: Arc<dyn Approver>,
    read_tracker: Arc<ReadTracker>,
    reporter: Option<Arc<AgentReporter>>,
    socstat_tools: &[Arc<dyn Tool>],
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

    // 构建工具集：角色基础集 + 预设的数据分析通道（socstat MCP 统计工具）
    let registry = (def.build_tools)(project_path, llm, approver, read_tracker)?;
    if def.uses_socstat_mcp {
        for tool in socstat_tools {
            registry
                .register(tool.clone())
                .map_err(|e| AgentBuildError::ToolRegistry(e.to_string()))?;
        }
    }
    // 可选整体观测包装
    let registry = match reporter {
        Some(ref r) => observe_registry(&registry, r.clone()),
        None => registry,
    };

    let mut builder = FluenRuntimeBuilder::new(llm_provider)
        // 引擎单轮 LLM 超时放宽至 5 分钟（学术写作长生成），见 timeouts 分层
        .with_config(motis_engine_config(SUBAGENT_AWAITING_TIMEOUT))
        .with_tools(registry, crate::agent_runtime::approval::approval_executor(SUBAGENT_TOOL_TIMEOUT));
    if let Some(reporter) = reporter {
        builder = builder.with_observer(reporter);
    }

    Ok((thinking_enabled, builder.build()))
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
        let def = find_agent_def(&AgentId::EssayWriting).unwrap();
        assert_eq!(def.id, AgentId::EssayWriting);
        assert!(!def.name.is_empty());
        assert!(!def.description.is_empty());
    }

    #[test]
    fn prompts_are_nonempty() {
        assert!(!build_essay_writing_prompt().is_empty());
        assert!(!build_essay_review_prompt().is_empty());
        assert!(!build_essay_critique_prompt().is_empty());
        assert!(!build_knowledge_builder_prompt().is_empty());
        assert!(!build_data_analyst_prompt().is_empty());
    }

    #[test]
    fn socstat_mcp_preset_only_for_data_analyst() {
        for def in all_agent_defs() {
            assert_eq!(
                def.uses_socstat_mcp,
                def.id == AgentId::DataAnalyst,
                "{} 的 socstat 预设不符",
                def.id
            );
        }
    }
}
