# 大纲面板优化方案与执行计划

> 状态：已实施（Phase 0-4 全部落地，2026-08-22）
> 范围：`src/views/main/composables/outlineParser.ts`、`useOutline.ts`、`functionpanel/panels/OutlinePanel.vue`、`OutlineNodeItem.vue` 及编辑器联动层
> 关联：Motis AI 多写者收口为后续独立议题，本方案不依赖它（见第九节）

## 一、问题诊断

### 1.1 症状与根因对应

| 症状 | 根因 |
|------|------|
| 卡顿 | 每次按键全量重算 + 全量重渲染（§1.2） |
| 结构变化不实时 / 回跳 / 丢字 | CM6 与后端 `main_md` 双数据源冲突（§1.3） |
| 不美观 / 冗余元素 | 每行常驻隐藏按钮、占位元素、逐层嵌套（§1.4） |

### 1.2 根因一：每次按键全量重算

当前数据链（每敲一个字跑一遍）：

```
CM6 事务
 → update.state.doc.toString()          // codemirror/setup.ts:70，O(N) 全文拷贝
 → parseOutline(md)                      // useOutline.ts:58，split + 逐行正则 + 建树
 → filterByLevel(...)                    // useOutline.ts:85，整树再克隆一次
 → 所有 OutlineNodeItem 收到全新 node 对象 → 整棵递归树重渲染
```

放大器：

- **key 不稳定**：`OutlinePanel.vue:145` 与 `OutlineNodeItem.vue:214` 用 `` `${sectionId}-${line}` `` 作 key。标题上方增删一行会使其后所有标题的 `line` 偏移 → key 变化 → Vue 将这些子树**销毁重建**而非 patch，伴随 GC 压力。
- **面板常驻**：`Sidebar.vue:40` 用 `v-show`，未激活时上述开销照付。

### 1.3 根因二：双数据源冲突（"不实时"的真相）

运行时存在两条互不同步的真相链：

| 数据源 | 写入者 | 读取者 |
|--------|--------|--------|
| CM6 编辑器缓冲 | 用户打字 | `onDocChange` → 大纲、预览 |
| 后端 `_currentProject.main_md` | 打开/保存/结构 IPC | `mainMd` watch → 大纲、`:md` prop |

大纲面板的重命名/插入走第二条链（`useProject.ts:110-170`）：IPC 到后端，基于**上次保存的内容**做文本匹配。后果：

1. 编辑器有未保存修改时，后端匹配的是旧文本 → 操作失败或错位；
2. 即使成功，返回值整体替换 `_currentProject` → `FluenEditor.vue:72-79` 的 `props.md` watch 触发 `setMd()` → **用后端版本覆盖编辑器缓冲，未保存输入被静默丢弃**，undo 历史清空（`setMd` 重建 `EditorState`）;
3. 往返延迟期间 `isSaving` 锁 UI，两次解析先后落地造成闪烁；
4. 第二条链自身的放大器：`watch(mainMd)` 注册在 `useOutline()` 函数体内（`useOutline.ts:75-81`），而递归组件 `OutlineNodeItem` 每个节点实例各调用一次 `useOutline()` → N 个标题注册 N+1 份 watcher，`mainMd` 每次变化（打开/保存/结构 IPC）触发 N+1 次全量 `parseOutline`。注意它不放大打字开销（§1.2），只放大本链路。

### 1.4 冗余 DOM

以 100 个标题为例，面板常驻约：100 个占位 span（`OutlineNodeItem.vue:146`）、200 个 `opacity:0` 的 hover 按钮、约 400 条 SVG `<path>`（每行 chevron 1 + 重命名 2 + 插子标题 1）；chevron 用两条 path 切换方向而非 CSS rotate；SVG 大量内联重复。另发现死代码：`useOutline.ts:32` 的 `jumpTarget` 无任何消费者。

---

## 二、目标与非目标

### 目标

