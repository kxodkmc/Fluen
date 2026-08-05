# 知识库构建 V2 设计方案

> **版本**：V2.1（修复预算估算与异常处理缺陷）
> **时间**：2026 年 8 月
> **状态**：待审核
> **作者**：Fluen Team

## 一、背景与目标

### 1.1 现状问题（V1）

1. **跨文献概念碎片化**：Planning 阶段才 `query` 已有条目，AI 缺乏全局视野，易创建重复条目。
2. **上下文浪费**：每篇文献独立 runtime，无法利用模型前缀缓存（DeepSeek / Anthropic 可降本 50-90%）。
3. **检索时机单一**：仅 Planning 做去重，每条创建前未查重，无法应对 Planning 后知识库变化。

### 1.2 设计目标

- **质量提升**：AI 创建条目前先看全局 Index，每条创建前做 L2 混合检索查重
- **成本降低**：跨论文会话复用 runtime，命中模型前缀缓存
- **稳定优先**：基于真实 usage 跟踪上下文，瞬态错误保留会话重试，结构性错误才销毁
- **可恢复**：任务中断后从 checkpoint 断点续传，已创建条目不丢失

### 1.3 约束

- 严格遵守 [AGENTS.md] 模块化规范，单文件不超 800 行
- 代码简洁、无冗余、易于维护与扩展
- 不破坏 V1 现有 checkpoint 格式，向后兼容

---

## 二、核心设计

### 2.1 设计概览

```
┌─────────────────────────────────────────────────────────┐
│  每次构建任务                                            │
│                                                          │
│  1. 实时读取 index.md → 提取紧凑快照（屏蔽 ID）          │
│  2. 计算上下文预算（128K 门槛 + 75% 上限）               │
│  3. 获取/重建会话（跨论文复用 runtime）                  │
│  4. 执行 Pipeline：                                      │
│     ├─ Planning：注入快照，AI 全局视野规划               │
│     └─ Execution：每条创建前 L2 混合检索查重             │
│        每次调用后从 usage 更新真实上下文占用             │
│  5. 瞬态错误 → 指数退避重试（保留会话）                  │
│     结构性错误 → 销毁会话                                │
│  6. 成功 → 检查预算，决定下次是否复用                    │
└─────────────────────────────────────────────────────────┘
```

### 2.2 Index 快照：实时提取

**不存储、不缓存**，每次构建任务从 `references/wiki/index.md` 实时解析。

| index.md 原文 | 快照输出 |
|--------------|---------|
| `- <@wiki-abc>[[concepts/wiki-abc-数智化技术]]` | `- [C] 数智化技术` |
| `- [[summaries/wiki-xyz-综述]]` | `- [S] 综述` |
| `- <@wiki-def>[[entities/wiki-def-湖南农业大学]]` | `- [E] 湖南农业大学` |
| `- tag-001-个性化学习` | `- #个性化学习` |

**优势**：始终反映磁盘最新状态；纯解析模块无状态可单测；每条约 8-15 tokens，320 条目 ≈ 3-5K tokens。

### 2.3 两层检索架构

| 层级 | 时机 | 内容 | 目的 |
|------|------|------|------|
| L1 Index 快照 | Planning 开始前 | 全部条目标题 + 标签（屏蔽 ID） | 全局去重视图 |
| L2 混合检索 | 每条创建前 | 命中条目全文 + ID（Top 3） | 更新 vs 新建决策 |

**L2 检索策略**：默认混合检索（向量 + 关键词）；Embedding 未配置时回退到 Jaro-Winkler 模糊匹配。

### 2.4 上下文预算（基于真实 usage）

**核心修正**：`history_used` 不再累加估算的输入 tokens，而是取**会话最后一次 LLM 调用的 `usage.input_tokens + usage.output_tokens`**。

**原理**：LLM API 的 `usage.input_tokens` 已包含完整历史（system + 所有历史轮次输入输出 + 当前轮输入），无需自行累加。这是 provider 返回的真实计量，避免估算误差。

