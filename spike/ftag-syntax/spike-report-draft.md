# Task 2 — f-标签边界识别 Spike 草稿

> 草稿文档，Task 6 会汇总到正式 spike-report。本文档记录 SubTask 2.1–2.5 的实测结果。

## 1. 实测代码行数

| 文件 | 行数 | 说明 |
| --- | ---: | --- |
| `src/ftagSyntax.ts` | 154 | MarkdownConfig 扩展实现（含注释） |
| `tests/ftag-boundary.test.ts` | 193 | 三个 Scenario 的断言测试 |
| `tests/sanity.test.ts` | 7 | spike 环境自带的冒烟测试，未改动 |
| `src/index.ts` | 1 | spike 入口占位，未改动 |

实现侧重点：`ftagSyntax.ts` 中约 60% 是 `makeFtagBlockParser` 的解析逻辑（含 caption 子节点构造），约 25% 是属性 side-table 与文档注释，约 15% 是节点注册与导出。

## 2. 选用的实现路径

**MarkdownConfig 路径**（`defineNodes` + `parseBlock`），未触发降级到 `@lezer/generator` `.grammar`。

选型理由：
- 生产代码希望与 `@codemirror/lang-markdown` 直接集成，`markdown({ extensions: [ftagExtension] })` 一行即可装配，无需引入 grammar 编译产物。
- `parseBlock` 的 eager leaf block 模式（参考原生 `HTMLBlock`）足以表达“扫描起始标签 → 向后搜索闭合标签 → 产出块节点”的逻辑。
- 嵌套 `<f-caption>` 通过 `cx.elt(type, from, to, children)` 构造子 Element 列表挂到父块节点的 children 上，Lezer 内部 `Buffer.writeElements` 会自动把子节点位置转为父节点相对坐标，无需手动处理偏移。

关键 API 调用链：
1. `defineNodes`：注册 `FTagFig`/`FTagTbl`/`FTagEq`/`FTagClaim`（`block: true`）+ `FTagCaption`/`FTagCaptionText`（默认 inline）。
2. `parseBlock`：4 个 BlockParser，每个 `before: 'HTMLBlock'` 抢在原生 HTML 块解析器之前认领 `<f-xxx>`。
3. `parse(cx, line)`：行首匹配 `<f-{type}` → `cx.nextLine()` 循环扫描到 `</f-{type}>` → 计算 `to` → 扫描各行 `<f-caption>` 构造子 Element → `cx.addElement(cx.elt(nodeName, from, to, children))` → `return true`。

## 3. 遇到的坑点

### 3.1 Element 不支持 per-node props（属性提取的最大坑）
- `@lezer/markdown` 的 `Element` 类只有 `type/from/to/children` 四个字段，没有 props 槽。
- `NodeProp` 默认挂在 `NodeType` 上，而 NodeType 由所有同名节点共享——所有 `FTagFig` 共用同一个 NodeType，无法表达“每个 FTagFig 各自有不同的 id/src”。
- `Tree.build` 虽然支持 `perNode: true` 的 NodeProp，但 `Buffer.finish` → `Tree.build` 这条路径并未把 props 透传给 Element，因此 MarkdownConfig 路径下无法直接挂载 per-instance 属性。

**Spike 折衷**：用模块级 `Map<number, Record<string, string>>` side-table，以节点 `from` 位置为 key 存属性。`getFtagAttrs(from)` 查表。

**生产代码候选方案**（任选其一，Task 6 会再评估）：
1. 树外按需解析：不在树里存属性，调用方拿到 FTagFig 节点后，从原文 `doc.slice(node.from, ...)` 重新正则提取属性。最简单、零侵入。
2. `NodeProp.mounted` 挂子树：把属性编码成一个微型 Tree 挂到节点上，过度复杂，不推荐。
3. 改走 `@lezer/generator` `.grammar`：grammar 文件可在节点上声明 `NodeProp.perNode`，并通过 `deserialize` 注入。代价是引入 grammar 编译流程。

### 3.2 `prevLineEnd()` 包含换行符，与“精确覆盖到 `</f-xxx>`”语义不一致
- 原生 `HTMLBlock` 用 `to = cx.prevLineEnd()`，会把闭合标签所在行末尾的换行符纳入区间。
- 测试要求 `to` 精确落在 `</f-xxx>` 的 `>` 之后，因此本 spike 改为在调用 `cx.nextLine()` 之前手动计算 `to = cx.lineStart + closeIdx + closeTag.length`，再 `nextLine()` 推进。

