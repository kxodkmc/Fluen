# Fluen 知识库能力集成方案

> **版本**：v1.0
> **日期**：2026-08-02
> **状态**：待审批
> **范围**：为 Fluen 添加基于参考文献的 AI 知识库自动构建能力

---

## 一、概述

### 1.1 目标

为 Fluen 集成文献知识库能力，用户导入文献（PDF/图片）完成 OCR 后，可由 AI 严格按照规范自动创建知识库条目（综述页 + 概念页 + 实体页），支持任务串行队列与中断接续。

### 1.2 核心约束

- **轻量**：复用现有 `fluen-knowledge` / `confluent` / `llm_config` 基础设施，不引入重型依赖
- **高效**：AI 通过 `tool_calling` 直接调用知识库工具，避免自由文本解析
- **可维护**：模块化拆分，单文件 <800 行，配置驱动
- **可恢复**：任务持久化 + checkpoint + fluen-knowledge 天然幂等去重
- **可配置**：场景化模型槽位，用户可快捷选择 AI 模型与启用项

### 1.3 用户决策确认

| 决策点 | 选择 |
|--------|------|
| 任务队列作用范围 | **项目级串行**（项目内任务严格串行，跨项目隔离） |
| AI 提取粒度 | **Summary + Concept + Entity**（完整提取 + 自动建立 relations） |
| 任务队列持久化 | **JSON 文件**（`data/task-queue.json`，单项目隔离） |
| AI 模型配置 | **场景化槽位**（`LlmConfig.scene_models.knowledge_build`） |

---

## 二、架构设计

### 2.1 模块分层

```
┌──────────────────────────────────────────────────────────────┐
│ 前端 UI 层                                                    │
│  KnowledgeBuildPrompt.vue  KnowledgeConfigSection.vue        │
│  useKnowledgeBuilder.ts    usePersistedTasks.ts              │
│  （复用）useTaskQueue.ts   TaskQueueIndicator/Panel          │
├──────────────────────────────────────────────────────────────┤
│ Tauri Command 层                                              │
│  knowledge_builder/commands.rs  task_queue/commands.rs       │
├──────────────────────────────────────────────────────────────┤
│ 业务层                                                        │
│  knowledge_builder/pipeline.rs   task_queue/runner.rs        │
│  knowledge_builder/prompts.rs    task_queue/store.rs         │
│  knowledge_builder/config.rs     task_queue/recovery.rs      │
├──────────────────────────────────────────────────────────────┤
│ 基础设施层（已有，复用）                                       │
│  fluen-knowledge（async_kb + tools）  confluent（agent_runtime）│
│  llm_config   ai_services   references                       │
└──────────────────────────────────────────────────────────────┘
```

### 2.2 新增模块

```
src-tauri/src/
├── knowledge_builder/          # 知识库构建业务层
│   ├── mod.rs                  # 模块入口 + 公共类型
│   ├── pipeline.rs             # 构建流水线（ref-md → AI → wiki entries）
│   ├── prompts.rs              # prompt 模板与渲染
│   ├── config.rs               # KnowledgeBuildConfig（模型、prompt、启用项）
│   └── commands.rs             # Tauri commands
├── task_queue/                 # 持久化串行任务队列
│   ├── mod.rs
│   ├── store.rs                # JSON 持久化
│   ├── runner.rs               # 单 worker 串行执行器
│   ├── recovery.rs             # 启动时中断接续
│   └── commands.rs             # Tauri commands
└── references/commands.rs      # 修改：import_completed 后推送事件（不直接触发构建）

src/
├── composables/
│   ├── useKnowledgeBuilder.ts  # 构建任务管理 composable
│   └── usePersistedTasks.ts    # 持久化任务列表 composable
├── views/main/components/
│   └── knowledge/
│       ├── KnowledgeBuildPrompt.vue   # 导入完成后的询问弹窗
│       └── KnowledgeTaskList.vue      # 知识库任务列表（嵌入 taskqueue panel）
└── views/settings/sections/
    └── KnowledgeSection.vue    # 知识库配置分区
```

### 2.3 与现有系统的关系

| 现有系统 | 关系 |
|---------|------|
| `references` 模块 | 文献导入完成后推送 `EVENT_IMPORT_COMPLETED`，前端监听并询问。**不修改导入流程** |
| `useTaskQueue`（前端内存态） | **保持不变**，仅作为实时进度通知层。新增 `usePersistedTasks` 提供持久化任务列表 |
| `llm_config` | 扩展 `LlmConfig` 增加 `scene_models` 字段，不破坏现有 active_model |
| `fluen-knowledge` | src-tauri 新增依赖（features = ["async", "tools"]），首次使用时 `init_wiki` |
| `confluent` | 复用 `agent_runtime::ToolRegistry` + `llmkit` provider，AI 通过 tool_calling 调用 `KnowledgeToolProvider` |
| `motis_chat` | 独立，不耦合。知识库构建是后台任务，不走 motis 对话流 |

---

## 三、数据结构

### 3.1 任务队列（task_queue/store.rs）