| 参数 | 值 | 说明 |
|------|-----|------|
| `MIN_CONTEXT_FOR_CACHING` | 128_000 | 硬门槛，低于此值不启用会话复用 |
| `MAX_HISTORY_RATIO` | 0.75 | 历史占用比例上限 |
| 输出预留 | 16K | 固定扣除（应对单次大输出） |
| 系统开销 | 2K | 系统 prompt + 工具定义 |

**判断公式**：

```
history_budget = model_context × 0.75 - 输出预留 - 系统开销
                （注：history_used 已含 Index 快照与当前论文实际占用，不再扣除）

history_used = 最后一次 LLM 调用的 (input_tokens + output_tokens)

可追加下一篇 ⟺ supports_caching
                && history_used + next_paper_estimated ≤ history_budget

next_paper_estimated = paper_md_tokens × 2.5
                       （2.5 倍系数覆盖输出、L2 候选注入、Planning 等开销）
```

**示例**（基于真实占用，每篇实际 40-60K）：

| 模型上下文 | 历史预算 | 实际单篇占用 | 可处理论文数 |
|-----------|---------|------------|------------|
| 64K | - | - | 0（不启用缓存） |
| 128K | 78K | 50K | 1 篇 |
| 256K | 174K | 50K | 3 篇 |
| 1M | 732K | 50K | 14 篇 |

### 2.5 异常处理：分级策略

**核心修正**：放弃"任何异常销毁会话"的刚性策略，改为**错误分类 + 分级处理**。

| 错误类型 | 例子 | 会话处理 | 重试策略 |
|---------|------|---------|---------|
| **瞬态错误** | 429 / 503 / Timeout / Network | **保留会话** | 指数退避，最多 3 次 |
| **结构性错误** | Context Overflow / Schema / Parse / Auth | **销毁会话** | 不重试，标记 Failed |
| **用户取消** | CancellationToken | **保留会话** | 不重试，等用户 resume |

**指数退避**：1s → 2s → 4s，上限 30s。重试耗尽后任务 Failed，但**仍保留会话**（会话本身未损坏，下次任务可复用）。

---

## 三、模块化结构

### 3.1 新增模块

```
src-tauri/src/knowledge_builder/
├── index_snapshot.rs       # 【新增】Index 快照实时提取（纯解析）
├── context_budget.rs       # 【新增】上下文预算计算（纯计算）
├── session.rs              # 【新增】跨论文会话管理（runtime 复用 + 真实 usage 跟踪）
├── error_classify.rs       # 【新增】LLM 错误分类与重试策略
├── pipeline.rs             # 【改造】注入快照 + L2 检索 + 更新 usage
├── prompts.rs              # 【改造】新增 candidates 渲染
└── ...
```

### 3.2 模块职责

| 模块 | 职责 | 状态 |
|------|------|------|
| `index_snapshot.rs` | 从 index.md 解析紧凑快照 | 无状态 |
| `context_budget.rs` | 计算上下文预算与门槛判断 | 无状态 |
| `session.rs` | runtime 复用 + 真实 usage 跟踪 + 销毁决策 | 有状态（runtime + usage） |
| `error_classify.rs` | LLM 错误分类 + 重试策略 | 无状态 |
| `pipeline.rs` | 构建流水线主流程 | 有状态（checkpoint） |

### 3.3 模块独立性

```
index_snapshot ──┐
                 │
context_budget ──┼──→ pipeline ──→ runner
                 │         ↑
session ─────────┤         │
                 │         │
error_classify ──┘         │
                           │
recovery ──→ TaskQueueState
```

---

## 四、核心数据结构

### 4.1 IndexSnapshot

```rust
pub struct IndexSnapshot {
    pub summaries: Vec<String>,
    pub concepts: Vec<String>,
    pub entities: Vec<String>,
    pub tags: Vec<String>,
}

impl IndexSnapshot {
    pub fn parse(index_md: &str) -> Result<Self, KnowledgeBuilderError>;
    pub fn render(&self) -> String;
    pub fn estimated_tokens(&self) -> usize;
}
```

### 4.2 ContextBudget（修正：移除 current_paper_tokens）

