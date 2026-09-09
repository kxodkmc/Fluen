# 知识库迁移计划：fluen-knowledge → fluen-kb

> 目标：将宿主（src-tauri）从旧知识库 `fluen-knowledge` 全面切换到新 SDK `fluen-kb`。
> 已定决策：**不实现 referee-ai 工具适配层，Agent 面统一走 MCP**（fluen-kb 内置 rmcp server）。
> 前置设计：`crates/fluen-kb/KNOWLEDGE_BASE_REDESIGN.md`（行内溯源 `<ref-xxx>`、类型化关系、MD 单一真相源）。

## 0. 现状盘点（已核实的依赖面）

| 宿主文件 | 使用方式 | 迁移动作 |
|---|---|---|
| `knowledge_builder/commands.rs` | `wiki::init_wiki`（knowledge_init 命令）、`AsyncKnowledgeBase::open` + `MetaResult`（meta 查询） | 改为 `KbBuilder::open` / `knowledge_meta` 语义 |
| `knowledge_builder/pipeline.rs` | 五阶段管线；AI 经工具写条目；宿主直调 `kb.edit_entry`（L338）、`l2_query`（L689，hybrid top-3） | 工具面切 MCP；直调改 `AsyncKb` |
| `knowledge_builder/llm_helper.rs` | `KnowledgeToolProvider` 注册 `knowledge_create_entry`/`knowledge_edit_entry` 进 `ToolRegistry`（L420） | **删除**，替换为 MCP 客户端桥接（见 M2） |
| `task_queue/runner.rs` | `AsyncKnowledgeBase::init` + embedding router 注入（L362-365） | `KbBuilder` + attach 适配器 |
| `builtin_providers/embedding.rs` | 为旧 `KnowledgeEmbedding`（async 批量）实现 router 适配 | 为新 trait（同步单文本）重写适配器 |
| `agent_tools/literature.rs` | `LiteratureSearchTool`（referee-ai Tool）直调 `kb.query` hybrid top-4 | 改走 MCP `knowledge_query` |
| `agent_tools/assemble.rs` | 打开 `AsyncKnowledgeBase::init`（L191）读条目 | `Kb` 句柄直读 |

## 1. 能力对照与缺口

新库已覆盖旧库全部核心能力（init/open、create 去重分流、edit 三原语、query 三模式、delete+引用清理、rebuild、embedding 注入），另有旧库没有的：`upsert_from_source`、`delete_source`、`lint`/`prune`、行内溯源、原子写。

需要处理的差异：

1. **无 `tools` feature** → 按决策不实现；Agent 访问统一走 MCP server（`fluen_kb::mcp`，10 个工具，含 `knowledge_query` / `knowledge_query_batch` / `knowledge_create_entry` / `knowledge_edit_entry`）。
2. **Embedding trait 形状不同**：旧 = `async embed(Vec<String>) -> Vec<Vec<f32>>`；新 = `sync embed(&str) -> KbResult<Vec<f32>>`。宿主 `embedding.rs` 写一层薄适配即可。
3. **index.db schema 不兼容**：旧库 v1 / 新库 v2，新库遇旧版本直接拒绝。迁移策略：**删除 index.db → rebuild**（MD 是唯一真相源，派生缓存可随时重建，符合不变式）。
4. **tags 移除**：`WikiTag`、`tag_titles` 不复存在；新库 `migrate()` 自动删除 frontmatter `tags` 键。前端需同步下线 tag 展示。
5. **source 格式变化**：旧存 `raw/ref-xxx.pdf` 路径；新统一 `ref-xxx`（`migrate()` 内置 `normalize_source_value` 转换）。前端 `useKnowledgeBase.loadExistingStatus` 的匹配逻辑需跟随。
6. **关系带谓词**：`Vec<String>` → `(Predicate, WikiId)`（如 `作者 :: [[wiki-xxx]]`），前端关联展示需渲染谓词。
7. **正文含 `<ref-xxx>` 标签**：任何把 body 直接给前端预览/给模型的路径，先经 `fluen_kb::syntax::strip` 剥离。
8. **`retrieval_method_used` 无对应物**：新 `SearchHit` 不回传实际命中的检索方式（ID 直查/FTS/语义）。如前端或 agent 输出依赖此字段，在宿主层补充或裁剪。

## 1.5 正文语法契约（新旧对照）

正文的书写语法本身发生变化，这不只是存储迁移问题，也直接约束 M2 管线的 AI 产出要求：

