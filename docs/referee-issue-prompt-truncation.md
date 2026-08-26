# Issue：会话 prompt 预算默认过小，长用户消息被静默整条丢弃

> 反馈方：Fluen 桌面端知识库构建模块
> 目标仓库：`crates/referee`（referee-ai）
> 严重级别：高（静默数据丢失，导致产物与输入完全不符）

---

## 一、一句话概述

referee-ai 的 `SessionConfig.prompt_budget_tokens` 默认值只有 **8000 token**。当调用方向引擎提交的**单条 user 消息**本身就超过该预算时，[`prompt::History::truncate`] 会把这条消息**整体丢弃**，且**零日志、零报错**。模型因此根本收不到这段内容。

## 二、用户可见症状

Fluen 的「知识库构建」任务会把**整篇文献原文**放入 Planning 阶段的单条 user 消息（约 1.4 万～1.9 万字符 ≈ 1 万+ token），超过 8000 预算后被整体静默丢弃。结果：

- 模型只看到 system 提示词 + 工具声明，看不到论文正文；
- 因此对任何论文都产出泛化、与文献主题完全无关的摘要 / 概念（实测：deepseek 反复编"数智化"综述，mimo 反问用户要文献、创建"文献处理服务"等元概念）；
- 该现象在 4 个不同项目、多种模型上稳定复现，且**无任何日志可指向根因**，极难排查。

## 三、根因（代码定位）

### 3.1 预算默认值

`crates/referee/referee-ai/src/session/mod.rs`

```rust
impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            // ...
            prompt_budget_tokens: 8000,   // ← 过小
            // ...
        }
    }
}
```

### 3.2 超预算时静默丢弃

`crates/referee/referee-ai/src/prompt/mod.rs` → `PromptFragment::truncate` 的 `History` 分支：

```rust
PromptFragment::History(msgs) => {
    let mut current_tokens = 0u64;
    let mut keep = VecDeque::new();
    for msg in msgs.iter().rev() {
        let cost = TokenEstimator::estimate(msg.content.as_text().unwrap_or(""));
        if current_tokens + cost > budget {
            break;                       // ← 单条消息超预算：直接 break
        }
        current_tokens += cost;
        keep.push_front(msg.clone());
    }
    // 若 keep 为空 → 返回 None → finalize 中静默丢弃该片段
    ...
}
```

`finalize` 中片段被丢的路径：

```rust
match fragment.truncate(remaining) {
    Some((kept, cost)) => { ... }
    None => {
        // ← 预算不足丢弃该片段；此处无任何 WARN/ERROR 日志
    }
}
```

### 3.3 叠加影响

- 调用方（如 Fluen）只把 `budget.session_limit` 调大，却未同步抬 `session.prompt_budget_tokens`，于是该截断持续静默发生。
- `History::truncate` 是**按消息整条保留/丢弃**的（不做单条消息内的片段截断），因此一条超预算的文献消息 = 整条消失。

## 四、复现（最小化）

用一段 > 8000 token 的单条 user 消息 + 默认会话配置发起 chat，观察发给 provider 的 `ChatRequest.messages` 中该 user 消息不存在，且无日志告警。

Fluen 已在本地用 mock provider 写出了复现/验证用例（`src-tauri/src/knowledge_builder/pipeline.rs` 的 `planning_injects_literature_to_model_and_roundtrips_plan`）：
- 默认 8000 预算：注入的长文献**末端标记收不到**（复现 bug）；
- 将 `prompt_budget_tokens` 抬高到 128K：末端标记**完整送达**（证明根因与修复方向）。

## 五、建议修复

> 以下为**建议**，具体实现以维护者评估为准。核心诉求：**长消息超预算时不再静默，要么放行，要么大声告警。**

### 5.1 降低默认截断对合法长文本的误伤

- 将 `prompt_budget_tokens` 默认抬高到与常见模型上下文一致的量级（如 `128 * 1024`），或由调用方按模型 `ModelSpec::context_window_tokens` 显式配置，避免"合法长文献被默认小预算误截"。
- 调用方侧（Fluen）已改为显式设置 `engine_config.session.prompt_budget_tokens = 128 * 1024`。

### 5.2 静默丢失 → 显式告警（本次重点）

在以下三处补 WARN 级日志（`tracing::warn!`），使后续任何截断/丢失都不再无声：

1. `PromptFragment::truncate` 的 `History` 分支：
   - 发生 `break` 时记录 `dropped_messages`、`dropped_tokens_estimate`、`budget`；
   - 当 `keep` 为空（**整段 history 被丢弃**）时，输出一条醒目 WARN："history fragment fully dropped: a message exceeds prompt budget"；
   - **单条消息自身超预算被整体丢弃是最危险的路径，必须单独、明确告警**（现在的 `break` 无任何输出）。

2. `finalize` 中 `fragment.truncate(remaining)` 返回 `None` 时：`WARN` 记录片段类型（System/Tools/History/Memory/Artifacts）与剩余预算。

3. `PromptFragment::System` 按字符截断文本（`System` 分支）时：`WARN` 记录截断前后估算 token 数（`before/after`），并保留现有 `...[Truncated]` 后缀。

### 5.3 可选：提供可观测回执

- 若可行，在 `ChatRequest` 或返回结构中暴露"是否发生截断"标记（如 `was_truncated: bool`），便于上层在收敛诊断时直接发现数据被裁，而不必靠日志大海捞针。

## 六、为什么需要修复

- 这是**静默数据损坏**：产物内容与实际输入不一致，且无任何错误信号，会直接污染下游知识库条目。
- 排查成本极高：本次从用户反馈到定位，跨越了模型选择、文件寻址、提示词注入等多个假设，最终才锁到预算截断——本可在事发当场由一行 WARN 解决。

---
*附：本问题是 Fluen 侧在接入 referee-ai 知识库构建时发现的。除上述 referee 修复外，Fluen 侧已通过 `prompt_budget_tokens=128K` 穿行并加测试覆盖。*