```rust
/// 任务类型（便于未来扩展翻译、纠错等任务）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskKind {
    KnowledgeBuild {
        ref_id: String,
        model_ref: SceneModelRef,
        options: KnowledgeBuildOptions,
    },
    // 未来：Translation { ref_id, target_lang, ... }
    // 未来：OcrCorrection { ref_id, ... }
}

/// 场景化模型引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneModelRef {
    pub provider_id: String,
    pub model_id: String,
}

/// 知识库构建选项
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KnowledgeBuildOptions {
    pub create_summary: bool,    // default true
    pub create_concepts: bool,   // default true
    pub create_entities: bool,   // default true
    pub auto_relations: bool,    // default true，自动建立 summary ↔ concept/entity relations
    pub max_concepts: Option<usize>,    // 限制提取数量，避免 AI 失控
    pub max_entities: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// 任务记录（持久化到 task-queue.json）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,                      // task-{uuid}
    pub project_path: String,            // 项目绝对路径（多项目隔离）
    pub kind: TaskKind,
    pub status: TaskStatus,
    /// 断点续传信息（任务特定结构）
    pub checkpoint: serde_json::Value,
    pub error: Option<String>,
    pub created_at: String,              // RFC3339
    pub updated_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

/// 知识库构建的阶段（状态机）
///
/// 严格顺序：Planning → CreatingSummary → CreatingConcepts
///           → CreatingEntities → EstablishingRelations → Done
///
/// 中断恢复时根据 `stage` 判断从哪一步继续，避免跳过 relations
/// 产生孤儿条目。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BuildStage {
    #[default]
    Planning,               // 规划阶段：AI 阅读全文，产出结构化提取计划
    CreatingSummary,        // 创建 summary 条目
    CreatingConcepts,       // 创建 concept 条目（逐条）
    CreatingEntities,       // 创建 entity 条目（逐条）
    EstablishingRelations,  // 为 summary 建立 relations
    Done,
}

/// 知识库构建任务的 checkpoint（阶段化状态机）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KnowledgeBuildCheckpoint {
    /// 当前阶段（恢复入口）
    pub stage: BuildStage,
    /// 规划阶段产出的提取计划（Planning 完成后填充）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<ExtractionPlan>,
    /// summary 条目 ID（CreatingSummary 完成后填充）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_id: Option<String>,
    /// 已创建的 concept 条目 ID 列表
    #[serde(default)]
    pub concept_ids: Vec<String>,
    /// 已创建的 entity 条目 ID 列表
    #[serde(default)]
    pub entity_ids: Vec<String>,
    /// relations 是否已建立（EstablishingRelations 完成后置 true）
    #[serde(default)]
    pub relations_established: bool,
    /// AI 对话已消耗的 token 数（成本追踪）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_used: Option<u32>,
}

/// 规划阶段产出的提取计划（AI 结构化输出）
///
/// 在 Planning 阶段，AI 阅读全文后输出此计划，包含：
/// - summary 的要点（用于第二阶段创建 summary 条目）
/// - concept 候选列表（**已去重**：AI 必须先 query 已有知识库，
///   对已存在的概念标记 `existing_id`，新建的标记 `proposed`）
/// - entity 候选列表（同上）
///
/// 计划持久化到 checkpoint，执行阶段按计划逐条创建，
/// 中断后无需重新规划。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtractionPlan {
    pub summary_points: Vec<String>,
    pub concepts: Vec<PlannedEntry>,
    pub entities: Vec<PlannedEntry>,
}

/// 计划中的单个条目（concept 或 entity）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedEntry {
    pub title: String,
    /// 该条目在文献中的简述（用于 AI 第二阶段生成正文）
    pub brief: String,
    /// 去重决策：
    /// - Some(wiki_id)：知识库已存在相似条目，**不新建**，仅建立 relation
    /// - None：新建条目
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub existing_id: Option<String>,
    /// 若 existing_id 为 Some，AI 应合并的补充内容（追加到已存在条目）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_supplement: Option<String>,
}

/// 持久化文件结构
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskQueueFile {
    pub version: String,                 // "1.0.0"
    pub tasks: Vec<TaskRecord>,
}
```

### 3.2 LlmConfig 扩展（llm_config/model.rs）

```rust
/// 场景化模型配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneModels {
    /// 知识库构建专用模型
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge_build: Option<SceneModelRef>,
    // 未来扩展：
    // pub translation: Option<SceneModelRef>,
    // pub ocr_correction: Option<SceneModelRef>,
}

// 在 LlmConfig 中新增字段：
// #[serde(default, skip_serializing_if = "Option::is_none")]
// pub scene_models: Option<SceneModels>,
```

### 3.3 知识库构建配置（knowledge_builder/config.rs）

```rust
/// 知识库构建全局配置（应用级，非项目级）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeBuildConfig {
    /// 导入后的默认行为
    pub post_import_behavior: PostImportBehavior,
    /// 默认构建选项
    pub default_options: KnowledgeBuildOptions,
    /// 自定义 prompt 模板（None 则用内置默认）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_prompt: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PostImportBehavior {
    #[default]
    AlwaysAsk,      // 每次询问
    AlwaysBuild,    // 自动加入
    NeverBuild,     // 自动跳过
}
```

### 3.4 事件协议

```rust
// knowledge_builder/events.rs
pub const EVENT_KB_BUILD_STARTED: &str = "kb-build:started";
pub const EVENT_KB_BUILD_PROGRESS: &str = "kb-build:progress";
pub const EVENT_KB_BUILD_COMPLETED: &str = "kb-build:completed";
pub const EVENT_KB_BUILD_FAILED: &str = "kb-build:failed";
pub const EVENT_KB_BUILD_CANCELLED: &str = "kb-build:cancelled";

// 进度事件 payload
pub struct KbBuildProgressPayload {
    pub task_id: String,
    pub ref_id: String,
    pub stage: String,           // "reading_md" | "ai_extracting" | "creating_summary" | "creating_concepts" | ...
    pub created_count: usize,    // 已创建条目数
    pub total_planned: Option<usize>,  // 计划创建总数（AI 可能不确定）
    pub detail: Option<String>,  // 如 "正在创建概念页：数智化技术"
}
```

---

## 四、核心流程

### 4.1 触发流程

