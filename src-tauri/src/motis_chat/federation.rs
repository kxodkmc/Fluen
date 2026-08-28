//! # Motis 子智能体联邦——referee 内核拓扑
//!
//! 以 referee 的 Kernel + Extension 机制承载 Motis 的子智能体编排：
//!
//! - [`Federation`]：一个长寿命的 [`Kernel`](referee_core::Kernel)，其上以
//!   [`AgentRuntime`] 扩展注册各子代理引擎。子代理运行时按「指纹」
//!   （项目路径 × LLM 配置 × 启用清单）构建并**跨委派复用**，指纹变化时
//!   整体重建——复用是拓扑的自然结果，而非外挂缓存。
//! - [`FederationPool`]：进程级共享槽（tauri State），双检锁惰性构建。
//! - [`DelegationTracker`]：进程级「进行中委派」登记表——供取消传播
//!   （中断仍在运行的子会话）与事件路由（子代理工具事件回溯父委派）共享。
//! - [`ProjectArtifactStore`](super::artifact_store::ProjectArtifactStore)：
//!   项目级持久化成果板，随联邦构建打开、落盘于用户数据目录，跨轮次与
//!   重启可读（内置内存版按会话分板，与本应用的按轮会话模型不兼容）。
//!
//! ## 委派协议
//!
//! 委派工具（[`super::delegate::DelegateAgentTool`]）经
//! `kernel.invoke(runtime_id, envelope, timeout)` 把任务作为**全新会话**
//! 派发给目标子代理（每次委派独立会话，保留隔离语义；`peer_depth + 1`
//! 透传，深度门控由引擎 `max_subagent_depth` 兜底）。大结果或显式
//! `artifact_ref` 模式落入项目成果板，仅回传 `artifact_id`，
//! Motis 可经 `list_my_board` / `read_artifact` 自主取回。

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

use referee_agent::artifact::StoreConfig;
use referee_agent::AgentRuntime;
use referee_ai::tool::Tool;
use referee_core::kernel::SupervisionPolicy;
use referee_core::{CapabilityId, Kernel};
use tokio::sync::RwLock;

use crate::agent_runtime::approval::Approver;
use crate::llm_config::model::LlmConfig;
use crate::mcp_host::socstat::SocstatMcpHost;

use super::agent_reporter::AgentReporter;
use super::artifact_store::ProjectArtifactStore;
use super::agents::{self, AgentId, AgentBuildError};

/// 每个扩展的入站队列容量（委派为低频操作，小队列即可）。
const EXTENSION_QUEUE_SIZE: usize = 64;

/// 工件板容量上限（与 referee 默认一致：1024 个 / 64 MiB）。
const STORE_MAX_ARTIFACTS: usize = 1024;
const STORE_MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;

/// 进行中的委派信息——取消传播与事件路由的共享数据。
#[derive(Debug, Clone)]
pub struct DelegationInfo {
    /// 目标子智能体。
    pub agent_id: AgentId,
    /// 父级 `delegate_agent` 工具调用 ID（前端事件关联键）。
    pub parent_tool_call_id: String,
    /// 任务预览（事件展示用，已截断）。
    pub task_preview: String,
    /// 委派发起时刻（耗时统计）。
    pub started_at: std::time::Instant,
}

/// 「进行中委派」登记表（子会话 → 委派信息）。
///
/// 进程级共享：[`Federation`] 重建时登记表保留（旧条目先被
/// 中断清空），使子代理工具事件始终能回溯到父委派。
#[derive(Default)]
pub struct DelegationTracker {
    inflight: std::sync::Mutex<HashMap<uuid::Uuid, DelegationInfo>>,
}