```rust
pub const MIN_CONTEXT_FOR_CACHING: usize = 128_000;
pub const MAX_HISTORY_RATIO: f64 = 0.75;

pub struct ContextBudget {
    pub model_context: usize,
    pub reserved_for_output: usize,    // 默认 16K
    pub system_overhead_tokens: usize, // 默认 2K
}

impl ContextBudget {
    pub fn from_config(
        llm: &LlmConfig,
        model_ref: &SceneModelRef,
    ) -> Result<Self, KnowledgeBuilderError>;

    pub fn supports_caching(&self) -> bool {
        self.model_context >= MIN_CONTEXT_FOR_CACHING
    }

    /// 历史上下文可用预算（75% 上限，扣除固定开销）。
    /// 注：history_used 由 Session 从真实 usage 跟踪，已含 Index 快照与当前论文。
    pub fn history_budget(&self) -> usize {
        let fixed = self.reserved_for_output + self.system_overhead_tokens;
        (self.model_context as f64 * MAX_HISTORY_RATIO) as usize - fixed
    }

    /// 下一篇预估占用（输入 × 2.5 覆盖输出/检索/Planning 开销）。
    pub fn estimate_next_paper(paper_md_tokens: usize) -> usize {
        (paper_md_tokens as f64 * 2.5) as usize
    }
}
```

### 4.3 KnowledgeBuildSession（修正：真实 usage 跟踪）

```rust
pub struct KnowledgeBuildSession {
    runtime: confluent::ConfluentRuntime,
    budget: ContextBudget,
    processed: Vec<String>,
    /// 真实上下文占用：取会话最后一次 LLM 调用的 (input + output)
    history_used: usize,
}

impl KnowledgeBuildSession {
    pub async fn new(
        llm: &LlmConfig,
        model_ref: &SceneModelRef,
        kb: AsyncKnowledgeBase,
        budget: ContextBudget,
    ) -> Result<Self, KnowledgeBuilderError>;

    pub fn runtime(&self) -> &confluent::ConfluentRuntime;
    pub fn budget(&self) -> &ContextBudget;
    pub fn history_used(&self) -> usize;
    pub fn papers_processed(&self) -> usize;

    /// 从 LLM 响应的 usage 字段更新真实占用。
    /// 在每次 runtime.run() 返回后调用。
    /// input_tokens 已包含完整历史（system + 所有历史轮次 + 当前轮输入）。
    pub fn update_usage(&mut self, input_tokens: usize, output_tokens: usize) {
        self.history_used = input_tokens + output_tokens;
    }

    /// 记录已处理论文（仅统计用，不参与恢复）。
    pub fn record_paper(&mut self, ref_id: &str) {
        self.processed.push(ref_id.to_string());
    }

    /// 是否应该关闭会话（超 75% 或不支持缓存）。
    pub fn should_close(&self, next_paper_md_tokens: usize) -> bool {
        if !self.budget.supports_caching() { return true; }
        let next = ContextBudget::estimate_next_paper(next_paper_md_tokens);
        self.history_used + next > self.budget.history_budget()
    }
}
```

### 4.4 SessionPool

```rust
pub struct SessionPool {
    session: Option<KnowledgeBuildSession>,
}

impl SessionPool {
    pub fn new() -> Self;

    /// 获取或重建会话。
    pub async fn acquire(...) -> Result<&mut KnowledgeBuildSession, _>;

    /// 强制销毁会话（结构性错误恢复用）。
    pub fn destroy(&mut self);

    /// 保留会话（瞬态错误重试或用户取消）。
    pub fn keep(&self) { /* no-op，仅语义明确 */ }
}
```

### 4.5 LlmErrorKind（新增：错误分类）

```rust
/// LLM 错误分类——决定会话保留还是销毁。
#[derive(Debug, Clone)]
pub enum LlmErrorKind {
    /// 瞬态错误：可重试，保留会话
    Transient {
        retry_after_ms: u64,
    },
    /// 结构性错误：不可重试，销毁会话
    Structural(StructuralReason),
}

#[derive(Debug, Clone)]
pub enum StructuralReason {
    ContextOverflow,   // 上下文超限（会话已不可用）
    InvalidRequest,    // 请求格式错误
    ParseError,        // 响应解析失败（数据损坏）
    AuthError,         // 认证失败
    Other,
}

/// 重试策略配置。
pub const MAX_TRANSIENT_RETRIES: u32 = 3;
pub const MAX_RETRY_DELAY_MS: u64 = 30_000;

/// 从错误信息分类。
pub fn classify_error(err: &KnowledgeBuilderError) -> LlmErrorKind;

/// 计算指数退避延迟。
pub fn backoff_delay(attempt: u32) -> u64 {
    (1000 * 2u64.pow(attempt)).min(MAX_RETRY_DELAY_MS)
}
```