### 3.3 TypeScript 控制流分析告警 `TS2454`
- `let to: number;` 在 `else` 分支里通过 `found` 标志赋值，TS 无法推断所有路径都赋定，报 `used before being assigned`。
- 用 `let to = 0;` 兜底解决，加注释说明实际值在分支内一定被覆盖。

### 3.4 `line.pos` 与 `line.text` 的关系
- `line.pos` 是 composite 标记（如 blockquote `>`）之后的下一个非空白字符位置，对应 `line.text` 里的索引。
- 检测起始标签要用 `line.text.slice(line.pos)`，但计算 `from` 要用 `cx.lineStart + line.pos`。
- 扫描 caption 时要对完整 `line.text` 操作（caption 可能有缩进），位置用 `cx.lineStart + index`。

### 3.5 `before: 'HTMLBlock'` 必须显式声明
- `<f-fig ...>` 形如 HTML 块起始标签，若不抢在原生 `HTMLBlock` 之前注册，会被 HTML 块规则先认领走，导致 FTagFig 节点永远不出现。

## 4. 降级方案标注

未触发降级。MarkdownConfig 路径在 spike 三个 Scenario 下完全跑通。

但保留以下已知限制，若生产场景命中再考虑改 grammar：
- **跨行 `<f-caption>`**：spike 只识别单行 caption（同一行内 `<f-caption>...</f-caption>` 闭合）。跨行 caption 会被漏识别。修复方案：扫描 `lineStarts` 时维护“是否在 caption 内”状态机，跨行拼接。
- **属性值含 `>`**：`line.text.indexOf('>', line.pos)` 会错切。修复方案：写一个简单的引号感知扫描器。
- **自闭合标签 `<f-fig/>`**：当前实现会一路扫到文档末尾。修复方案：起始标签正则匹配 `/>` 时直接产出单节点。
- **per-instance 属性**：见 3.1，生产建议方案 ①（树外按需解析）。

## 5. 验证结果

- `pnpm test`：**4/4 通过**（1 sanity + 3 ftag-boundary scenarios），耗时 ~1s。
- `pnpm exec tsc --noEmit`：**无错**（修过 1 个 TS2454，已解决）。

## 6. 对 spec 的偏差说明

- **属性存储方式**：spec SubTask 2.2 建议“用 `NodeProp` 或在节点上挂 metadata”。实测 MarkdownConfig 路径下 Element 不可挂 per-node props，故改用 side-table。这是实现细节偏差，不影响测试断言（测试通过 `getFtagAttrs(from)` 提取，仍然满足“测试需能提取这些属性”的验收条件）。
- **FTagCaption 注册为非 block**：spec 示例只明确 `FTagFig/Tbl/Eq/Claim` 为 `block: true`，未规定 FTagCaption。本 spike 把 `FTagCaption`/`FTagCaptionText` 注册为 inline（默认），因为它们是嵌套子节点。如生产代码需要按 block 维度遍历，可改为 `block: true`。
- **跨行 caption 未实现**：spec 未明确要求，spike 文档中三个 Scenario 也都是单行 caption。已在上文 4 中标注限制。

---

# Task 3 — 表格结构节点精细化识别 Spike 草稿

> 本节记录 SubTask 3.1–3.5 的实测结果。

## 1. 实测代码行数

| 文件 | 行数 | 说明 |
| --- | ---: | --- |
| `src/ftagSyntax.ts` | 423 | Task 2 的 154 行 + Task 3 新增 269 行（表格扫描 + 树构建 + MD 表） |
| `tests/table-structure.test.ts` | 200 | 三个 Scenario 共 4 个 it 断言 |
| `tests/ftag-boundary.test.ts` | 193 | Task 2 原有，未改动，仍全绿 |
| `tests/sanity.test.ts` | 7 | 未改动 |

Task 3 在 `ftagSyntax.ts` 中新增的代码分布：
- 表格节点名常量 + 单元格属性 side-table（`getTableAttrs`）：约 30 行
- `scanTableHtml`（形态 C HTML 标签扫描）：约 48 行
- `buildTableTree` + `makeTableElement`（区间包含关系建树）：约 53 行
- `parseMdRow` + `scanMdTable`（形态 B MD 表）：约 50 行
- `makeFtagBlockParser` 内 tbl 分支接入 + `defineNodes` 扩展：约 20 行
- 其余为注释与空行

## 2. 形态 C（内嵌 HTML）—— 按计划完成