1. 打字过程中大纲更新无可感知开销（增量维护，无全量解析、无整树克隆、无子树重建）；
2. 重命名 / 插入子标题瞬时生效（本地事务），且**不再有丢字风险**；
3. 任一时刻 DOM 中只存在一份操作按钮；节点行结构精简；
4. 模块职责单一：纯函数解析 / 文档同步 / UI 状态 / 展示组件四层清晰，可单测。

### 非目标

- 不改动后端命令协议（`rename_heading` / `insert_heading` 保留但退出交互路径）;
- 不做虚拟滚动（扁平化为其留口，数百标题内无必要）;
- 不在本方案内处理 Motis AI 写稿的多写者收口（见第九节）。

## 三、设计原则

**编辑器打开期间，CM6 是内容唯一事实源；结构操作是对 CM6 的本地事务；后端只负责持久化。**

由此三条推论：

- 一切下游消费（大纲、预览、dirty 标记、章节备份）经同一条 `onDocChange` 管道自然一致；
- 本地 dispatch 进入 CM6 history → **Ctrl+Z 可撤销大纲操作**（现有 IPC 路径做不到）；
- 章节 `sec-*.md` 备份时机不变：仍由 `editor_save_content → save_document` 在保存时统一拆分。

---

## 四、目标架构

### 4.1 模块划分

```
src/views/main/composables/outline/
  outlineParser.ts        # 纯函数：parseFlat / buildTree / filterByLevel（迁移+扩展）
  headingIds.ts           # 纯函数：assignStableIds —— 跨解析的稳定节点 ID（新）
  headingOps.ts           # 纯函数：renameHeadingAt / insertChildAt → 文本编辑片段（新）
  useOutlineDocument.ts   # 订阅 CM6 事务，增量维护扁平标题索引（新）
  useOutlineUi.ts         # 折叠集合、hover 行、输入框等瞬态状态（从 useOutline 拆出）
  index.ts                # useOutline() 门面，合并导出保持调用方 API 兼容

components/functionpanel/panels/
  OutlinePanel.vue        # 壳：标题栏 / 新建章节 / 空态（基本不变）
  OutlineTree.vue         # 扁平列表渲染 + 可见行计算（新，替代递归）
  OutlineRow.vue          # 纯展示行，只 emit 意图（替代 OutlineNodeItem）
```

删除：`useOutline.ts`（拆分并入上列模块）、`OutlineNodeItem.vue`（由 OutlineRow 替代）。

### 4.2 数据流

```
用户打字 ──────────────→ CM6 事务 ─→ onDocUpdate(update) ┐
重命名/插子标题 ─→ headingOps 计算编辑片段 ─→ view.dispatch ┘
                        │
          useOutlineDocument：iterChangedRanges 增量扫描，
          维护 flatHeadings: shallowRef<FlatHeading[]>，
          assignStableIds 保持 id 稳定
                        │
          visibleRows = computed(折叠过滤)
                        │
          OutlineTree（v-for 扁平行，key=stableId，v-memo）
                        │
          保存（既有 debounce/Ctrl+S）─→ editor_save_content
                                        （校验 + main.md + sec-*.md 备份）

外部写者（新建章节 createSection 等）：仍走 IPC，但增加 dirty 守卫。
```

---

## 五、关键设计

### 5.1 稳定节点 ID（headingIds.ts）

`assignStableIds(prevFlat, nextParsed)` 以内容键 `sectionId|level|text` 匹配新旧列表并复用旧 id；匹配不到的新建 id。约 30 行，O(N)。

- 效果：上方打字/插行不再改变任何节点的 key → 零子树重建；重命名会使该行获得新 id（仅一行 remount，可接受；折叠集合按 id 存储，该行的折叠状态随之丢失，已知取舍）。
- `FlatHeading` 在解析期附带 `depth`（树深度），供缩进使用，不再依赖嵌套 `<ul>`。

### 5.2 结构操作本地事务化（headingOps.ts）

纯函数，输入文档文本与锚点信息，输出 `{changes}` 编辑片段，由调用方 dispatch 进 CM6：

