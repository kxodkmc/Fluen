# 智能体工具重构 — 交接文档

> 更新：2026-08-23 ｜ 范围：`src-tauri/src/agent_tools`、`src-tauri/src/motis_chat`、`crates/fluen-knowledge` 及关联装配/提示词/前端
> 状态：**三轮重构已完成并验证**（src-tauri 530 测试通过；fluen-knowledge 39 lib + 8 集成 + 5 mcp 通过；vue-tsc 与 vitest 113 通过）
> 授权记录：第三轮获准改动 `crates/fluen-knowledge`（P2 规范化）；**referee 与 socstat 未改动任何源码**，联邦化全程只组合其公开 API

---

## 一、背景

对全部 LLM 工具做了多轮质量审查与重构：

1. **第一轮**：删除多动作复合工具（`paper_content` / `project_file`），按「一工具一职责」拆分为 `paper/`、`project/` 模块；文件原语下沉 referee；写入通道收敛到 `manuscript`；落盘链路换原子写。
2. **第二轮（本轮收尾）**：
   - **检索双轨定案**：探查发现 `knowledge_*` 工具族只注册在后台知识库构建管线与未被引用的 MCP server，从不进入用户侧聊天运行时——双轨实为分层。**保留 `literature_search` 为聊天运行时唯一检索入口**（score 改数值、输出补 `success`、注册逻辑下沉单一出处）；`knowledge_*` 维持管线专用定位。
   - **委派双轨定案**：评估 referee `AgentTool` 迁移时发现关键事实——`AgentRuntime::handle` 把信封中的 `session_id` 原样转发给引擎，「固定目标会话」只是 `AgentTool` 构造函数的封装选择而非协议约束。据此将 Motis 子智能体编排**联邦化**为 referee Kernel 拓扑（见下文），既符合「原语下沉 referee」哲学，又完整保留每次委派独立会话的产品语义。
   - **知识库工具规范化（P2 全套）**：8 个 `knowledge_*` 工具描述中文化、score 数值化、`list_entries` 加 limit 分页、`meta` 按 query_type 裁剪字段、get/delete 未找到语义统一为 `found:false`、`edit_entry.edits[]` 条件必填前置校验。
   - **小项清理**：creator 落盘原子写、四处重复的 project 三件套注册收敛为共享装配层、前端 `append` 死分支与 i18n 死键清除、文档表修正。

## 二、当前工具全景

### 分层模型（理解全局的关键）

```
语义层（懂论文结构）    paper_outline / paper_section / manuscript
业务层（Fluen 领域）    literature_search / delegate_agent / submit_plan
原语层（通用文件门面）  project_read / project_write / project_edit
成果板（referee）       list_my_board / read_artifact（Motis 已注册，配合委派大结果）
知识库管线专用          knowledge_query 等 8 个（fluen-knowledge；仅后台构建管线与 MCP）
```

- **读路径开放**：`project_read` 可读 main.md 原文（含标记），结构化访问走 `paper_*`——二者是"文件管理器 vs 大纲视图"，不冲突。
- **写路径收敛**：正文唯一通道 `manuscript`；`project_write`/`project_edit` 在安全层显式拒绝触碰 `main.md`。
- **检索唯一**：聊天运行时的检索入口只有 `literature_search`；`knowledge_*` 从不与其共存于同一注册表。

| 工具 | 位置 | 审批 | 说明 |
|---|---|---|---|
| `paper_outline` | `agent_tools/paper/outline.rs` | 免 | H1-H6 标题树 |
| `paper_section` | `agent_tools/paper/section.rs` | 免 | 章节定位读取；未找到时列可用标题 |
| `manuscript` | `agent_tools/manuscript.rs` | ✅ | 正文唯一写入通道：markup 校验 + 章节同步 |
| `project_read` | `agent_tools/project/read.rs` | 免 | 字符窗口 + offset 续读元数据 |
| `project_write` | `agent_tools/project/write.rs` | ✅ | referee 原子写，自动建父目录 |
| `project_edit` | `agent_tools/project/edit.rs` | ✅ | referee 唯一匹配替换 + replace_all，拒二进制 |
| `literature_search` | `agent_tools/literature.rs` | 免 | 混合检索 top4；score 数值；KB 缺失时装配降级 |
| `delegate_agent` | `motis_chat/delegate.rs` | 免 | 内核 RPC 委派子代理（每次新会话，深度 +1） |
| `list_my_board` / `read_artifact` | referee-agent | 免 | 成果板 ACL 读取（Motis 注册） |

### 联邦拓扑（本轮新增）

```
FederationPool (tauri State, 惰性构建)
└── Federation { Kernel + InMemoryArtifactStore }
    ├── AgentRuntime(academic_writer)   ← CapabilityId
    ├── AgentRuntime(knowledge_builder) ← CapabilityId
    └── AgentRuntime(data_analyst)      ← CapabilityId

Motis 引擎: approval_executor().with_kernel(kernel)
delegate_agent.execute:
  新会话 SessionId::new_v4() → SessionMessage::Chat{peer_depth+1}
  → kernel.invoke(runtime_id, timeout=10min)
  → 结果 >4KB 或异步派发 → ensure_board(父会话) 落工件板，仅回 artifact_id
取消传播: motis_chat_cancel / send 收尾 → pool.interrupt_children()
指纹失效: project_path × LLM 配置(Debug 全量) × enabled_agents 任一变化整体重建；
         子代理引擎因此跨委派复用（此前每次委派重建 HTTP client + SQLite + 工具集）
```