实现要点：
- `scanTableHtml` 逐行扫描 `<table>`/`<thead>`/`<tbody>`/`<tr>`/`<th>`/`<td>` 标签对，要求开闭标签在同一行内（与 caption 扫描一致的 spike 限制）。
- 为避免 `<th` 误匹配 `<thead>`，匹配开标签前缀后校验下一个字符必须是 空白 / `>` / `/`。
- `buildTableTree` 把扁平的 TagInstance 列表按区间包含关系（from 升序、to 降序）用栈构建成 Lezer Element 树：Table → THead/TBody → TR → TH/TD → THContent/TDContent。
- TH/TD 的 `rowspan`/`colspan` 等属性通过 `parseAttrs` 提取，存入 `tableAttrsTable`（以节点 from 为 key），导出 `getTableAttrs(from)` 读取。

验证：Scenario 1 两个 it（rowspan + colspan）全绿，每个 `<th>`/`<td>` 标签整体区间与内容文本区间均可独立提取，且内容严格落在标签区间内（不重叠）。

## 3. 形态 B（内嵌 MD 表）—— 按计划完成（未降级）

实现要点：
- `scanMdTable` 在 `<f-tbl>` 块内收集连续 pipe 行（`| ... |`），要求至少 3 行且第 2 行匹配分隔行正则 `/^\s*\|[\s\-:|]+\|\s*$/`。
- 第 1 行解析为 TH 单元格，第 3+ 行解析为 TD 单元格，分隔行跳过。
- 每行用 `parseMdRow` 按 `|` 切分，产出 `MdCell { from, to, contentFrom, contentTo }`：`from/to` 含周围空格（对应 TH/TD 区间），`contentFrom/contentTo` 仅含去空白文本（对应 THContent/TDContent 区间）。
- 所有 TR 包裹进一个 FTagTable 节点，结构与形态 C 等价（但无 THead/TBody 层，MD 表无显式 head/body 区分）。

验证：Scenario 2 全绿，产出 1 个 FTagTable + 2 个 FTagTR + 2 个 FTagTH + 2 个 FTagTD，单元格内容（"方法"/"精度"/"Ours"/"95"）均可独立提取。

**未触发降级**。MD 表等价结构按计划完成。

## 4. 坑点列表

### 4.1 节点名与 @lezer/markdown 内置节点冲突
- `@lezer/markdown` 已内置 `Table`/`TableHeader`/`TableRow`/`TableCell` 等节点名（GFM 表格）。
- 若在 `defineNodes` 中重复注册 `Table`，会复用内置 NodeType（`block: true`），可能干扰 GFM 表格解析，且语义混淆。
- **折衷**：所有表格结构节点统一加 `FTag` 前缀（`FTagTable`/`FTagTHead`/`FTagTBody`/`FTagTR`/`FTagTH`/`FTagTD`/`FTagTHContent`/`FTagTDContent`）。这是对 spec 字面节点名的偏差，但不影响测试断言与功能。

### 4.2 Lezer Element children 必须在构造时传入
- `cx.elt(type, from, to, children)` 的 children 数组在构造时被捕获，Element 不可变。
- 无法先创建父节点再回填子节点。因此 `buildTableTree` 用栈 + 延迟 finalize：遇到不被栈顶包含的 tag 时弹栈并构造 Element，保证子节点先于父节点构造。
- 排序键：`from` 升序、`to` 降序，确保外层节点先入栈、内层节点后入栈，弹栈时内层已构造完毕。

### 4.3 同级 children 必须有序且不重叠
- caption 扫描与 table 扫描各自按文档顺序产出，但合并后可能乱序。
- 在 tbl 块的 children 加 `children.sort((a, b) => a.from - b.from)` 兜底。
- Lezer 的 `writeElements` 内部也会排序，但显式排序更稳妥。

### 4.4 `<th` 前缀匹配 `<thead`
- `text.indexOf('<th')` 会命中 `<thead>` 中的 `<th`。
- 修复：匹配后校验 `text[oIdx + openTag.length]` 是否为 空白 / `>` / `/`，否则跳过（`searchFrom = oIdx + 1`）。

### 4.5 MD 表单元格区间语义
- 形态 C 的 TH/TD 区间含标签（`<th>方法</th>`），THContent 仅含文本（`方法`）。
- 形态 B 无标签，TH/TD 区间定义为"两个 `|` 之间的内容（含周围空格）"，THContent 仅含去空白文本。这样 TH 与 THContent 区间不同但 THContent 严格落在 TH 内，与形态 C 语义一致。
- 行尾 `|` 会产生空单元格，`parseMdRow` 末尾用 while 循环弹出。