| 语法点 | 旧库 | 新库 |
|---|---|---|
| 行内溯源 | 无，纯 Markdown | `<ref-16hex>…</ref-16hex>` 包裹，可嵌套；内容归属 = 开放标签栈中所有 SourceId 的并集 |
| 容错语义 | 无 | T1 未闭合块尾自动闭合；T2 孤立闭标签忽略；T3 畸形按字面文本；T4 围栏/行内代码豁免；T5 跨界嵌套规范化；T6 开标签属性接受并忽略；T7 空标签清除 |
| 关联区行 | `- [[wiki-xxx]]` | `- [谓词 ::] [[dir/wiki-xxx-标题]]`，`related` 谓词省略 |
| 正文内链 | `[[wiki-xxx]]` | 格式兼容仍有效；标题提示受 lint 校验（与当前标题不符警告），悬空由 `prune()` 摘除链接 token、保留句子 |
| frontmatter | tags / authors / source 路径 | 全部移除，信息迁移至行内标签与关联行 |

对管线的约束：

- **AI 产出必须带溯源**：prompt 模板要求 summary 全文单源包裹（写时强制校验，违反报错）；concept/entity 用 `<ref-xxx>` 标注来源，允许未溯源片段（lint 警告不阻塞）
- **存量迁移只覆盖一半**：`migrate()` 可将旧 summary 经 frontmatter source 自动包裹；旧 concept/entity 正文无来源信息，**无法自动补溯源**，保持未溯源状态并在 lint 中警告。是否对存量概念/实体重跑构建（补溯源）→ 待决策
- **normalize 落库**：AI 产出的 T1/T5 违例正文，写入前过 `syntax::normalize` 落地为显式闭合的规范形态，保证编辑器预览、导出、二次编辑拿到的都是干净文本

## 2. 迁移阶段

### M0 — 接入与验证（无宿主改动）

- [ ] 根 `Cargo.toml` workspace members 加入 `crates/fluen-kb`
- [ ] `cargo build -p fluen-kb --all-features`、`cargo test -p fluen-kb` 全绿（当前 75 测试）
- [ ] 确认 `crates/fluen-kb` 与 `D:\Dev\fluen-kb` 双副本策略：**项目内一份为接入源，dev 目录停止手改**

### M1 — 基础层替换（宿主直调部分，与 Agent 无关）

- [ ] `builtin_providers/embedding.rs`：为 `fluen_kb::KnowledgeEmbedding`（sync 单文本）实现 `EmbeddingRouter` 适配器；批 → 单调用循环摊平
- [ ] `task_queue/runner.rs`：`AsyncKnowledgeBase::init` → `KbBuilder::new(refs).open()`，`into_async()`，`attach_embedding(router)`
- [ ] `commands.rs`：`knowledge_init` → `KbBuilder::open`（幂等，目录已存在不报错）；meta 查询 → `Searcher::list_entries` 聚合或 MCP `knowledge_meta` 同语义
- [ ] `pipeline.rs`：`kb.edit_entry` → `AsyncKb.edit`（EditOp 映射：`search_replace`/`insert_after` 直映，`replace_source` 为新增能力可暂不用）；`l2_query` → `AsyncKb.query`（Hybrid，top 3，`expand: 0`）
- [ ] `assemble.rs`：打开句柄换 `Kb`，`get_entry` 直读（注意 strip 正文标签）
- [ ] 此阶段旧库仍在编译，双轨并存，行为不回退

### M2 — Agent 面切换到 MCP（核心阶段）

不移植 `KnowledgeToolProvider`。方案：**进程内 MCP 直连**——宿主同时拉起 `fluen_kb::mcp::KbServer`（rmcp 支持自定义传输，用内存传输对直连；若 rmcp 版本内存传输不可用，退化为 stdio 子进程 + WAL 并发打开同一库，`busy_timeout` 已设 5s）。

- [ ] 宿主新增 `src-tauri/src/knowledge_mcp_bridge.rs`：
  - 一侧 serve `KbServer`（挂 `Kb` 句柄，含 embedding）
  - 另一侧持 rmcp Client，把 `knowledge_create_entry` / `knowledge_edit_entry` / `knowledge_query` 包装成 referee-ai `Tool` 注册进 `ToolRegistry`
  - 桥接层**只做转发**，工具 schema 直接复用 `mcp::tools::definitions()`，避免双份定义漂移