---

## 五、核心流程

### 5.1 Pipeline 主流程（改造后）

```rust
pub async fn build(
    project_path: &Path,
    ref_id: &str,
    model_ref: &SceneModelRef,
    options: &KnowledgeBuildOptions,
    checkpoint: &mut KnowledgeBuildCheckpoint,
    llm_config: &LlmConfig,
    session: &mut KnowledgeBuildSession,
    cancel: &CancellationToken,
    on_progress: impl Fn(KbBuildProgressPayload),
) -> Result<(), KnowledgeBuilderError> {
    let refs_dir = project_path.join("references");
    let kb = AsyncKnowledgeBase::init(&refs_dir)?;

    // ── 实时读取 Index 快照 ──
    let index_md = read_index_md(&refs_dir)?;
    let snapshot = IndexSnapshot::parse(&index_md)?;

    // ── 阶段 1：Planning（注入快照）──
    if checkpoint.stage == BuildStage::Planning {
        let md_content = read_md_content(project_path, ref_id)?;
        let (plan, usage) = run_planning(
            session.runtime(), &snapshot, &md_content, options, cancel,
        ).await?;
        session.update_usage(usage.input, usage.output);  // 更新真实占用
        checkpoint.plan = Some(plan);
        checkpoint.stage = BuildStage::CreatingSummary;
    }

    let plan = checkpoint.plan.as_ref().ok_or(...)?.clone();

    // ── 阶段 2a：CreatingSummary（L2 检索 + usage 更新）──
    if checkpoint.stage == BuildStage::CreatingSummary && options.create_summary {
        let candidates = kb.query(QueryParams::hybrid(&plan.summary_title())).await?;
        let (summary_id, usage) = run_create_summary_with_candidates(
            session.runtime(), ref_id, &plan.summary_points, &candidates, cancel,
        ).await?;
        session.update_usage(usage.input, usage.output);
        checkpoint.summary_id = Some(summary_id);
        checkpoint.stage = BuildStage::CreatingConcepts;
    }

    // ── 阶段 2b：CreatingConcepts（每条 L2 检索 + usage 更新）──
    if checkpoint.stage == BuildStage::CreatingConcepts && options.create_concepts {
        for (i, planned) in plan.concepts.iter().enumerate() {
            check_cancel(cancel)?;
            if planned.existing_id.is_some() { continue; }
            if i < checkpoint.concept_ids.len() { continue; }

            let candidates = kb.query(QueryParams::hybrid(&planned.title)).await?;
            let (id, usage) = run_create_concept_with_candidates(
                session.runtime(), planned, &candidates, cancel,
            ).await?;
            session.update_usage(usage.input, usage.output);

            checkpoint.concept_ids.push(id);
        }
        checkpoint.stage = BuildStage::CreatingEntities;
    }

    // ── 阶段 2c：CreatingEntities（同上）──
    // ── 阶段 2d：EstablishingRelations（已有逻辑 + usage 更新）──

    session.record_paper(ref_id);
    Ok(())
}
```

**注意**：`run_planning` / `run_create_*` 函数需返回 `(result, Usage)` 元组，从 LLM 响应中提取 usage。这要求 `confluent::ConfluentRuntime::run()` 的返回值包含 usage 字段。

### 5.2 Runner 会话管理（修正：分级错误处理）