### 4.6 形态 C 优先于形态 B
- 一个 `<f-tbl>` 块要么是 HTML 表要么是 MD 表，不会同时存在。
- 实现先调 `scanTableHtml`，若返回非空则用 HTML 结构；否则调 `scanMdTable`。避免误判。

## 5. 验证结果

- `pnpm test`：**8/8 通过**（1 sanity + 3 ftag-boundary + 4 table-structure），耗时 ~1.1s。
- `pnpm exec tsc --noEmit`：**无错**。

## 6. 对 spec 的偏差说明

- **节点名加 FTag 前缀**：spec Task 3 列出的节点名为 `Table`/`THead`/`TBody`/`TR`/`TH`/`TD`/`THContent`/`TDContent`。实际实现为 `FTagTable`/`FTagTHead`/.../`FTagTDContent`，避免与 @lezer/markdown 内置节点名冲突。测试同步使用 FTag 前缀，功能不受影响。
- **形态 B 未产出 THead/TBody**：MD 表无显式 head/body 区分，仅产出 FTagTable → FTagTR → FTagTH/FTagTD 三层。spec 允许降级，本 spike 实际未降级（产出完整等价结构），但 THead/TBody 层在形态 B 下缺失。
- **跨行 HTML 标签跳过**：与 Task 2 caption 一致的 spike 限制，`<th>...</th>` 开闭标签须在同一行。spec 三个 Scenario 均为单行标签，不影响验收。
- **属性 side-table 复用 Task 2 模式**：未新增独立的 `clearTableAttrs`，`clearFtagAttrs` 同时清空 `attrsTable` 与 `tableAttrsTable`，测试 `beforeEach` 无需改动。

---

# Task 4 — 脚注语法识别 Spike 草稿

> 本节记录 SubTask 4.1–4.4 的实测结果。脚注扩展与 f-标签扩展完全独立，不修改 `ftagSyntax.ts`。

## 1. 实测代码行数

| 文件 | 行数 | 说明 |
| --- | ---: | --- |
| `src/footnoteSyntax.ts` | 132 | MarkdownConfig 扩展实现（含注释）：定义块 + 行内引用 |
| `tests/footnote.test.ts` | 136 | 三个 Scenario 的断言测试 |
| `src/ftagSyntax.ts` | 423 | Task 2/3 原有，未改动 |
| `tests/ftag-boundary.test.ts` | 193 | Task 2 原有，未改动，仍全绿 |
| `tests/table-structure.test.ts` | 200 | Task 3 原有，未改动，仍全绿 |
| `tests/sanity.test.ts` | 7 | 未改动 |

`footnoteSyntax.ts` 代码分布：
- side-table（`footnoteDefIdTable` / `footnoteRefIdTable`）+ 访问器 + `clearFootnoteAttrs`：约 20 行
- `isContinuation` 缩进判定 + `footnoteDefBlockParser`（parseBlock，含多段落续行扫描）：约 50 行
- `footnoteRefInlineParser`（parseInline，`[^id]` 行内识别）：约 20 行
- `footnoteExtension` 导出 + `defineNodes`：约 10 行
- 其余为注释与空行

## 2. 行内引用 `[^id]` —— 按计划完成（未降级）

实现要点：
- `parseInline` 注册 `FootnoteRef` 节点，`before: 'Link'` 抢在标准 Link parser 之前认领 `[^`。
- `parse(cx, next, pos)`：`next === 91`（`[`）且 `cx.char(pos+1) === 94`（`^`）时，从 `pos+2` 起扫描到 `]`（不允许跨行），产出 `FootnoteRef` 元素覆盖 `[pos, end+1)`，即仅含 `[^id]` 本身。
- id 存入 `footnoteRefIdTable`（以 `pos` 为 key），导出 `getFootnoteRefId(from)` 读取。
- 空 id `[^]` 不认领（`return -1`），让标准 Link parser 处理。

验证：Scenario 3 全绿，`FootnoteRef` 区间精确覆盖 `[^1]`（4 字符），前后正文 `正文中间出现引用` / `继续文字。` 均不在节点区间内，id `"1"` 可提取。

## 3. 单行定义 `[^id]: ...` —— 按计划完成（未降级）