- [ ] `llm_helper.rs`：删除 `KnowledgeToolProvider` 路径，改注册桥接工具；`EntryCaptureGuard` 捕获逻辑不变（工具名相同）
- [ ] `literature.rs`：`LiteratureSearchTool` 内部改调 MCP `knowledge_query`（对外参数不变）；其单测中 `wiki::CreateEntryParams` 造数据改为 `Kb.ops().create()`
- [ ] **重写 Planning / 创建阶段的 prompt 模板**（见 §1.5）：要求 AI 产出带 `<ref-xxx>` 行内溯源（summary 全文单源包裹；concept/entity 标注来源、允许未溯源片段）；建条目前先 `knowledge_query` 查重（两阶段管线规则不变）
- [ ] 写入前经 `syntax::normalize` 规范化 AI 产出的正文（T1/T5 落地为显式闭合）
- [ ] 五阶段管线（Planning → CreatingSummary → CreatingConcepts → CreatingEntities → EstablishingRelations）阶段语义与 `kb-build:*` 事件**保持不变**，仅工具底层与产出语法换血

### M3 — 存量数据迁移（一次性，按项目执行）

写入时机：宿主首次用新库打开旧项目时自动执行，迁移前弹窗确认。

- [ ] 检测：`wiki/index.db` 存在且 `PRAGMA user_version` ≠ 2（或 frontmatter 含 `tags`/`authors`/`source` 键）
- [ ] 备份 `references/wiki/` → `references/wiki.bak-{日期}/`
- [ ] 删除 `index.db`（派生缓存，安全）
- [ ] `KbBuilder::open` + `ops.migrate()`：summary body 无 `<ref->` 时用 frontmatter source 包裹；`authors` → 关联行（`作者 :: [[wiki-xxx]]`）；删除 `tags` 键；`raw/ref-xxx.pdf` → `ref-xxx`
- [ ] `IndexHandle::rebuild()` 重建索引；`ops.lint()` 输出迁移后体检报告，Error 级问题呈现给用户
- [ ] 未溯源的旧 concept/entity 正文保持原样（设计允许未溯源内容；lint 会警告，不阻塞）。**待决策**：是否提供"重跑构建补溯源"入口，或接受存量条目长期未溯源

### M4 — 前端契约更新

- [ ] `useKnowledgeBase.loadExistingStatus`：source 匹配从 `raw/ref-xxx.pdf` 改为 `ref-xxx`
- [ ] 下线 tags 相关展示（badge、tag 列、tag_titles 字段引用）
- [ ] 关联列表渲染谓词（`related` 省略，其余显示 `谓词 :: 标题`）
- [ ] 条目正文预览：对 `<ref-xxx>` 做剥离或高亮渲染（宿主 Tauri command 内先 `syntax::strip`，或前端按标签着色——建议前者，保持前端无解析逻辑）
- [ ] `knowledge_meta` 返回结构对齐（overview/recent，无 tags 计数）

### M5 — 下线旧库

- [ ] 回归清单全绿：知识库构建（含断点恢复）、文献导入、`useKnowledgeBase` 状态徽章、agent 检索、MCP 外部客户端连通
- [ ] workspace members 移除 `crates/fluen-knowledge`，删除 crate 及 `agent_tools` 中残留的旧 API 引用
- [ ] project memory 更新：知识库规则以 fluen-kb 为准

## 3. 风险与对策

| 风险 | 对策 |
|---|---|
| rmcp 内存传输不可用，退化为子进程双连接 | WAL + busy_timeout 5s 已就绪；写操作集中在 MCP 侧，宿主侧只读为主，冲突面小 |
| migrate 对非标准旧条目报错中断 | migrate 前强制备份；逐条目容错：失败条目记录并跳过（报告呈现），不中断整体 |
| AI 生成的 body 破坏 summary 单源约束 | 新库写时校验已兜底（create/merge/upsert 均拒绝跨源），错误信息含条目 id，管线重试即可 |
| 前端并行期新旧字段混用 | M1-M3 期间 Tauri command 返回结构保持旧形状（宿主层做适配），M4 一次性切换前端 |
| 正文 `<ref-xxx>` 泄漏到导出/预览 | 统一约定：所有 body 出 SDK 后先 strip（预览、组装、导出三条路径都过） |

## 4. 明确不做

- 不实现 fluen-kb 的 referee-ai `tools` feature（决策已定，Agent 走 MCP）
- 不做旧库 `RetrievalMethodUsed` 的精确对等复刻（仅在前端确有依赖时补）
- 不迁移旧 tags 数据（设计已移除该概念，tags 随 migrate 丢弃）