```
[1] 用户导入文献
[2] references/importer.rs 完成 OCR，生成 references/md/ref-{id}.md
[3] references/commands.rs 推送 EVENT_IMPORT_COMPLETED（已有逻辑，不改）
[4] 前端 useReferences 监听 → 调用 useKnowledgeBuilder.handleImportCompleted(entry)
[5] useKnowledgeBuilder 根据 PostImportBehavior 决定：
    ├─ AlwaysAsk → 弹出 KnowledgeBuildPrompt.vue
    ├─ AlwaysBuild → 直接调用 build_knowledge_from_ref command
    └─ NeverBuild → 忽略
[6] 用户确认 → 调用 task_queue_enqueue command
[7] 后端 task_queue/store.rs 持久化 TaskRecord（status=Pending）
[8] 后端 task_queue/runner.rs 串行执行
```

### 4.2 串行执行流程（task_queue/runner.rs）

```rust
/// 项目级串行执行器
pub struct TaskRunner {
    project_path: PathBuf,
    store: Arc<TaskStore>,
    /// 项目内串行：单 worker + mpsc channel
    tx: mpsc::Sender<TaskId>,
    /// 取消令牌集合（task_id → CancellationToken）
    cancel_tokens: Arc<DashMap<String, CancellationToken>>,
}

impl TaskRunner {
    /// 启动项目级 worker（每个项目一个）
    pub async fn start(&self) {
        let mut rx = self.store.subscribe();  // 监听新任务
        loop {
            // 取下一个 Pending 任务
            let task = self.store.next_pending(&self.project_path).await?;
            if task.is_none() { break; }
            
            let task_id = task.as_ref().unwrap().id.clone();
            let cancel_token = CancellationToken::new();
            self.cancel_tokens.insert(task_id.clone(), cancel_token.clone());
            
            // 标记为 Running
            self.store.update_status(&task_id, TaskStatus::Running).await?;
            
            // 执行任务
            let result = self.execute_task(task.unwrap(), &cancel_token).await;
            
            // 更新终态
            match result {
                Ok(()) => self.store.update_status(&task_id, TaskStatus::Completed).await?,
                Err(TaskError::Cancelled) => self.store.update_status(&task_id, TaskStatus::Cancelled).await?,
                Err(e) => self.store.update_status_with_error(&task_id, TaskStatus::Failed, &e.to_string()).await?,
            }
            
            self.cancel_tokens.remove(&task_id);
        }
    }
    
    async fn execute_task(&self, mut task: TaskRecord, cancel: &CancellationToken) -> Result<(), TaskError> {
        match &task.kind {
            TaskKind::KnowledgeBuild { ref_id, model_ref, options } => {
                // 委托给 knowledge_builder::pipeline
                let checkpoint: KnowledgeBuildCheckpoint = 
                    serde_json::from_value(task.checkpoint.clone()).unwrap_or_default();
                
                let new_checkpoint = knowledge_builder::pipeline::build(
                    &self.project_path,
                    ref_id,
                    model_ref,
                    options,
                    &checkpoint,
                    cancel,
                    |progress| {
                        // 推送进度事件
                        let _ = self.window.emit(EVENT_KB_BUILD_PROGRESS, &progress);
                    },
                ).await?;
                
                // 更新 checkpoint（即使被取消也保存）
                task.checkpoint = serde_json::to_value(&new_checkpoint)?;
                self.store.update_checkpoint(&task.id, &task.checkpoint).await?;
            }
        }
        Ok(())
    }
}
```

### 4.3 AI 构建流水线（两阶段，knowledge_builder/pipeline.rs）

**核心改进**：废弃"单次 agent.run 塞入全文 + 纯 tool_calling"的反模式，改为
**两阶段流水线**：

- **阶段 1（Planning）**：AI 阅读全文，**先 query 已有知识库做去重**，产出结构化
  `ExtractionPlan`（JSON）。计划持久化到 checkpoint。
- **阶段 2（Execution）**：按计划逐条创建条目，每条独立 LLM 调用，可中断恢复。
  最后单独执行 EstablishingRelations 阶段。

**这样解决三个问题**：
1. 跨文献去重：Planning 阶段强制 query，AI 决定"合并 existing_id"还是"新建"
2. 长文献稳定性：Planning 只产出计划（短输出），Execution 逐条短调用，避免长链路 tool-calling
3. 孤儿条目：Execution 按 `BuildStage` 状态机推进，relations 是独立阶段