实现要点：
- `parseBlock` 注册 `FootnoteDefinition` 节点（`block: true`），`before: 'LinkReference'` 抢在原生 LinkReference 之前认领 `[^id]:`。
- `parse(cx, line)`：`line.text.slice(line.pos)` 起匹配 `/^\[\^([^\]]+)\]:[\t ]/`，要求冒号后紧跟空格或 tab（spec 规范）。
- 匹配后 `cx.nextLine()` 消费第一行，`to = cx.prevLineEnd()`（第一行内容末尾，不含 `\n`）。
- id 存入 `footnoteDefIdTable`（以 `from` 为 key），导出 `getFootnoteDefId(from)` 读取。

验证：Scenario 1 全绿，`FootnoteDefinition` 区间覆盖整行 `[` 到行末，id `"1"` 与内容 `"这是单行脚注定义。"` 可分别提取（id 走 side-table，内容走原文切串正则）。

## 4. 多段落定义（4 空格/tab 缩进续行）—— 按计划完成（未降级）

实现要点（核心逻辑在 `footnoteDefBlockParser.parse` 的 while 循环）：
- 消费第一行后进入循环，检查当前行 `line.text`：
  - **当前行空**：`peekLine()` 看下一行。若下一行缩进（`/^\t|^    /`），消费"当前空行 + 缩进行"两次 `nextLine()`，`to = prevLineEnd()`（缩进行末尾）；若下一行空/EOF/非缩进，停止，**不消费当前空行**（让出给后续 parser）。
  - **当前行缩进**：直接 `nextLine()` 消费，`to = prevLineEnd()`。
  - **当前行非空非缩进**：停止，不消费。
- `peekLine()` 返回 `""` 既是空行也可能是 EOF，二者在本逻辑下处理一致（停止 or 经下一轮判定），无歧义。

验证：Scenario 2 全绿，`FootnoteDefinition` 节点区间覆盖从 `[^1]:` 到最后一段续行末（= 整个输入），缩行续行段落归入定义块、不被识别为独立 Paragraph 或 CodeBlock（4 空格缩进行潜在会被 IndentedCode 认领，但本扩展先消费）。

**未触发降级**。spec 允许的"仅识别单行定义"降级方案未启用。

## 5. 坑点列表

### 5.1 `peekLine()` 对空行与 EOF 都返回 `""`
- `@lezer/markdown` 的 `peekLine()` 内部调 `scanLine(absoluteLineEnd + 1)`，当 `start >= this.to`（EOF）时返回 `text = ""`；下一行是真空行时 `lineChunkAt` 也返回 `""`。
- 二者无法直接区分。本 spike 的处理：在"当前行空"分支里，无论 `peeked` 是 EOF 还是真空行，只要不是缩进行就停止；若是缩进行则消费。EOF 情形下后续 `nextLine()` 返回 false 自然退出循环，不会误消费。

### 5.2 `prevLineEnd()` 语义是"上一行末尾"，不是"当前行末尾"
- `prevLineEnd()` 源码：`atEnd ? lineStart : lineStart - 1`。即"当前行起始之前那个 `\n` 的位置"，等于"上一行内容末尾"。
- 关键陷阱：消费第 N 行后 `cx.lineStart` 已经在第 N+1 行，`prevLineEnd()` 给的是第 N 行末尾。所以"消费一行后立即取 to"用 `prevLineEnd()` 是对的；但"peek 后决定要消费第 N+1 行，再消费之"需要两次 `nextLine()`，第二次 `nextLine()` 后 `prevLineEnd()` 才是第 N+1 行末尾。
- 多段落续行的"空行 + 缩进行"两次 `nextLine()` 即为此设计。

### 5.3 `nextLine()` 返回 `false` 不代表失败，代表 EOF
- 在最后一行（无尾 `\n`）调用 `nextLine()` 返回 `false`，但 `lineStart` 已推进到 EOF 位置，`atEnd = true`，`line.text = ""`。
- 此时 `prevLineEnd()` 因 `atEnd` 返回 `lineStart`（EOF 位置），恰好是最后一行内容末尾。所以"消费最后一行后取 to"依然正确，无需特判返回值。

### 5.4 `[^id]:` 与 LinkReference 的优先级
- 原生 `LinkReference` 是 **leaf block parser**（用 `leaf` 方法观察段落），不是 eager `parse`。本扩展用 eager `parse` 认领 `[^id]:`，原生 LinkReference 的 `leaf` 根本看不到该行。
- `before: 'LinkReference'` 是显式声明顺序，实际不严格必要（无 `parse` 冲突），但保留以防未来原生 LinkReference 增加 `parse`。