impl DelegationTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一个进行中的委派子会话。
    pub fn track(&self, session_id: uuid::Uuid, info: DelegationInfo) {
        self.inflight
            .lock()
            .expect("delegation tracker poisoned during track")
            .insert(session_id, info);
    }

    /// 注销委派并返回其信息（不存在时为 `None`）。
    pub fn end(&self, session_id: &uuid::Uuid) -> Option<DelegationInfo> {
        self.inflight
            .lock()
            .expect("delegation tracker poisoned during end")
            .remove(session_id)
    }

    /// 查询委派信息（子代理工具事件路由用）。
    pub fn lookup(&self, session_id: &uuid::Uuid) -> Option<DelegationInfo> {
        self.inflight
            .lock()
            .expect("delegation tracker poisoned during lookup")
            .get(session_id)
            .cloned()
    }

    /// 取出全部进行中的委派（批量中断用）。
    fn drain(&self) -> Vec<(uuid::Uuid, DelegationInfo)> {
        self.inflight
            .lock()
            .expect("delegation tracker poisoned during drain")
            .drain()
            .collect()
    }
}

/// 已注册的子代理：内核能力 ID + 运行时控制句柄。
pub struct RegisteredAgent {
    /// 内核路由用的能力 ID（invoke 目标）。
    runtime_id: CapabilityId,
    /// 运行时控制句柄（中断子会话；Clone 与注册实例共享内部状态）。
    runtime: AgentRuntime,
    /// 构建运行时时解析的思考模式开关（模型能力驱动）。
    thinking_enabled: bool,
}

impl RegisteredAgent {
    pub fn runtime_id(&self) -> CapabilityId {
        self.runtime_id
    }

    pub fn thinking_enabled(&self) -> bool {
        self.thinking_enabled
    }
}

/// 子智能体联邦——长寿命内核 + 注册的子代理扩展。
pub struct Federation {
    fingerprint: u64,
    kernel: Kernel,
    store: Arc<ProjectArtifactStore>,
    agents: HashMap<AgentId, RegisteredAgent>,
    /// 进行中委派登记表（与进程级 tracker 共享同一实例）。
    tracker: Arc<DelegationTracker>,
}

impl Federation {
    /// 构建联邦：为每个启用的子代理构建引擎并注册为内核扩展。
    ///
    /// `store` 为项目级持久化成果板（由调用方 [`FederationPool`] 按项目
    /// 打开后注入，测试可注入临时目录实例）；`tracker` 为进程级登记表
    /// （跨联邦重建共享）；`reporter` 非空时子代理工具集整体观测包装，
    /// 并注入引擎观测器（增量透传 + 失败兜底）；`socstat` 为应用级
    /// socstat MCP 统计服务器宿主（`None` 时数据分析通道降级为空）。
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn build(
        llm: &LlmConfig,
        project_path: &str,
        approver: Arc<dyn Approver>,
        read_tracker: Arc<crate::agent_tools::project::read_state::ReadTracker>,
        enabled_agents: &[String],
        fingerprint: u64,
        tracker: Arc<DelegationTracker>,
        reporter: Option<Arc<AgentReporter>>,
        store: Arc<ProjectArtifactStore>,
        socstat: Option<SocstatMcpHost>,
    ) -> Result<Self, AgentBuildError> {
        let ids: Vec<AgentId> = if enabled_agents.is_empty() {
            AgentId::all().to_vec()
        } else {
            AgentId::all()
                .iter()
                .filter(|a| enabled_agents.contains(&a.as_str().to_string()))
                .cloned()
                .collect()
        };

        // socstat MCP 工具快照：宿主待机连接跨联邦重建复用；
        // 失败/未启用时为空列表，数据分析助手退化为无统计工具
        let socstat_tools: Vec<Arc<dyn Tool>> = match &socstat {
            Some(host) => host.tools().await,
            None => Vec::new(),
        };

        let kernel = Kernel::new();

        let mut agents = HashMap::new();
        for id in ids {
            let (thinking_enabled, runtime) = agents::build_agent_runtime(
                &id,
                llm,
                project_path,
                approver.clone(),
                read_tracker.clone(),
                reporter.clone(),
                &socstat_tools,
            )?;
            let agent_rt =
                AgentRuntime::new(runtime.engine().clone()).with_artifact_store(store.clone());
            kernel
                .register(
                    Box::new(agent_rt.clone()),
                    EXTENSION_QUEUE_SIZE,
                    SupervisionPolicy::Transient,
                )
                .await
                .map_err(|e| AgentBuildError::Kernel(format!("扩展注册失败: {e}")))?;
            agents.insert(
                id,
                RegisteredAgent {
                    runtime_id: agent_rt.id(),
                    runtime: agent_rt,
                    thinking_enabled,
                },
            );
        }

        Ok(Self {
            fingerprint,
            kernel,
            store,
            agents,
            tracker,
        })
    }

    fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    /// 共享内核（注入 Motis 执行器，使 `ctx.kernel` 可用）。
    pub fn kernel(&self) -> &Kernel {
        &self.kernel
    }

    /// 项目成果板存储（Motis 侧注册读取工具时使用）。
    pub fn artifact_store(&self) -> Arc<ProjectArtifactStore> {
        self.store.clone()
    }

    /// 进行中委派登记表（委派工具登记 / 事件路由查询共用）。
    pub fn tracker(&self) -> &Arc<DelegationTracker> {
        &self.tracker
    }

    /// 取目标子代理的注册信息；未启用/未构建返回 `None`。
    pub fn agent(&self, id: &AgentId) -> Option<&RegisteredAgent> {
        self.agents.get(id)
    }

    /// 结束一个委派子会话：注销登记并中断对应子会话（幂等）。
    ///
    /// 中断对已完成的会话无副作用；对仍在运行的子会话（外层超时
    /// 切断、取消传播等场景）可终止其后台空跑。
    pub fn end_child(&self, session_id: &uuid::Uuid) {
        if let Some(info) = self.tracker.end(session_id) {
            if let Some(registered) = self.agents.get(&info.agent_id) {
                registered.runtime.interrupt(*session_id);
            }
        }
    }

    /// 中断所有仍在运行的子会话（父回合被取消/异常退出时的兜底清理）。
    pub async fn interrupt_children(&self) {
        for (session_id, info) in self.tracker.drain() {
            if let Some(registered) = self.agents.get(&info.agent_id) {
                registered.runtime.interrupt(session_id);
            }
        }
    }
}

/// 进程级联邦槽——按指纹惰性构建 / 失效重建（tauri State）。
///
/// LLM 配置、项目切换、启用清单任一变化都会产生新指纹，
/// 下次访问时整体重建联邦（旧内核随槽位替换被丢弃）。
/// 委派登记表 [`DelegationTracker`] 与槽位同生命周期、跨重建共享。
pub struct FederationPool {
    slot: RwLock<Option<Arc<Federation>>>,
    tracker: Arc<DelegationTracker>,
}

impl Default for FederationPool {
    fn default() -> Self {
        Self {
            slot: RwLock::new(None),
            tracker: Arc::new(DelegationTracker::new()),
        }
    }
}

impl FederationPool {
    pub fn new() -> Self {
        Self::default()
    }

    /// 进行中委派登记表（构造事件上报器时注入，见 `agent_reporter`）。
    pub fn tracker(&self) -> Arc<DelegationTracker> {
        self.tracker.clone()
    }

