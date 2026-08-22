# outline — 大纲面板模块

> 位置：`src/views/main/composables/outline` + `components/functionpanel/panels`（OutlinePanel/OutlineTree/OutlineRow）
> 设计原则：编辑器打开期间，CM6 缓冲是内容唯一事实源；结构操作是对 CM6 的本地事务（可 Ctrl+Z 撤销）；后端只负责持久化。

## 模块分层

| 层 | 文件 | 职责 |
|----|------|------|
| 纯函数·解析 | `outlineParser.ts` | 解析 `main.md` 为扁平标题（`FlatHeading`），附 `depth`/`childrenCount`；保留树形聚合 `buildTree`/`filterByLevel` |
| 纯函数·稳定 id | `headingIds.ts` | `assignStableIds`：按 `sectionId|level|text` 复用旧 id，避免上方增删行导致 key 变化、子树重建 |
| 纯函数·结构操作 | `headingOps.ts` | `renameHeadingAt`/`insertChildAt`：计算文档编辑片段（`ChangeSpec`），由调用方 dispatch |
| 文档同步 | `useOutlineDocument.ts` | 订阅 CM6 变化与项目打开事件，维护扁平标题列表（模块级注册一次，避免 N+1 watcher） |
| UI 瞬态 | `useOutlineUi.ts` | 折叠集合、hover 行、活动行、编辑输入状态；`applyRename`/`applyInsertChild` 走本地事务 |
| 门面 | `index.ts` | `useOutline()` 合并导出，保持调用方 API 兼容 |
| 渲染·扁平 | `OutlineTree.vue` | 计算可见行（折叠屏障线性扫描）+ `v-for` 渲染 |
| 渲染·纯展示 | `OutlineRow.vue` | 只 emit 意图，不消费单例状态 |
| 渲染·壳 | `OutlinePanel.vue` | 标题栏、空态、新建章节（含 dirty 守卫） |

## 数据流

```
用户打字 ──→ CM6 事务 ──→ onDocChange ──→ parseOutlineFlat + assignStableIds ──→ flatHeadings
结构操作 ──→ headingOps 计算片段 ──→ useFluenEditor.dispatchChanges（进入 history）──────→ 同上
重命名/插子标题 可被 Ctrl+Z 撤销；sec-*.md 拆分时机不变（保存时统一）。
```

## 关键约束

- 稳定 key：扁平列表以 `node.id` 作 `:key`，折叠集合按 id 索引；重命名使该行获得新 id（仅一行 remount，折叠状态随之丢失，已知取舍）。
- 结构操作校验：`headingOps` 对目标行做文本复检，失步时返回错误、绝不静默错改。
- `createSection`（需生成本地文件）仍走后端，外层加 dirty 守卫：编辑器脏时先保存再创建。
- 后端 `rename_heading` / `insert_heading` 命令保留但退出交互路径（前端不调用）。

## 测试

- 纯函数可独立单测：`outlineParser`（围栏/继承/空文档）、`headingIds`（上方插入删除 id 不变、重名匹配）、`headingOps`（失步报错、作用域末尾/EOF 插入）。
- 运行：`pnpm vitest run src/views/main/composables/outline`、`pnpm typecheck`。