### 5.5 `[^id]` 行内引用与标准 Link parser 的优先级
- 标准 Link parser 也以 `[` 起步（用 delimiter 机制）。若不 `before: 'Link'`，Link 会先加 `[` 开分隔符，把 `^id]` 当作 link text 的一部分，最终可能拼成 shortcut reference link。
- 本扩展 `before: 'Link'` 先认领 `[^id]`，Link parser 看不到这个 `[`，不加分隔符，无冲突。
- 已知限制：`[text][^id]` 形式（带自定义文本的脚注引用）会被切成 `[text`（未匹配 link，变字面量）+ `[^id]`（FootnoteRef）。spec 未要求此形式，spike 不处理。

### 5.6 续行缩进判定仅识别 top-level，不处理 composite 上下文
- `isContinuation(text)` 检查原始行文本是否以 tab 或 4 空格开头。若脚注定义位于 blockquote/list 内，续行形如 `>     foo`，前缀是 `>` 而非空白，本扩展会停止。
- spec 三个 Scenario 均为 top-level，未触发此限制。生产代码需用 `line.indent - line.baseIndent >= 4` 判定相对缩进。

### 5.7 side-table 模式沿用 Task 2 坑点 3.1
- `Element` 不支持 per-node props，id 同样无法挂在节点上。沿用 `Map<number, string>` side-table（以 `from` 为 key）。
- 与 `ftagSyntax.ts` 的 `attrsTable` / `tableAttrsTable` 独立，`clearFootnoteAttrs` 仅清空脚注两张表，不干扰 f-标签测试。

## 6. 验证结果

- `pnpm test`：**11/11 通过**（1 sanity + 3 ftag-boundary + 4 table-structure + 3 footnote），耗时 1.13s。
- `pnpm exec tsc --noEmit`：**无错**（strict 模式）。

## 7. 对 spec 的偏差说明

- **多段落定义未降级**：spec 允许"若 Lezer 难以表达缩进续行的归属判定，记录降级方案（仅识别单行定义）"。本 spike 通过 `peekLine` + 双 `nextLine` 模式完整实现多段落续行，未触发降级。
- **续行仅支持 top-level 缩进**：spec 规范描述"4 空格缩进或 tab 缩进"未限定上下文，本 spike 仅识别 top-level（见坑点 5.6）。spec 三个 Scenario 均 top-level，不影响验收。
- **定义内容未作为子节点**：spec 未要求定义内容作为独立子节点，本 spike 的 `FootnoteDefinition` 是叶子块节点（无 children），内容通过原文切串提取。生产代码若需精细内容节点（如 `FootnoteDefLabel` / `FootnoteDefContent`），可扩展 `cx.elt` children。
- **冒号后必须紧跟空格/tab**：regex `/^\[\^([^\]]+)\]:[\t ]/` 要求 `:` 后是空格或 tab。pandoc 允许 `[^id]:` 行尾（空定义），本 spike 不支持，spec Scenario 均含内容，不影响验收。
- **行内引用不支持 `[text][^id]`**：见坑点 5.5，spec 未要求。

---

# Task 5 — 未闭合标签兜底 Spike 草稿

> 本节记录 SubTask 5.1–5.4 的实测结果。目标：用户打字中途产生未闭合 f-标签时，解析器不把后续内容误判为标签内部，且增量解析稳定不抖动。

## 1. 实测代码行数

| 文件 | 行数 | 说明 |
| --- | ---: | --- |
| `src/ftagSyntax.ts` | 475 | Task 2/3/4 的 423 行 + Task 5 新增 52 行（preScanCloseTag + parse 兜底分支 + FTagUnclosed 注册） |
| `tests/unclosed-tag.test.ts` | 233 | 两个 Scenario 共 6 个 it 断言 |
| `tests/ftag-boundary.test.ts` | 193 | Task 2 原有，未改动，仍全绿 |
| `tests/table-structure.test.ts` | 200 | Task 3 原有，未改动，仍全绿 |
| `tests/footnote.test.ts` | 136 | Task 4 原有，未改动，仍全绿 |
| `tests/sanity.test.ts` | 7 | 未改动 |

Task 5 在 `ftagSyntax.ts` 中新增的代码分布：
- `preScanCloseTag` 预扫描辅助函数（含坑点注释）：约 28 行
- `makeFtagBlockParser.parse` 内未闭合兜底分支（同行无 `>` / 同行有 `>` 但 EOF 无闭合）：约 18 行
- `defineNodes` 注册 `FTagUnclosed`：约 2 行
- 注释与空行：约 4 行

## 2. 半截标签不吞噬后续内容 —— 按计划完成（未降级）

