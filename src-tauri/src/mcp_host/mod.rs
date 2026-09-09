//! # mcp_host — 应用级 MCP 服务器宿主
//!
//! 以 stdio 子进程方式托管外部 MCP 服务器（referee `McpServer` 桥）：
//! **应用启动时连接**、空闲待机、**应用退出时优雅停机**。已发现工具以
//! `Arc<dyn Tool>` 快照供智能体工具注册表共享——子进程与连接跨联邦
//! 重建复用，不随会话启停。
//!
//! ## 模块结构
//!
//! | 子模块 | 职责 |
//! |--------|------|
//! | [`process`] | [`process::McpProcess`]：单个 MCP 服务器的生命周期包装（连接 / 工具快照 / 停机）。连接失败降级为空工具集，失败状态即时清除、下次访问重试 |
//! | [`socstat`] | [`socstat::SocstatMcpHost`]：socstat-mcp 统计服务器专属配置、二进制定位与 tauri State |
//!
//! ## 扩展
//!
//! 接入新的 MCP 服务器 = 新增一个 sibling 模块（提供 [`McpServerConfig`]）
//! + 复用 [`process::McpProcess`]，生命周期逻辑零改动。
//!
//! ## 注册边界
//!
//! socstat 统计工具仅进入 `data_analyst` 工具集（见 `motis_chat::agents`
//! 的 `uses_socstat_mcp` 预设字段），不进入 Motis 总督与其他子智能体。
//! 统计工具只操作 MCP 服务器进程内的数据集状态，不写用户文件，故不套
//! 审批（ApprovalGuard）；`load_dataset` 应指向项目 `data/` 目录（提示词约定）。

pub mod process;
pub mod socstat;