```rust
pub async fn build(
    project_path: &Path,
    ref_id: &str,
    model_ref: &SceneModelRef,
    options: &KnowledgeBuildOptions,
    checkpoint: &KnowledgeBuildCheckpoint,
    cancel: &CancellationToken,
    on_progress: impl Fn(KbBuildProgressPayload),
) -> Result<KnowledgeBuildCheckpoint, KnowledgeError> {
    let refs_dir = project_path.join("references");
    let kb = AsyncKnowledgeBase::init(&refs_dir)?;
    let llm_client = build_llm_client(model_ref).await?;

    // 工具集：create + edit + query + query_batch + get_entry_full
    // 关键：暴露 query，让 AI 能检索已有条目做去重
    let provider = KnowledgeToolProvider::builder(kb.clone())
        .enable_tools(&[
            "knowledge_create_entry",
            "knowledge_edit_entry",
            "knowledge_query",
            "knowledge_query_batch",
            "knowledge_get_entry_full",
        ])
        .build();
    let registry = ToolRegistry::new();
    registry.register_provider(&provider).await;

    let mut ck = checkpoint.clone();

    // ── 阶段 1：Planning ──────────────────────────────────────────
    if ck.stage == BuildStage::Planning {
        on_progress(progress("planning", 0, None));
        let md_content = read_md(&refs_dir, ref_id).await?;

        // AI 阅读 + query 去重 + 产出 ExtractionPlan（JSON）
        let plan = run_planning(&llm_client, &registry, &md_content, options, cancel).await?;
        ck.plan = Some(plan);
        ck.stage = BuildStage::CreatingSummary;
        persist_checkpoint(&ck)?;  // 立即持久化，中断后无需重新规划
    }
    let plan = ck.plan.as_ref().ok_or(KnowledgeError::MissingPlan)?;

    // ── 阶段 2a：CreatingSummary ─────────────────────────────────
    if ck.stage == BuildStage::CreatingSummary {
        on_progress(progress("creating_summary", 0, None));
        let summary_id = run_create_summary(
            &llm_client, &registry, ref_id, &plan.summary_points, cancel
        ).await?;
        ck.summary_id = Some(summary_id);
        ck.stage = BuildStage::CreatingConcepts;
        persist_checkpoint(&ck)?;
    }

    // ── 阶段 2b：CreatingConcepts（逐条，跳过 existing_id）──────
    if ck.stage == BuildStage::CreatingConcepts {
        for (i, planned) in plan.concepts.iter().enumerate() {
            if cancel.is_cancelled() { return Err(KnowledgeError::Cancelled); }
            if planned.existing_id.is_some() { continue; }  // 已存在，跳过新建
            if i < ck.concept_ids.len() { continue; }       // 本任务已创建，中断恢复跳过

            on_progress(progress("creating_concepts", i, Some(plan.concepts.len())));
            let id = run_create_concept(&llm_client, &registry, planned, cancel).await?;
            ck.concept_ids.push(id);
            persist_checkpoint(&ck)?;  // 每条创建后立即持久化
        }
        ck.stage = BuildStage::CreatingEntities;
        persist_checkpoint(&ck)?;
    }

    // ── 阶段 2c：CreatingEntities（逐条，跳过 existing_id）──────
    if ck.stage == BuildStage::CreatingEntities {
        for (i, planned) in plan.entities.iter().enumerate() {
            if cancel.is_cancelled() { return Err(KnowledgeError::Cancelled); }
            if planned.existing_id.is_some() { continue; }
            if i < ck.entity_ids.len() { continue; }

            on_progress(progress("creating_entities", i, Some(plan.entities.len())));
            let id = run_create_entity(&llm_client, &registry, planned, cancel).await?;
            ck.entity_ids.push(id);
            persist_checkpoint(&ck)?;
        }
        ck.stage = BuildStage::EstablishingRelations;
        persist_checkpoint(&ck)?;
    }

    // ── 阶段 2d：EstablishingRelations（独立阶段，杜绝孤儿）─────
    if ck.stage == BuildStage::EstablishingRelations && !ck.relations_established {
        on_progress(progress("establishing_relations", 0, None));
        let summary_id = ck.summary_id.as_ref().ok_or(KnowledgeError::MissingSummary)?;
        let related_ids: Vec<_> = plan.concepts.iter()
            .filter_map(|c| c.existing_id.clone().or_else(|| {
                // 本任务新建的，从 concept_ids 按顺序取
                None
            }))
            .chain(plan.entities.iter().filter_map(|e| e.existing_id.clone()))
            .collect();
        // 收集本任务新建的 concept/entity id
        let mut all_related = related_ids;
        all_related.extend(ck.concept_ids.iter().cloned());
        all_related.extend(ck.entity_ids.iter().cloned());

        run_establish_relations(&llm_client, &registry, summary_id, &all_related, cancel).await?;
        ck.relations_established = true;
        ck.stage = BuildStage::Done;
        persist_checkpoint(&ck)?;
    }

    Ok(ck)
}
```

**Planning 阶段的去重逻辑**（关键）：

```rust
async fn run_planning(
    llm: &LlmClient,
    registry: &ToolRegistry,
    md_content: &str,
    options: &KnowledgeBuildOptions,
    cancel: &CancellationToken,
) -> Result<ExtractionPlan, KnowledgeError> {
    // 方式 A（默认）：让 AI 自主调用 knowledge_query 做去重
    //   prompt 强制要求：每个候选 concept/entity 必须先 query 检索，
    //   若返回高相似度条目（score > 0.7 或 title 高度匹配），
    //   则标记 existing_id，不再新建
    //
    // 方式 B（可选，更严格）：AI 仅产出候选 title 列表，
    //   后端用 knowledge_query_batch 批量检索，硬性去重后再回填 plan
    //   适合 AI 工具调用不稳定的场景

    let agent = LlmAgent::new(llm.clone(), registry.clone())
        .with_system_prompt(PLANNING_PROMPT)
        .with_cancel_token(cancel.clone());

    // AI 输出 JSON（structured output 或 tool_call 返回 plan）
    let result = agent.run(format!("文献内容：\n{md_content}")).await?;
    let plan: ExtractionPlan = parse_plan_from_result(&result)?;
    Ok(plan)
}
```

**长文献处理**：若 `md_content.len() > 50000`（约 1.2 万 token），Planning 阶段
先调用 `summarize_for_planning()` 做一次摘要预处理，再基于摘要产出计划，
避免单次上下文超限。摘要本身也可持久化到 checkpoint 供 Execution 复用。

### 4.4 中断接续机制（基于 BuildStage 状态机）

**核心改进**：废弃"扁平 created_wiki_ids + prompt 告知 AI 跳过"的脆弱方案，
改为 `BuildStage` 状态机驱动恢复，**relations 阶段独立可恢复**。

```
App 启动
  → 遍历所有项目的 task-queue.json
  → 所有 status=Running 的任务 → 重置为 Pending
  → 触发对应项目的 TaskRunner.start()
  → runner 拾取 Pending 任务，读取 checkpoint.stage
  → 从对应阶段继续执行（无需重新规划，无需依赖 AI "记住"进度）：

     stage=Planning              → 重新规划（最坏情况，plan 未持久化）
     stage=CreatingSummary       → 创建 summary
     stage=CreatingConcepts      → 按 plan.concepts 逐条创建，
                                    ck.concept_ids 已有的跳过
     stage=CreatingEntities      → 按 plan.entities 逐条创建，
                                    ck.entity_ids 已有的跳过
     stage=EstablishingRelations → 单独执行 relations（关键！）
                                    即使 concept/entity 全建完，
                                    若 relations_established=false 仍会执行
     stage=Done                  → 跳过
```