- `renameHeadingAt(line, oldText, level, newText)`：校验该行内容与旧文本一致后替换为 `'#'.repeat(level) + ' ' + newText`；不一致返回错误（防御失步）。
- `insertChildAt(docText, parentNode, title)`：定位父作用域末尾（下一个 `level <= parent.level` 的标题行之前，或文件尾），插入 `newLevel 标题行 + 空行`；子标题不生成新 `@sec_id` marker（归属父 section，与现后端 `insert_heading` 语义一致）。

锚点用 `flatHeadings` 中新鲜的 `line`（增量维护保证新鲜度）+ 文本校验双保险，不再依赖后端旧副本。

**保留 IPC 的操作**：`createSection`（需动文件系统生成 `sec-*.md`）仍走后端，但外层加 dirty 守卫——编辑器脏时先触发保存再创建，杜绝覆盖。

### 5.3 增量解析管线（useOutlineDocument.ts）

- `useFluenEditor` 新增 `onTransaction(cb: (u: Update) => void)` 订阅（现有 `onDocChange(md)` 保留给预览/liveMd，不受影响）。
- 事务到达时用 `update.changes.iterChangedRanges()` 取变化区间，只对区间内行做正则扫描，splice 进 `flatHeadings`。
- 围栏状态（``` 开关）可能随编辑翻转：扫描前从文档头快速推进围栏奇偶至编辑起点（无分配的轻循环，MB 级预估 <5ms，以 Phase 2 验收实测为准）；若编辑前后奇偶一致则完全跳过。
- 每次 splice 后跑一遍 `assignStableIds`（轻量 O(N) 匹配）。单次按键总成本预估 ≈ 微秒~亚毫秒级（Phase 2 验收实测确认），无需 debounce。
- 折叠集合继续按稳定 id 存储（现状按 sectionId 的思路正确，改为按新 id）。

### 5.4 扁平化渲染与折叠算法（OutlineTree.vue）

放弃递归组件，`v-for` 渲染 `visibleRows`。折叠 = 一次线性扫描：

```ts
const visibleRows = computed(() => {
  const out = [];
  let barrier = Infinity;               // 最近折叠祖先的层级
  for (const h of flatHeadings.value) {
    if (h.level <= barrier) barrier = Infinity;   // 走出折叠作用域
    if (h.level > barrier) continue;              // 折叠作用域内的子孙
    out.push(h);
    if (collapsedSet.value.has(h.id)) barrier = h.level;
  }
  return out;
});
```

缩进由 `depth` 映射为 `padding-left`；层级字号/字重沿用现有 h1–h6 class 方案。

### 5.5 渲染清理细则

| 现状 | 改为 |
|------|------|
| 每行常驻 2 个隐藏按钮（opacity:0） | 仅 `hoveredId === node.id`（或 focus-within）的行 `v-if` 渲染操作按钮——任一时刻全局最多一份 |
| 叶子占位 `<span>` 对齐 | 删除，CSS 缩进代替 |
| chevron 双 path 切换 | 单 path + `transform: rotate(-90deg)` 过渡 |
| 内联 SVG 重复 | 收敛为 `panels/icons.ts` 小型函数式图标组件 |
| 全量 patch | 行级 `v-memo="[node, isActive]"` |
| `jumpTarget` 死代码 | 删除（`jumpTo` 已直连 `scrollToLine`） |

键盘可达性：行容器 `tabindex="0"`，`:focus-within` 时同样显示操作按钮。

---

## 六、执行计划

每阶段独立 commit、可单独 revert；Phase 1 完成后即可交付主要体验改善。

### Phase 0 — 快速止血（约半天）

低风险小改，先行缓解卡顿：

1. 新增 `headingIds.ts`，解析后立即 `assignStableIds`；两处 key 改为稳定 id；
2. `onDocChange` 的解析加 200ms 尾随 debounce（Phase 2 增量化后移除）；
3. `watch(mainMd)` 移至模块级注册一次。

验收：连续打字时 DevTools 中大纲行不再成批 unmount/remount；Performance 面板无长任务。

### Phase 1 — 数据源统一：结构操作本地事务化（约 1 天，核心）

1. 新增 `headingOps.ts` + 单测；
2. `useFluenEditor` 暴露 `dispatchChanges(changes)`；
3. `useOutlineUi` 提供 `applyRename / applyInsertChild`（内部走 headingOps + dispatch）；
4. `OutlinePanel` / `OutlineNodeItem` 切换到新接口；`useProject.renameHeading / insertHeading` 退出交互路径（暂不删除）；
5. `createSection` 增加 dirty 守卫。

验收：
- 编辑器脏状态下重命名/插入不再丢字、不再回跳；
- 大纲操作可被 Ctrl+Z 撤销；
- 保存后 `sec-*.md` 备份与 main.md 一致（手动验证一个项目）。

### Phase 2 — 增量解析管线（约 1 天）

1. `useFluenEditor` 增加 `onTransaction`；新增 `useOutlineDocument.ts`；
2. `iterChangedRanges` 区间扫描 + 围栏奇偶处理 + `assignStableIds`；
3. 移除 Phase 0 的 debounce；`filterByLevel` 从渲染路径移除（纯函数保留）；
4. 旧 `useOutline` 双 watch 与模块副作用订阅删除。

验收：属性测试「随机编辑序列下，增量结果 === 全量 parse 结果」通过；500KB 文档打字 CPU 占用对比 Phase 0 明显下降。

### Phase 3 — 渲染重构（约 1 天）

1. 新增 `OutlineTree.vue` / `OutlineRow.vue` / `icons.ts`，删除 `OutlineNodeItem.vue`；
2. 扁平行渲染 + 可见行折叠算法 + hover 单份操作按钮 + v-memo；
3. i18n key 复用现有命名，无新增文案（如确需新增补三语言）。

验收：DOM 节点数较现状下降 ≥50%（100 标题样例对比）；交互清单回归：跳转、高亮、重命名、插子标题、折叠/全部展开收起、空态、无项目态。

### Phase 4 — 清理与测试补齐（约半天）

1. 删除 `jumpTarget`、确认 `useProject.renameHeading/insertHeading` 去留（建议删除前端调用封装，Tauri 命令保留）；
2. 单测补齐：outlineParser / headingIds / headingOps / useOutlineDocument；
3. 更新相关模块注释与本文档状态为"已实施"。

## 七、测试计划

| 层 | 内容 |
|----|------|
| outlineParser | 围栏内 `#` 忽略、marker 继承、层级提升（filterByLevel）、空文档 |
| headingIds | 上方插入/删除行后 id 不变、重名标题队列匹配、全新标题分配新 id |
| headingOps | 重命名行校验失败报错、父作用域末尾插入（含 EOF）、嵌套作用域边界 |
| useOutlineDocument | 随机编辑序列属性测试：增量 === 全量；围栏翻转场景 |
| 手动回归 | 第六节各阶段验收项 |