```rust
async fn execute_knowledge_build(&self, task: &TaskRecord, ...) -> Result<(), TaskQueueError> {
    let llm_config = self.llm_storage.load()?;
    let mut checkpoint = deserialize_checkpoint(&task.checkpoint);

    // 实时提取快照 + 计算预算
    let refs_dir = self.project_path.join("references");
    let snapshot = IndexSnapshot::parse(&read_index_md(&refs_dir)?)?;
    let budget = ContextBudget::from_config(&llm_config, model_ref)?;

    // 获取或重建会话
    let session = self.session_pool.acquire(
        &llm_config, model_ref, &kb, &budget,
    ).await?;

    // 执行 pipeline，带瞬态错误重试
    let result = self.execute_pipeline_with_retry(
        session, &mut checkpoint, cancel, /* ... */
    ).await;

    // 持久化 checkpoint（无论成功失败都保存）
    let ckpt_value = serde_json::to_value(&checkpoint)?;
    self.store.update_checkpoint(&task.id, &ckpt_value)?;

    // 分级错误处理
    match result {
        Ok(()) => Ok(()),
        Err(KnowledgeBuilderError::Cancelled) => {
            // 用户取消：保留会话（缓存仍可复用）
            tracing::info!(task_id = %task.id, "用户取消，保留会话");
            Err(TaskQueueError::Cancelled)
        }
        Err(err) => {
            match classify_error(&err) {
                LlmErrorKind::Transient { .. } => {
                    // 瞬态错误重试耗尽：任务 Failed，但保留会话
                    tracing::warn!(task_id = %task.id, "瞬态错误重试耗尽，保留会话");
                    Err(TaskQueueError::Execution(err.to_string()))
                }
                LlmErrorKind::Structural(reason) => {
                    // 结构性错误：销毁会话
                    tracing::error!(task_id = %task.id, reason = ?reason, "结构性错误，销毁会话");
                    self.session_pool.destroy();
                    Err(TaskQueueError::Execution(err.to_string()))
                }
            }
        }
    }
}

/// 带瞬态错误重试的 pipeline 执行。
async fn execute_pipeline_with_retry(
    &self,
    session: &mut KnowledgeBuildSession,
    checkpoint: &mut KnowledgeBuildCheckpoint,
    cancel: &CancellationToken,
    /* 其他参数 */
) -> Result<(), KnowledgeBuilderError> {
    let mut attempt = 0u32;
    loop {
        let result = pipeline::build(
            &self.project_path, ref_id, model_ref, options,
            checkpoint, &self.llm_config, session, cancel,
            |progress| { /* emit event */ },
        ).await;

        match result {
            Ok(()) => return Ok(()),
            Err(KnowledgeBuilderError::Cancelled) => return Err(KnowledgeBuilderError::Cancelled),
            Err(err) => {
                match classify_error(&err) {
                    LlmErrorKind::Transient { retry_after_ms } => {
                        attempt += 1;
                        if attempt > MAX_TRANSIENT_RETRIES {
                            tracing::warn!(attempt, "瞬态错误重试耗尽");
                            return Err(err);
                        }
                        let delay = backoff_delay(attempt - 1).max(retry_after_ms);
                        tracing::warn!(attempt, delay_ms = delay, "瞬态错误，退避重试");
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                        // 保留会话，重试
                        // 注：pipeline 内部从 checkpoint 继续，已完成的条目不会重复
                    }
                    LlmErrorKind::Structural(_) => {
                        return Err(err);
                    }
                }
            }
        }
    }
}
```

---

## 六、中断恢复设计

### 6.1 恢复边界

| 层级 | 可恢复性 | 持久化内容 | 恢复方式 |
|------|---------|----------|---------|
| **Task 级** | ✅ 可恢复 | checkpoint（stage / plan / wiki_ids） | 从 checkpoint.stage 继续 |
| **Session 级** | ❌ 不可恢复 | runtime 对话历史在 provider 端 | 销毁后新建，从 checkpoint 继续 |

**核心保证**：会话中断 = runtime 历史丢失，但 Task 的 checkpoint 仍可恢复。新会话从 checkpoint 继续执行，业务正确性不受影响。

### 6.2 中断场景（修正：取消不销毁会话）

#### 场景 1：进程崩溃

```
Task A 执行中崩溃
  → checkpoint 已保存（如 CreatingConcepts 第 5 条）
  → 应用重启 → recovery 扫描 task-queue.json
  → status: Running → Pending（保留 checkpoint）
  → 销毁所有 SessionPool（runtime 历史无法恢复）
  → Runner 拾取 → 创建新 Session → 从第 5 条继续
```

#### 场景 2：LLM 调用失败