**杜绝孤儿条目的关键**：
- `EstablishingRelations` 是独立阶段，不依赖 AI "记得"要建关系
- 恢复时若 `stage < EstablishingRelations`，正常推进到该阶段
- 恢复时若 `stage == EstablishingRelations && !relations_established`，
  **必然执行** relations 建立
- 只有 `stage == Done` 才认为任务完成

**逐条持久化**：每创建一个条目、每完成一个阶段都立即 `persist_checkpoint`，
中断时已完成的条目不会丢失，恢复时从断点继续。

**四层幂等保障**：
1. **Plan 层**：Planning 阶段的去重决策（existing_id）持久化在 plan 中，
   恢复时新建条目不会与已存在条目重复
2. **Stage 层**：状态机保证每个阶段只执行一次，relations 不会被跳过
3. **去重层**：fluen-knowledge 的 summary 按 source、concept/entity 按 title+type 去重
   （兜底，应对 plan 丢失）
4. **MD 文件层**：wiki 条目 MD 文件原子写入，不会半成品

### 4.5 取消机制

```
用户点击"取消任务"
  → 前端调用 cancel_task(task_id) command
  → 后端从 cancel_tokens 取出对应 CancellationToken，调用 cancel()
  → runner 检测 cancel，停止 AI 调用（LLM 请求 abort）
  → 任务标记为 Cancelled，checkpoint 保留
  → 用户可选择"继续"（重新入队，从 checkpoint 恢复）或"删除"
```

---

## 五、Tauri Command API

### 5.1 任务队列 commands（task_queue/commands.rs）

```rust
/// 入队任务
#[tauri::command]
pub async fn task_queue_enqueue(
    project_path: String,
    kind: TaskKind,
    state: State<'_, TaskQueueState>,
) -> Result<TaskRecord, TaskQueueError>;

/// 查询项目下所有任务（含历史）
#[tauri::command]
pub async fn task_queue_list(
    project_path: String,
    status_filter: Option<Vec<TaskStatus>>,
    state: State<'_, TaskQueueState>,
) -> Result<Vec<TaskRecord>, TaskQueueError>;

/// 取消任务
#[tauri::command]
pub async fn task_queue_cancel(
    task_id: String,
    state: State<'_, TaskQueueState>,
) -> Result<(), TaskQueueError>;

/// 重试失败任务（清除 error，重置为 Pending）
#[tauri::command]
pub async fn task_queue_retry(
    task_id: String,
    state: State<'_, TaskQueueState>,
) -> Result<(), TaskQueueError>;

/// 继续已取消任务（保留 checkpoint，重置为 Pending）
#[tauri::command]
pub async fn task_queue_resume(
    task_id: String,
    state: State<'_, TaskQueueState>,
) -> Result<(), TaskQueueError>;

/// 删除任务记录
#[tauri::command]
pub async fn task_queue_delete(
    task_id: String,
    state: State<'_, TaskQueueState>,
) -> Result<(), TaskQueueError>;

/// 清除项目下所有已完成/失败/取消的任务
#[tauri::command]
pub async fn task_queue_clear_finished(
    project_path: String,
    state: State<'_, TaskQueueState>,
) -> Result<usize, TaskQueueError>;
```

### 5.2 知识库构建 commands（knowledge_builder/commands.rs）

```rust
/// 便捷封装：入队知识库构建任务
#[tauri::command]
pub async fn build_knowledge_from_ref(
    project_path: String,
    ref_id: String,
    model_ref: Option<SceneModelRef>,  // None 用 scene_models.knowledge_build 默认
    options: Option<KnowledgeBuildOptions>,
    state: State<'_, TaskQueueState>,
    llm_storage: State<'_, LlmConfigStorage>,
) -> Result<TaskRecord, KnowledgeBuilderError> {
    // 解析 model_ref（None 则读 scene_models.knowledge_build）
    // 解析 options（None 则读 default_options）
    // 调用 task_queue_enqueue
}

/// 查询文献是否已加入知识库（通过 source 字段反查）
#[tauri::command]
pub async fn check_ref_in_knowledge(
    project_path: String,
    ref_id: String,
) -> Result<Option<WikiEntryDetail>, KnowledgeBuilderError>;
```

### 5.3 配置 commands

```rust
/// 读取知识库构建配置
#[tauri::command]
pub async fn get_knowledge_build_config(
    storage: State<'_, AppConfigStorage>,
) -> Result<KnowledgeBuildConfig, ConfigError>;

/// 更新知识库构建配置
#[tauri::command]
pub async fn set_knowledge_build_config(
    config: KnowledgeBuildConfig,
    storage: State<'_, AppConfigStorage>,
) -> Result<(), ConfigError>;
```

---

## 六、前端设计

### 6.1 KnowledgeBuildPrompt.vue（询问弹窗）

```
┌──────────────────────────────────────────────┐
│  📚 加入知识库                                │
│                                                │
│  文献「{filename}」已完成 OCR                  │
│  是否由 AI 自动提取并加入知识库？              │
│                                                │
│  模型：[DeepSeek Chat ▼]                       │
│  提取：☑ 综述页  ☑ 概念页  ☑ 实体页          │
│        ☑ 自动建立关联                          │
│                                                │
│  [本次不加入]  [本次加入]  [始终加入]  [设置] │
└──────────────────────────────────────────────┘
```

批量导入时：弹窗显示「N 篇文献待处理」，提供「全部加入 / 全部不加入 / 逐个询问」选项。

### 6.2 KnowledgeSection.vue（设置页分区）

```
设置 → 知识库
├── 默认行为：[每次询问 ▼]
├── 构建模型：[DeepSeek ▼] → [deepseek-chat ▼]
├── 默认提取项：☑ 综述  ☑ 概念  ☑ 实体  ☑ 自动关联
├── 数量限制：概念 [10] 实体 [10]（避免 AI 失控）
└── 自定义 Prompt：[展开编辑器]（高级用户）
```

