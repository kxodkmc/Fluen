# Issue：成果板按「会话作用域」隐式锁定，按轮重建会话的接入方跨轮必然失效

> 反馈方：Fluen 桌面端 Motis 总督对话模块
> 目标仓库：`crates/referee`（referee-agent）
> 性质：**接口对齐 + 约束文档化请求**（非 bug 报告——在 referee 默认的「会话跨轮存活」模型下，现有设计自洽）

---

## 一、一句话概述

成果板（`ArtifactStore` / `ListMyBoard`）把「板归属」锚定在**会话 UUID** 上，且 `ListMyBoard` 硬编码 `ctx.session_id` 作为查询键。对于「每轮重建会话、跨轮回放文本历史」的接入方（Fluen 当前的集成模式），成果板在委派发生的当轮之外完全不可见：`list_my_board` 永远返回空，`read_artifact` 因 ID 丢失无法命中。

## 二、复现路径（Fluen 集成）

1. 每轮对话：新建 `SessionId::new_v4()` → 回放前端 user/assistant **纯文本**历史（工具结果不回放）→ `chat_stream`。
2. 第 N 轮：Motis 调 `delegate_agent(reply_mode=artifact_ref)` → 成功落库（板 key = 当轮 session UUID）→ 工具结果（含 artifact_id）仅存在于当轮上下文，轮末随会话丢弃。
3. 第 N+1 轮：新 session UUID。`list_my_board` → `list_by_creator(新 UUID)` → 空数组；`read_artifact` → LLM 只能凭上轮文本总结"回忆" ID（实为幻觉）→ `"artifact not found"`。
4. 实测日志：一次委派 3 个子代理全部成功落库，约 2 分钟后（同一运行内）读取 3 次全部 not found。

## 三、责任归属（先自我归责）

- **artifact_id 跨轮丢失**：我们的会话/历史模型所致（工具结果不回放），由我们修复（每轮注入成果板清单）。
- **但即便 ID 还在，`list_my_board` 在新会话中也拿不到板**：板创建的 key 我们本可以传稳定值（`ensure_board` 接受任意 UUID），可**读取路径的 key 在内置工具里写死**为当前会话，接入方无法通过自定义 `ArtifactStore` 实现完全绕开。会话稳定性这个前提目前只存在于实现里，文档未提及。

## 四、请求两点

### 1. 约束文档化 / 作用域可选（低改动）

- 在文档中明确「成果板按会话作用域，要求调用者会话跨轮稳定」；**或**
- 给 `ListMyBoard`（以及 `AgentTool` 的落库路径）提供稳定作用域选项，如允许注入 board key 或暴露 board_id 查询。

### 2. `ArtifactStore` trait 稳定性确认（配合你们的持久化接口规划）

- 我们正基于现有 trait 实现外部持久化 store（项目作用域）。请确认计划中的持久化接口**不会对该 trait 做破坏性变更**；如有草案请提前分享，我们直接按草案实现，避免返工。

## 五、正面反馈

- `Artifact` 已 derive `Serialize/Deserialize`，trait 对象安全、`Send + Sync`——**外部持久化实现今天就可落地**，不需要内置持久化能力，与你们的分工设想一致。
- ID 凭证语义（持 ID 即可读正文）与多轮/多智能体场景契合良好。

## 六、我们侧的对策（供参考，不阻塞）

- 自研 `ProjectArtifactStore`：持久化到用户数据目录、按项目隔离、store 实例即项目板（绕开会话作用域）；
- 每轮把成果板清单（artifact_id + 标题）注入系统提示词，跨轮可读；
- 后续若持久化接口与现有 trait 有差异，迁移成本仅在我们自己的 store 实现内。

---

*2026-08-28 · Fluen v0.0.1-beta*