```
Task A 第 6 条 concept LLM 调用失败

瞬态错误（429/503/Timeout）：
  → checkpoint 保存（5 条已记录）
  → 保留会话
  → 指数退避重试（1s/2s/4s）
  → 重试成功 → 继续第 6 条
  → 重试 3 次耗尽 → 任务 Failed，会话仍保留
  → 用户 retry → 新任务复用会话（缓存仍命中）

结构性错误（Context Overflow）：
  → checkpoint 保存（5 条已记录）
  → 销毁会话（会话已不可用）
  → 任务 Failed
  → 用户 retry → 新建 Session → 从第 6 条继续
```

#### 场景 3：用户取消

```
Task A 执行中用户点击取消
  → CancellationToken 触发 → 当前 LLM 请求 abort
  → checkpoint 保存（已完成的进度）
  → 保留会话（历史仍有效，缓存可复用）
  → Task 标记 Cancelled
  → 用户点击 resume → 保留 checkpoint → 复用会话 → 从断点继续
```

### 6.3 启动时恢复流程

```rust
pub fn recover_on_startup(app: &AppHandle) -> Result<()> {
    let projects = scan_project_task_files(app)?;

    for project_path in &projects {
        let store = TaskStore::new(project_path);
        let tasks = store.list()?;

        for task in tasks {
            if task.status == TaskStatus::Running {
                tracing::warn!(task_id = %task.id, "发现中断任务，重置为 Pending");
                store.update_status(&task.id, TaskStatus::Pending)?;
            }
        }

        // 启动时必须清理会话池（runtime 历史在内存中，无法恢复）
        if let Some(state) = app.try_state::<TaskQueueState>() {
            state.destroy_session_pool(project_path);
        }
    }

    Ok(())
}
```

### 6.4 错误处理规则汇总（修正版）

| 场景 | 会话处理 | 重试 | 说明 |
|------|---------|------|------|
| 模型 context_window < 128K | 不创建会话 | - | 每篇独立任务（V1 模式） |
| 瞬态错误（429/503/Timeout/Network） | **保留** | 指数退避，最多 3 次 | 保护缓存 |
| 瞬态错误重试耗尽 | **保留** | 不再重试 | 任务 Failed，下次任务仍可复用会话 |
| 结构性错误（Context Overflow） | **销毁** | 不重试 | 会话已不可用 |
| 结构性错误（Schema/Parse/Auth） | **销毁** | 不重试 | 数据损坏或配置错误 |
| 用户取消 | **保留** | - | 缓存可复用，等用户 resume |
| 历史占用 + 下一篇 > 75% | **销毁** | - | 主动关闭，下次任务新建 |
| Embedding 配置变化 | **销毁** | - | runtime 失效 |
| 进程崩溃 | **销毁所有** | - | 启动时清理 |

---

## 七、关键设计决策

### 7.1 为什么快照实时从磁盘读取？

会话跨越多篇论文时，前一篇新增的条目需要让后一篇的 AI 看到。每次从 `index.md` 实时解析，确保 AI 始终看到最新知识库状态。`index.md` 在每次 `create_entry` 后由 `fluen-knowledge` 自动同步。

### 7.2 为什么 128K 作为硬门槛？

128K 以下可用预算不足以容纳 2 篇论文（每篇实际 40-60K），缓存复用收益低于风险。主流模型 DeepSeek(128K)、Claude Sonnet(200K)、GPT-4o(128K)、Qwen(128K) 均满足。

### 7.3 为什么 75% 而非 70%？

70% 过于保守（1M 模型浪费 300K）；75% 平衡（1M 可用 750K，预留 250K 应对单次大输出）。输出预留 16K 足够覆盖单次 LLM 响应。

### 7.4 为什么 history_used 用真实 usage 而非估算？

**V2.0 缺陷**：原设计 `record_paper` 仅累加输入 tokens，忽略输出（Planning JSON、条目正文、L2 候选注入），实际占用是估算的 2-3 倍，导致 Context Overflow。

**修正**：LLM API 的 `usage.input_tokens` 已包含完整历史（system + 所有历史轮次输入输出 + 当前轮输入），直接取最后一次调用的 `input + output` 作为 `history_used`，避免估算误差。下一篇预估用 `paper_md × 2.5` 覆盖输出/检索/Planning 开销。