### 6.3 useKnowledgeBuilder.ts

```typescript
export function useKnowledgeBuilder() {
  const { post_import_behavior, default_options } = useKnowledgeConfig();
  const { register: registerTask, update: updateTask } = useTaskQueue();
  
  /// 处理导入完成事件
  async function handleImportCompleted(entry: ReferenceEntry) {
    if (post_import_behavior.value === 'never_build') return;
    if (post_import_behavior.value === 'always_build') {
      return enqueueBuild(entry);
    }
    // always_ask → 弹窗
    showBuildPrompt(entry);
  }
  
  /// 入队构建任务
  async function enqueueBuild(entry: ReferenceEntry, options?: Partial<KnowledgeBuildOptions>) {
    const task = await invoke('build_knowledge_from_ref', {
      project_path: currentProject.value.path,
      ref_id: entry.id,
      options: { ...default_options.value, ...options },
    });
    
    // 注册到前端 taskqueue 通知系统
    registerTask(`kb.build.${task.id}`, {
      title: `构建知识库：${entry.title}`,
      status: 'pending',
      category: 'knowledge',
      progress: { ratio: 0 },
    });
  }
  
  /// 监听构建进度事件
  function setupListeners() {
    listen('kb-build:progress', (e) => {
      updateTask(`kb.build.${e.payload.task_id}`, {
        progress: { ratio: e.payload.created_count / (e.payload.total_planned ?? 1) },
        detail: e.payload.detail,
      });
    });
    // ... started/completed/failed/cancelled
  }
  
  return { handleImportCompleted, enqueueBuild, setupListeners };
}
```

### 6.4 usePersistedTasks.ts

```typescript
/// 持久化任务列表（与 useTaskQueue 协同）
export function usePersistedTasks() {
  const tasks = ref<TaskRecord[]>([]);
  
  async function refresh() {
    tasks.value = await invoke('task_queue_list', {
      project_path: currentProject.value.path,
    });
  }
  
  async function cancel(taskId: string) {
    await invoke('task_queue_cancel', { task_id: taskId });
    refresh();
  }
  
  async function retry(taskId: string) {
    await invoke('task_queue_retry', { task_id: taskId });
    refresh();
  }
  
  async function resume(taskId: string) {
    await invoke('task_queue_resume', { task_id: taskId });
    refresh();
  }
  
  return { tasks, refresh, cancel, retry, resume };
}
```

### 6.5 任务面板扩展

`TaskQueuePanel.vue` 增加持久化任务区：
- 顶部：活跃任务（来自 `useTaskQueue`，实时进度）
- 底部：历史/中断任务（来自 `usePersistedTasks`，含重试/继续按钮）

---

## 七、Prompt 设计（两阶段，knowledge_builder/prompts.rs）

**核心改进**：废弃"单一 prompt + 禁止文本输出"的脆弱方案，改为
**两阶段 prompt**，并**暴露 query 工具**让 AI 主动去重。

### 7.1 工具集（5 个，含检索）

| 工具 | 阶段 | 用途 |
|------|------|------|
| `knowledge_query` | Planning | 检索已有条目，做跨文献去重 |
| `knowledge_query_batch` | Planning | 批量检索候选概念 |
| `knowledge_create_entry` | Execution | 创建 summary/concept/entity |
| `knowledge_edit_entry` | Execution | 合并补充内容到已存在条目 |
| `knowledge_get_entry_full` | Execution | 读取已存在条目正文，用于合并 |

**关键**：`knowledge_query` 必须暴露给 AI，否则跨文献概念碎片化无法解决。

### 7.2 阶段 1：Planning Prompt

```rust
pub const PLANNING_PROMPT: &str = r#"
你是学术文献知识库的规划助手。阅读文献，产出结构化提取计划。

## 任务

1. 阅读文献全文，识别：
   - 核心研究问题与方法（用于 summary 的要点）
   - 关键学术概念（concept 候选，最多 {max_concepts} 个）
   - 重要人物/机构/项目（entity 候选，最多 {max_entities} 个）

2. **跨文献去重（强制）**：
   对每个 concept/entity 候选，必须先调用 `knowledge_query` 检索已有知识库。
   - 若返回结果中存在高度相似条目（title 语义匹配，或 score > 0.7）：
     标记 `existing_id` 为该条目 wiki_id，不新建
     若文献对该概念有新补充，在 `merge_supplement` 中说明
   - 若无相似条目：`existing_id` 留空，将新建

3. **输出格式**：调用 `submit_plan` 工具提交结构化计划，包含：
   - summary_points: Vec<String>（3-5 个要点）
   - concepts: Vec<PlannedEntry>（每个含 title/brief/existing_id/merge_supplement）
   - entities: Vec<PlannedEntry>（同上）

## 重要说明

- 你可以输出自由文本（思考过程），但最终必须调用 `submit_plan` 提交计划
- **不要在此阶段创建任何条目**（不调用 create_entry）
- 去重判断要严格：宁可合并到已有条目，也不要新建近似条目
  例如"机器学习"/"机器学习技术"/"ML"应合并到同一个 concept

## 文献内容

{md_content}
"#;
```

### 7.3 阶段 2：Execution Prompts（按条目类型分别设计）

每个 Execution 子阶段使用独立 prompt，避免单次长链路 tool-calling：