    /// 取当前指纹匹配的联邦，不存在则构建。
    ///
    /// `reporter` 为子代理事件上报器（工具观测 + 引擎观测双通道），
    /// 仅在（重）构建联邦时注入；`socstat` 为应用级 socstat MCP
    /// 统计服务器宿主（注入 `data_analyst` 工具集，见 `agents`）。
    pub async fn get_or_build(
        &self,
        llm: &LlmConfig,
        project_path: &str,
        approver: Arc<dyn Approver>,
        read_tracker: Arc<crate::agent_tools::project::read_state::ReadTracker>,
        enabled_agents: &[String],
        reporter: Option<Arc<AgentReporter>>,
        socstat: Option<SocstatMcpHost>,
    ) -> Result<Arc<Federation>, AgentBuildError> {
        let fp = fingerprint(llm, project_path, enabled_agents);

        // 快路径：读锁命中
        {
            let slot = self.slot.read().await;
            if let Some(fed) = slot.as_ref() {
                if fed.fingerprint() == fp {
                    return Ok(fed.clone());
                }
            }
        }

        // 慢路径：写锁 + 双检（避免并发重复构建）
        let mut slot = self.slot.write().await;
        if let Some(fed) = slot.as_ref() {
            if fed.fingerprint() == fp {
                return Ok(fed.clone());
            }
        }
        // 重建前中断旧联邦仍在运行的子会话（防后台空跑与登记残留）
        if let Some(old) = slot.as_ref() {
            old.interrupt_children().await;
        }
        // 打开项目级持久化成果板（随联邦生命周期；跨重建从磁盘恢复条目）
        let store = Arc::new(
            ProjectArtifactStore::open(
                project_path,
                StoreConfig {
                    max_artifacts: STORE_MAX_ARTIFACTS,
                    max_total_bytes: STORE_MAX_TOTAL_BYTES,
                },
            )
            .map_err(|e| AgentBuildError::Kernel(format!("成果板存储初始化失败: {e}")))?,
        );
        let fed = Arc::new(
            Federation::build(
                llm,
                project_path,
                approver,
                read_tracker,
                enabled_agents,
                fp,
                self.tracker.clone(),
                reporter,
                store,
                socstat,
            )
            .await?,
        );
        *slot = Some(fed.clone());
        Ok(fed)
    }

    /// 兜底中断当前联邦中仍在运行的子会话（无联邦时为空操作）。
    pub async fn interrupt_children(&self) {
        let slot = self.slot.read().await;
        if let Some(fed) = slot.as_ref() {
            fed.interrupt_children().await;
        }
    }
}

/// 计算联邦指纹：项目路径 × LLM 配置（Debug 全量）× 启用清单。
fn fingerprint(llm: &LlmConfig, project_path: &str, enabled_agents: &[String]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    project_path.hash(&mut hasher);
    format!("{llm:?}").hash(&mut hasher);
    enabled_agents.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_changes_with_inputs() {
        let llm = LlmConfig::default();
        let enabled: Vec<String> = vec![];

        let base = fingerprint(&llm, "/tmp/project-a", &enabled);
        assert_eq!(base, fingerprint(&llm, "/tmp/project-a", &enabled));

        assert_ne!(base, fingerprint(&llm, "/tmp/project-b", &enabled));
        assert_ne!(
            base,
            fingerprint(&llm, "/tmp/project-a", &vec!["data_analyst".to_string()])
        );
    }

    #[test]
    fn tracker_track_lookup_end_roundtrip() {
        let tracker = DelegationTracker::new();
        let sid = uuid::Uuid::new_v4();
        let info = DelegationInfo {
            agent_id: AgentId::EssayWriting,
            parent_tool_call_id: "call-1".into(),
            task_preview: "撰写引言".into(),
            started_at: std::time::Instant::now(),
        };

        tracker.track(sid, info.clone());
        let found = tracker.lookup(&sid).expect("tracked delegation missing");
        assert_eq!(found.parent_tool_call_id, "call-1");
        assert_eq!(found.agent_id, AgentId::EssayWriting);

        let ended = tracker.end(&sid).expect("end returns delegation");
        assert_eq!(ended.parent_tool_call_id, "call-1");
        assert!(tracker.lookup(&sid).is_none(), "ended delegation must be removed");
        assert!(tracker.end(&sid).is_none(), "double end is a no-op");
    }
}