### 7.5 为什么瞬态错误保留会话？

**V2.0 缺陷**：原设计"任何异常销毁会话"在面对云服务常见的瞬态错误（429/503/Timeout）时，会破坏昂贵的上下文缓存，违背"成本降低"目标。

**修正**：错误分类 + 分级处理。瞬态错误保留会话并指数退避重试（保护缓存）；结构性错误才销毁会话（会话已不可用）。用户取消也保留会话（缓存可复用）。

### 7.6 为什么会话不持有 Index 快照？

快照是知识库状态，会话是 runtime 状态，职责分离。磁盘是唯一真相源，避免会话内缓存与磁盘不一致。简化中断恢复：会话销毁无需考虑快照持久化。

### 7.7 为什么 L2 检索每条都做？

Planning 阶段的去重决策可能过时（其他任务并发创建条目）。每条创建前查重确保不创建重复条目。单次混合检索 < 100ms，相比 LLM 调用可忽略。

---

## 八、与 V1 的兼容性

### 8.1 向后兼容

| 项目 | V1 | V2 | 兼容性 |
|------|----|----|--------|
| checkpoint 格式 | stage + plan + wiki_ids | 不变 | ✅ 完全兼容 |
| TaskKind | KnowledgeBuild | 不变 | ✅ 完全兼容 |
| 事件类型 | kb-build:* | 不变 | ✅ 完全兼容 |
| 命令接口 | knowledge_build_start | 不变 | ✅ 完全兼容 |
| 知识库结构 | wiki/ 目录 | 不变 | ✅ 完全兼容 |

### 8.2 降级策略

```
model_context < 128K
  → 不创建会话
  → 每篇独立任务
  → pipeline.rs 中 session 为临时会话（用完即销毁）
  → 行为等价于 V1
```

---

## 九、实施计划

### 9.1 分阶段实施

| 阶段 | 内容 | 风险 | 验证方式 |
|------|------|------|-----------|
| **P1** | `index_snapshot.rs` + `context_budget.rs` | 低 | 纯函数单测 |
| **P2** | `error_classify.rs` | 低 | 纯函数单测 |
| **P3** | 扩展 runtime 返回 usage（依赖 confluent 库） | 中 | 集成测试 |
| **P4** | `session.rs` + 改造 `pipeline.rs`（usage 更新 + L2 检索） | 中 | 集成测试 |
| **P5** | 改造 `runner.rs`（分级错误处理 + 重试） | 中 | 端到端测试 |
| **P6** | 扩展 `recovery.rs` + 边界情况测试 | 低 | 启动测试 |

### 9.2 测试策略

**单元测试**：
- `index_snapshot::parse` 各种 index.md 格式
- `context_budget` 128K 边界、75% 计算
- `error_classify` 各类错误信息分类
- `session::update_usage` 真实占用跟踪
- `session::should_close` 各种预算场景

**集成测试**：
- 单篇构建：快照注入 + L2 检索查重
- 多篇构建：会话复用 + usage 跟踪
- 瞬态错误：429 重试 + 会话保留
- 结构性错误：Context Overflow 销毁会话
- 中断恢复：进程崩溃后从 checkpoint 继续

**端到端测试**：
- 128K 模型：单篇独立模式
- 256K 模型：3 篇会话复用
- 1M 模型：14 篇会话复用

### 9.3 验证指标

| 指标 | V1 基线 | V2 目标 |
|------|---------|---------|
| 重复条目率 | 基线 | 降低 50%+ |
| 多篇构建 token 成本 | 基线 | 降低 50%+（缓存命中） |
| Context Overflow 事故 | N/A | 0（基于真实 usage） |
| 瞬态错误恢复率 | 0（销毁会话） | 90%+（重试保留会话） |
| 中断恢复成功率 | 已有 | 100% |

---

## 十、风险与缓解