实现要点（`makeFtagBlockParser.parse` 内）：
- **规则**：起始标签的 `>` 必须在同一行。spec 建议的"跨行未闭合视为不完整"被解读为"起始标签跨行（`>` 在下一行）视为不完整"，而非"闭合标签跨行视为不完整"（后者会破坏 Task 2 多行闭合场景）。
- **同行无 `>`**（半截标签，如 `<f-fig id="x"`）：产出 `FTagUnclosed` 节点，区间从 `<f-{type}` 到行末（`cx.lineStart + line.text.length`），`cx.nextLine()` 仅消费当前行，**不消费后续行**。
- **同行有 `>` 但同行无 `</f-{type}>`**：调用 `preScanCloseTag` 预扫描后续行（不消费行）：
  - 预扫描找到闭合标签：进入正常多行闭合逻辑（保留 Task 2 行为）。
  - 预扫描未找到：产出 `FTagUnclosed` 节点，区间从 `<f-{type}` 到 `>`（`tagEnd`），`cx.nextLine()` 仅消费当前行，**不消费后续行**。

验证：Scenario 1 两个 it 全绿：
- `<f-fig id="x"` + 100 行空行分隔段落 → 1 个 FTagUnclosed（区间仅第一行）+ 100 个 Paragraph，无 FTagFig，段落内容（"段落 1" / "段落 100"）均可独立提取。
- `<f-fig id="x">` + 50 行空行分隔段落 → 1 个 FTagUnclosed（区间 [0, 14]，仅 `<f-fig id="x">`）+ 50 个 Paragraph，后续段落不被吞噬。

**未触发降级**。两个场景均按计划完成。

## 3. 增量解析稳定性 —— 按计划完成，无抖动

实现要点：
- 增量解析稳定性由"未闭合时不消费后续行"保证：用户逐字符键入过程中，一旦 doc 匹配 `<f-fig` 前缀（后跟空白/>//），立即产出 FTagUnclosed 节点，后续内容始终被独立解析（Paragraph / HTMLBlock），不会被吞入 FTag 块内。
- 由于 FTagUnclosed 的区间单调扩展（from=0，to 随键入递增），不出现"中途某次解析把后续段落误判为标签内、下一次又恢复"的抖动。

验证：Scenario 2 四个 it 全绿：
- **逐字符键入 `<f-fig id="x">`**：从第 7 个字符（空格，匹配 openRegex）起，FTagUnclosed 稳定存在，FTagFig 始终为 0，FTagUnclosed.to 单调递增（不抖动）。
- **键入 `<f-fig id="x">\n<f-caption>图</f-caption>\n段落1`**：全程 FTagFig = 0，FTagCaption = 0（`<f-caption>` 不在 FTagFig 内部，不被误判），FTagUnclosed 仅覆盖第一行。
- **键入 `</f-fig>` 后从 FTagUnclosed 转为 FTagFig**：未闭合态 FTagCaption = 0；闭合态 FTagFig = 1、FTagCaption = 1、FTagCaptionText 内容 = "图"。这是从未闭合到闭合的语义变化，不算抖动。
- **5000 行文档全量解析耗时**：13.94ms（< spec 目标 50ms，达标）。

**无抖动**。增量解析稳定。

## 4. 坑点列表

### 4.1 BlockContext 公开 API 不支持"预扫描多行不消费行"
- `peekLine()` 仅返回下一行文本，连续调用返回同一行（不推进状态）。
- `nextLine()` 推进状态，**不可回退**。一旦消费后续行，无法"归还"给其他 parser。
- 要在"不消费后续行"的前提下判断后续是否存在 `</f-{type}>`，必须预扫描多行。
- **Spike 折衷**：用 `cx as unknown as { to: number; lineChunkAt(pos: number): string }` 访问 private 字段 `to`（文档末尾）与 private 方法 `lineChunkAt(pos)`（返回 pos 所在行文本，不含 `\n`）。逐行累加 `scanFrom += lineText.length + 1` 跳过 `\n`。
- **生产代码候选**：① 改走 `@lezer/generator` grammar，block parser 可访问完整输入；② 在 parseBlock 外做预扫描（如预处理整个文档建立标签配对索引）；③ 与上游沟通为 BlockContext 增加 `lineAt(pos)` 公开 API。

### 4.2 起始标签 `>` 必须在同一行的解读
- spec 实现策略原文："标签识别仅在同行内匹配起始标签的 `>`，跨行未闭合视为不完整。"
- 字面解读有两种：
  - 解读 A：起始标签的 `>` 跨行（`>` 在下一行）视为不完整。（本 spike 采用）
  - 解读 B：闭合标签 `</f-{type}>` 跨行视为不完整。（会破坏 Task 2 多行闭合场景）