运行方式：`pnpm vitest run src/views/main/composables/outline`、`pnpm typecheck`、`pnpm build`。

## 八、风险与回滚

| 风险 | 缓解 |
|------|------|
| 本地重命名后、保存前 sec-*.md 备份暂时滞后 | 与普通打字的备份时机一致（均保存时拆分），非新增风险；文档中明示 |
| 增量解析边界 bug（围栏、marker 跨区间） | 属性测试兜底；实现保留"超出阈值直接全量重 parse"的降级开关 |
| headingOps 校验失败（理论失步） | 返回明确错误并在 UI 提示，绝不静默错改 |
| 回滚 | 各 Phase 独立 commit；Phase 1 后 IPC 命令仍在，可一键切回旧调用 |

已知限制（维持现状，不在本方案解决）：setext 式标题（下划线 `===`）不入大纲。

## 九、后续演进（独立议题）

Motis AI 写稿（`agent_tools/manuscript.rs`）是 main.md 的第三个写者，与编辑器脏缓冲存在同类冲突。建议在 Phase 1 落地、dirty 语义清晰之后，将所有**后端**写者收口到一个统一的 Document Service（可注册进 referee 微内核，复用其排队/监督/WAL），规则：写入前检查 dirty → 脏则拒绝或提示合并；干净则写入并通知前端刷新。交互路径永远不经过该服务。