```rust
/// 创建 summary 条目
pub const CREATE_SUMMARY_PROMPT: &str = r#"
为以下文献创建综述页（summary）条目。

要点：
{summary_points}

要求：
- 调用 `knowledge_create_entry`，type=summary
- source 字段必须为 `raw/{ref_id}.pdf`
- title 用文献标题（若已知）或"文献综述-{ref_id}"
- content 写 200-400 字综述，涵盖研究问题、方法、结论
- 可输出自由文本，但必须以 create_entry 调用结束
"#;

/// 创建单个 concept 条目
pub const CREATE_CONCEPT_PROMPT: &str = r#"
为以下概念创建知识库条目。

概念标题：{title}
文献中的简述：{brief}

要求：
- 调用 `knowledge_create_entry`，type=concept
- title 用概念全称（如"数智化技术"而非"数智化"）
- content 包含：概念定义 + 该文献中的具体应用/贡献
- 可输出自由文本，但必须以 create_entry 调用结束
"#;

/// 创建单个 entity 条目（事实性，鼓励谨慎）
pub const CREATE_ENTITY_PROMPT: &str = r#"
为以下实体创建知识库条目。

实体标题：{title}
文献中的简述：{brief}

要求：
- 调用 `knowledge_create_entry`，type=entity
- title 用人物全名或机构全名
- content 包含：身份/性质 + 与该文献的关系/贡献
- **仅基于文献明确陈述的内容**，不得推断或幻觉
  若文献信息不足，在 content 中注明"待补充"
- 可输出自由文本，但必须以 create_entry 调用结束
"#;

/// 建立 relations（独立阶段，杜绝孤儿条目）
pub const ESTABLISH_RELATIONS_PROMPT: &str = r#"
为综述页建立关联关系。

summary 条目 ID：{summary_id}
需关联的条目 ID 列表：{related_ids}

要求：
- 调用 `knowledge_edit_entry`，对 summary 条目添加 relations
- relations 包含所有 related_ids
- 这一步是必须的，不得跳过
"#;
```

### 7.4 关于"自由文本输出"的放宽

**原方案的缺陷**：要求"不得输出任何自由文本"，但当前主流 LLM 的 tool-calling
机制无法严格保证。强行要求反而可能导致模型行为异常。

**修订策略**：
- **允许 AI 输出自由文本**（思考过程、解释说明）
- **以 tool_calling 为执行凭证**：只要调用了正确的工具，文本输出不影响正确性
- **后端解析以 tool_call 结果为准**，不依赖文本内容
- **structured output 优先**：Planning 阶段若模型支持 JSON mode 或
  structured output，优先使用，提升计划解析可靠性

### 7.5 长文献的分块策略

```
md_content.len() <= 50000  → 直接传入 Planning
md_content.len() > 50000   → 先调用 summarize_for_planning() 摘要
                              摘要持久化到 checkpoint，复用于 Execution
```

`summarize_for_planning()` 是一次独立 LLM 调用，prompt 要求产出
"保留核心概念、方法、结论的结构化摘要"，丢弃细节论述，
将长文献压缩到 Planning 可处理的规模。

---

## 八、集成点与依赖

### 8.1 Cargo.toml 变更

```toml
# src-tauri/Cargo.toml
[dependencies]
# 新增
fluen-knowledge = { path = "../crates/fluen-knowledge", features = ["async", "tools"] }
# 已有，确认 features
confluent = { path = "../crates/confluent", features = ["agent-runtime", "llmkit"] }
```

### 8.2 lib.rs 注册

```rust
// src-tauri/src/lib.rs
mod knowledge_builder;
mod task_queue;

// tauri::Builder
.invoke_handler(tauri::generate_handler![
    // 现有 commands...
    // 新增 task_queue commands
    task_queue::commands::task_queue_enqueue,
    task_queue::commands::task_queue_list,
    task_queue::commands::task_queue_cancel,
    task_queue::commands::task_queue_retry,
    task_queue::commands::task_queue_resume,
    task_queue::commands::task_queue_delete,
    task_queue::commands::task_queue_clear_finished,
    // 新增 knowledge_builder commands
    knowledge_builder::commands::build_knowledge_from_ref,
    knowledge_builder::commands::check_ref_in_knowledge,
    knowledge_builder::commands::get_knowledge_build_config,
    knowledge_builder::commands::set_knowledge_build_config,
])
.manage(task_queue::TaskQueueState::new())
.setup(|app| {
    // 启动时执行中断接续
    task_queue::recovery::recover_on_startup(app.handle())?;
    Ok(())
})
```

### 8.3 文件存储位置

| 文件 | 路径 | 说明 |
|------|------|------|
| 任务队列 | `{project}/data/task-queue.json` | 项目级，单项目隔离 |
| 知识库 | `{project}/references/wiki/` | fluen-knowledge 标准 |
| 构建配置 | 应用配置目录 `app_config.json` 内 | 全局，非项目级 |

---

## 九、实施计划

### 阶段 P1：基础设施（核心）

| 任务 | 模块 | 说明 |
|------|------|------|
| 1. task_queue 模块 | task_queue/ | store.rs + runner.rs + recovery.rs + commands.rs |
| 2. knowledge_builder pipeline | knowledge_builder/ | pipeline.rs + prompts.rs，单文献 → summary |
| 3. fluen-knowledge 集成 | src-tauri/Cargo.toml + lib.rs | 依赖、init_wiki、ToolRegistry 注入 |
| 4. LlmConfig 扩展 | llm_config/model.rs | scene_models 字段 |

### 阶段 P2：前端与配置

| 任务 | 模块 | 说明 |
|------|------|------|
| 5. 询问弹窗 | KnowledgeBuildPrompt.vue | 单文献 + 批量询问 |
| 6. 设置分区 | KnowledgeSection.vue | 模型选择、默认行为、prompt |
| 7. composable | useKnowledgeBuilder.ts + usePersistedTasks.ts | 事件监听 + 任务管理 |
| 8. 任务面板扩展 | TaskQueuePanel.vue | 持久化任务列表区 |

### 阶段 P3：完整提取

| 任务 | 模块 | 说明 |
|------|------|------|
| 9. concept/entity 提取 | knowledge_builder/prompts.rs | prompt 完整化 |
| 10. 自动建立 relations | pipeline.rs | edit_entry 调用 |
| 11. checkpoint 细化 | task_queue/ | 条目级断点 |
| 12. 数量限制 | prompts.rs + config | max_concepts/max_entities |