| 风险 | 影响 | 缓解 |
|------|------|------|
| runtime 不返回 usage | 无法跟踪真实占用 | P3 阶段扩展 confluent runtime 接口 |
| 缓存命中非保证 | OpenAI/DeepSeek 自动缓存依赖前缀，Anthropic 需显式 `cache_control` | Provider 能力探测，未命中不报错 |
| L2 检索增加延迟 | 每条多一次检索 | 单次 < 100ms，可忽略 |
| 2.5 倍预估系数偏差 | 下一篇预估不准 | 偏保守（宁可少复用，不可崩溃） |
| 会话内 Index 过期 | 会话跨越多篇时快照陈旧 | 每次从磁盘实时读取（已解决） |
| 任务隔离被打破 | 论文 A 错误污染 B、C | 结构性错误销毁会话；瞬态错误重试不影响已持久化 checkpoint |
| 取消语义复杂化 | 用户取消 B 时 A 的上下文处理 | 取消保留会话；checkpoint 隔离任务进度 |

---

## 十一、附录

### 11.1 模块依赖图

```
index_snapshot.rs ──┐
                    │
context_budget.rs ──┼──→ pipeline.rs ──→ runner.rs
                    │         ↑
session.rs ─────────┤         │
                    │         │
error_classify.rs ──┘         │
                              │
recovery.rs ──→ TaskQueueState
```

### 11.2 文件变更清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `knowledge_builder/index_snapshot.rs` | 新增 | Index 快照实时提取 |
| `knowledge_builder/context_budget.rs` | 新增 | 上下文预算计算 |
| `knowledge_builder/session.rs` | 新增 | 会话管理 + 真实 usage 跟踪 |
| `knowledge_builder/error_classify.rs` | 新增 | LLM 错误分类 + 重试策略 |
| `knowledge_builder/pipeline.rs` | 改造 | 注入快照 + L2 检索 + usage 更新 |
| `knowledge_builder/prompts.rs` | 改造 | 新增 candidates 渲染 |
| `knowledge_builder/mod.rs` | 改造 | 注册新模块 |
| `task_queue/runner.rs` | 改造 | 分级错误处理 + 重试 |
| `task_queue/recovery.rs` | 改造 | 启动时清理会话 |
| `task_queue/state.rs` | 改造 | 持有 SessionPool |
| `confluent` runtime | 扩展 | run() 返回值包含 usage |

### 11.3 不变项

- `knowledge_builder/commands.rs`：命令接口不变
- `knowledge_builder/events.rs`：事件类型不变
- `knowledge_builder/types.rs`：checkpoint 格式不变
- `knowledge_builder/config.rs`：配置不变
- 前端 `useKnowledgeBase.ts`：调用接口不变

---

## 十二、V2.1 修订说明

### 12.1 缺陷一修复：上下文预算低估

**问题**：V2.0 的 `record_paper` 仅累加输入 tokens，忽略输出（Planning JSON、条目正文、L2 候选注入），实际占用是估算的 2-3 倍，导致 Context Overflow。

**修复**：
- `ContextBudget` 移除 `current_paper_tokens` 字段（已包含在 `history_used` 中）
- `KnowledgeBuildSession::update_usage` 从 LLM 响应的真实 usage 更新 `history_used`
- 下一篇预估用 `paper_md × 2.5` 覆盖输出/检索/Planning 开销
- Pipeline 每次 `runtime.run()` 后调用 `update_usage`

**依赖**：需扩展 `confluent::ConfluentRuntime::run()` 返回 usage 信息（P3 阶段）。

### 12.2 缺陷二修复：异常处理破坏缓存

**问题**：V2.0 的"任何异常销毁会话"在面对瞬态错误（429/503/Timeout）时破坏昂贵的上下文缓存，违背"成本降低"目标。

**修复**：
- 新增 `error_classify.rs` 模块，错误分类为瞬态/结构性
- 瞬态错误：保留会话 + 指数退避重试（最多 3 次）
- 结构性错误：销毁会话（会话已不可用）
- 用户取消：保留会话（缓存可复用）

---

**审核要点**：
1. 真实 usage 跟踪方案是否可行（依赖 confluent runtime 扩展）
2. 2.5 倍预估系数是否合理
3. 瞬态错误重试 3 次是否足够
4. 用户取消保留会话是否符合直觉
5. 128K 硬门槛与 75% 软上限是否合理
6. 快照实时从磁盘读取的性能影响
7. L2 检索每条都做的必要性