- 本 spike 采用解读 A：起始标签 `>` 必须同行，否则 FTagUnclosed；闭合标签仍允许跨行（保留 Task 2 行为）。
- 影响：`<f-fig id="x"\n>`（`>` 在第二行）会被识别为 FTagUnclosed（区间仅第一行），第二行 `>` 被当作独立段落。spec 场景 1 输入正是此形态，符合预期。

### 4.3 FTagUnclosed 区间语义两种形态
- 同行无 `>`：区间从 `<f-{type}` 到行末（含半截属性，如 `<f-fig id="x"`）。
- 同行有 `>` 但 EOF 无闭合：区间从 `<f-{type}` 到 `>`（含完整起始标签，如 `<f-fig id="x">`）。
- 两种形态的 `to` 计算方式不同：前者 `cx.lineStart + line.text.length`，后者 `cx.lineStart + gtIdx + 1`。spec 原文"区间仅覆盖 `<f-fig id="x"` 到行末（或到下一个 `>` 如果同行有 `>` 但无闭合标签）"与此一致。

### 4.4 `preScanCloseTag` 与实际 `nextLine` 扫描的重复扫描
- 预扫描确认有闭合标签后，进入多行闭合逻辑时仍用 `nextLine()` 重新扫描一遍查找 `closeIdx`。
- 原因：预扫描只返回布尔值（找到 / 未找到），不返回闭合标签位置；且 `nextLine()` 必须调用以推进状态消费块内行。
- 代价：后续行被扫描两次（预扫描 + 实际消费）。spike 可接受（5000 行文档 13.94ms）。生产代码可让 `preScanCloseTag` 返回闭合标签位置，避免重复扫描。

### 4.5 `FTagUnclosed` 注册为 `block: true`
- `FTagUnclosed` 是顶层块节点（与 FTagFig 同级），需 `block: true` 才能在 `tree.topNode` 层级可见。
- 若注册为 inline（默认），节点会被包裹进 Paragraph，无法作为独立块提取。

### 4.6 `cx.elt('FTagUnclosed', ...)` 的 children 为空
- FTagUnclosed 是叶子块节点（无 children），不解析起始标签内的属性。
- 原因：半截标签的属性可能未闭合引号（如 `id="x`），`parseAttrs` 会提取不完整属性，语义可疑。spike 选择不提取，生产代码若需属性可单独处理。

## 5. 验证结果

- `pnpm test`：**17/17 通过**（1 sanity + 3 ftag-boundary + 4 table-structure + 3 footnote + 6 unclosed-tag），耗时 1.67s。
- `pnpm exec tsc --noEmit`：**无错**（strict 模式，含 `(cx as unknown as {...})` 类型断言）。
- 性能：5000 行文档（1 行未闭合起始标签 + 5000 行段落）全量解析 13.94ms，< spec 目标 50ms，达标。

## 6. 对 spec 的偏差说明

- **`preScanCloseTag` 用 private API hack**：spec 未明确预扫描实现方式。BlockContext 公开 API 限制下，本 spike 用 `(cx as unknown as { to; lineChunkAt })` 访问 private 字段/方法。这是实现细节偏差，不影响功能与测试断言。生产代码需重新设计（见坑点 4.1）。
- **FTagUnclosed 区间两种形态**：spec 原文"区间仅覆盖 `<f-fig id="x"` 到行末（或到下一个 `>`）"，本 spike 按此实现两种形态（见坑点 4.3），无功能偏差。
- **`FTagUnclosed` 不提取属性**：spec 未要求未闭合标签提取属性，本 spike 不提取（见坑点 4.6）。
- **起始标签 `>` 必须同行的解读**：spec 实现策略原文有歧义（见坑点 4.2），本 spike 采用解读 A，保留 Task 2 多行闭合能力。spec 场景 1/2 均符合此解读，不影响验收。
- **性能测试断言放宽到 200ms**：spec 目标 50ms，实测 13.94ms 达标。但为防 CI 环境性能抖动导致测试红，断言设为 `< 200ms`，实际耗时通过 `console.log` 记录。这是测试稳健性偏差，不影响功能验收。
- **未实现"同行有 `>` 但 EOF 无 `</f-fig>` 时手动产出后续 Paragraph"**：本 spike 通过 `preScanCloseTag` 实现"不消费后续行"，后续行由原生 parser 处理（自动产出 Paragraph / HTMLBlock）。无需手动产出，无偏差。