### 阶段 P4：优化与扩展

| 任务 | 说明 |
|------|------|
| 13. embedding 支持 | enable_embeddings=true，语义检索 |
| 14. 成本追踪 | tokens_used 统计与展示 |
| 15. 批量 UX 优化 | 批量导入时的批量询问 |
| 16. 错误恢复增强 | AI 调用失败的重试策略（指数退避） |

---

## 十、风险与缓解

### 10.1 内容质量风险（核心）

| 风险 | 缓解 |
|------|------|
| **跨文献概念碎片化**（"机器学习"/"机器学习技术"/"ML"反复创建） | Planning 阶段强制 `knowledge_query` 检索去重；AI 决策 `existing_id` 合并；prompt 明确"宁可合并不要新建"；可选方式 B 后端批量检索硬性去重 |
| **entity 事实性幻觉**（人物/机构归属错误被固化） | entity prompt 强制"仅基于文献明确陈述，不得推断"；信息不足时注明"待补充"；UI 突出展示新建 entity 供用户复核；支持后续 edit_entry 修正 |
| **summary 综述偏差**（AI 误解文献核心观点） | Planning 阶段先产出 summary_points 供后端校验；可选"严格模式"要求用户确认 plan 后再 Execution；summary 创建后可在 UI 编辑 |
| **concept 内容浅薄**（仅复制文献原话，无知识沉淀） | prompt 要求"概念定义 + 该文献中的具体应用/贡献"双层结构；后端可校验 content 长度与结构 |
| **孤儿条目**（concept/entity 已建但未被 summary 引用） | `BuildStage::EstablishingRelations` 独立阶段 + `relations_established` 标志位；恢复时若未建立则必然执行（见 4.4） |
| **去重误判**（AI 把不同概念误合并） | query 阈值 score > 0.7 较保守；merge_supplement 保留文献特有内容；用户可在 UI 拆分误合并条目 |

### 10.2 机制性风险

| 风险 | 缓解 |
|------|------|
| AI 不调用工具，仅输出文本 | 允许文本输出，但后端以 tool_call 为执行凭证；若 Planning 无 submit_plan 调用则任务 Failed，可重试；structured output 优先 |
| AI 创建过多 concept/entity 失控 | max_concepts/max_entities 限制 + prompt 明确"仅核心"；Planning 阶段产出 plan，后端可校验数量 |
| AI 调用失败（网络/限流） | 任务标记 Failed，保留 checkpoint，支持 retry；可加指数退避重试 |
| 长文献超上下文 | 4.3/7.5 的分块策略：>50000 字先摘要预处理；Execution 逐条短调用 |
| 任务队列文件损坏 | 原子写入 + 启动时 JSON 校验，损坏则备份后重置 |
| 跨项目任务互相影响 | 项目级隔离，每个项目独立 worker + 独立 task-queue.json |
| scene_models 引用失效 | 启动时校验 scene_models 引用是否存在，失效则回退到 active_model 并 warn |
| 中断时 wiki 条目半成品 | fluen-knowledge 的 atomic_write 保证 MD 完整；DB 事务保证原子性 |
| Planning 与 Execution 间模型切换 | model_ref 持久化在 TaskRecord，任务级锁定，不随配置变化 |

---

## 十一、测试策略

| 层级 | 测试内容 |
|------|---------|
| 单元测试 | task_queue store 读写、checkpoint 序列化、prompt 渲染、配置校验 |
| 集成测试 | pipeline 端到端（mock LLM）、中断接续（kill 后重启）、取消恢复 |
| fluen-knowledge | 复用其现有测试套件（wiki_integration.rs） |
| 前端 | 询问弹窗交互、任务列表展示、事件监听 |

---

## 十二、验收标准

### 12.1 功能验收

1. ✅ 用户导入文献后弹出"加入知识库"询问
2. ✅ 选择模型后，AI 自动创建 summary + concept + entity 条目
3. ✅ **条目自动建立 relations**：每个 summary 必须有指向其 concept/entity 的
   relations，可在 index.md 中查看（杜绝孤儿条目）
4. ✅ 批量导入时支持"全部加入"
5. ✅ 任务在状态栏 taskqueue 实时显示进度（区分 Planning/Creating/Relations 阶段）
6. ✅ App 异常退出后重启，未完成任务自动恢复继续
7. ✅ 用户可取消任务，保留 checkpoint 后可"继续"
8. ✅ 设置页可配置模型、默认行为、提取选项

### 12.2 内容质量验收（核心）

9. ✅ **跨文献去重**：连续导入 2 篇均提及"机器学习"的文献，知识库中
   `concept-机器学习` 条目仅有 1 个，第 2 篇的 summary 通过 relations
   指向该已有条目，而非新建近似条目
10. ✅ **无孤儿条目**：任意时刻中断任务后恢复，最终知识库中所有 concept/entity
    条目都被至少一个 summary 通过 relations 引用（可通过遍历 index.md 校验）
11. ✅ **entity 事实可追溯**：entity 条目 content 仅含文献明确陈述的信息，
    信息不足处标注"待补充"，不出现幻觉内容
12. ✅ **长文献稳定**：导入 100+ 页论文（MD > 50000 字），任务能正常完成，
    不因上下文超限失败

### 12.3 工程质量验收

13. ✅ 单文件 <800 行，模块边界清晰
14. ✅ cargo test + vue-tsc + vitest 全部通过
15. ✅ BuildStage 状态机有完整单元测试（覆盖每个阶段的中断恢复）
16. ✅ Planning 去重逻辑有集成测试（mock LLM + 预置已有知识库）

---

**审批通过后，按 P1 → P2 → P3 → P4 顺序实施。**