## 三、待办与已知事项

P1/P2/P3 清单已全部消化完毕（决策记录见「一、背景」）。当前遗留：

- `submit_plan` 是后端 capture 的伪工具（Planning 阶段结构化输出用），混在正常工具命名空间；可接受，知晓其特殊性即可（`knowledge_builder/llm_helper.rs`）。
- 若未来需要日志追加场景，再单独设计 append 类工具（当前无调用方）。
- referee 自带的 `read/write/edit` 至今未被 Fluen 直接注册（角色是被组合的原语）；`list_my_board` / `read_artifact` 本轮已随成果板闭环注册进 Motis。
- `data_analyst` 目前只有文件读写三件套，没有真正的分析执行工具（统计分析经 `socstat` 的能力只以 Tauri 命令形式服务前端）——如需让该子代理真正跑统计，需后续为其设计并注册数据分析工具（独立立项）。
- MCP server 是 `knowledge_*` 工具的平行实现（未复用 `tools.rs`）；本轮 score 类型等变更未同步其 schema，潜在外部 MCP 客户端属破坏性变更（Fluen 应用自身不受影响）。

## 四、新工具设计约定（后续遵循）

1. **单一职责**：一个工具一个动作，禁止 action 枚举复合工具。
2. **原语下沉 referee**：文件 IO 组合 referee 工具的 `execute()`；Fluen 层只做三件事——相对路径门面、安全校验、审批包装。
3. **安全层模式**：参照 `ProjectFs`（词法校验 → 保护规则 → symlink 拒绝 → canonicalize 包含检查四层）。
4. **错误分类**：参数错 → `InvalidArguments`；执行失败 → `Execution`；「章节未找到」类返回可用选项列表助模型自纠。
5. **输出**：结构化 JSON + 导航元数据（`truncated`/`end` 等）；score 用数值。
6. **语言**：description 与参数描述统一中文。
7. **trait 实现**：显式 `category() = Remote`、`default_wait() = true`（同步查询类）。
8. **测试基线**：每个工具覆盖 正常路径 / 缺参 / 越界与保护路径拒绝 / 边界（中文、二进制、symlink、空文件）。
9. **编排下沉 referee**：多智能体协作一律走 Kernel + AgentRuntime 拓扑（`motis_chat/federation.rs`）；委派类工具只组合 `kernel.invoke` + `SessionMessage` 协议，不自行管理子运行时生命周期；大结果遵循「>4096 字节或异步派发 → 工件板按调用者分板落库」策略。

## 五、关键文件地图

```
src-tauri/src/
├── agent_tools/
│   ├── mod.rs                  # 模块清单文档（含检索分层唯一性约定）
│   ├── assemble.rs             # 运行时装配辅助：paper/project/manuscript/literature 共享注册组合
│   ├── parse.rs                # 大纲/章节解析纯函数层
│   ├── literature.rs           # literature_search（score 数值）+ open_kb 公共装配入口
│   ├── manuscript.rs           # manuscript（正文唯一写通道）
│   ├── paper/
│   │   ├── mod.rs              # PaperReader（referee ReadTool 封装 + 迁移兜底）
│   │   ├── outline.rs          # paper_outline
│   │   └── section.rs          # paper_section
│   └── project/
│       ├── mod.rs              # ProjectFs 安全层（.git/main.md 保护）
│       ├── read.rs / write.rs / edit.rs
├── project/
│   ├── atomic.rs               # 同步原子写（loader/section/creator 落盘共用）
│   └── creator.rs              # 项目创建（四处落盘已换原子写）
├── motis_chat/
│   ├── runtime.rs              # Motis 注册点（async；联邦就绪 + 内核注入执行器）
│   ├── federation.rs           # 联邦：Kernel + AgentRuntime 扩展注册 + FederationPool（指纹复用）
│   ├── delegate.rs             # delegate_agent：内核 RPC 薄组合（新会话 + 工件板落库）
│   ├── commands.rs             # motis_chat_send/cancel（含子会话中断兜底）
│   ├── agents.rs               # 三个子代理注册点（工具集经 assemble 装配）
│   └── approval.rs             # needs_approval 审批判定
├── ai_assistant/runtime.rs     # 学术助手注册点（经 assemble 复用装配）
crates/fluen-knowledge/src/tools.rs  # knowledge_* 八件套（已规范化：中文描述/score 数值/limit/meta 裁剪/found 语义/edit 前置校验）
src/views/main/components/motis/toolDescribe.ts  # 工具调用展示映射
```

## 六、验证方式

```bash
cd src-tauri && cargo test --lib     # 530 passed
cargo test -p fluen-knowledge --features "async,tools"   # 39 lib + 8 集成 + 5 mcp
npx vue-tsc --noEmit                 # 无错误
pnpm vitest run                      # 113 passed
grep -rn "paper_content\|project_file\|PaperContentTool\|ProjectFileTool" src-tauri/src src
# 仅命中新函数名 register_project_files（子串巧合），旧工具残留为 0
```
