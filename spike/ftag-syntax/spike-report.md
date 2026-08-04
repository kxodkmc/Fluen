# Spike：f-标签 / 表格 / 脚注 / 未闭合兜底 语法识别 —— 最终报告

> 本文档为 spike 内部文档，不入产品文档树。
> 草稿历史见 `spike/ftag-syntax/spike-report-draft.md`（各 Task 实测小节）。
> spike 工作目录：`d:\Dev\Fluen\spike\ftag-syntax\`。

---

## 1. 摘要

### 1.1 Spike 目标

验证 `@lezer/markdown` 的 `MarkdownConfig` 扩展路径能否表达 Fluen 编辑器所需的四类块级 / 行内语法：

1. **1.5.1 f-标签边界**：识别 `<f-fig>/<f-tbl>/<f-eq>/<f-claim>` 块级标签对 + 嵌套 `<f-caption>`。
2. **1.5.2 表格结构**：识别 `<f-tbl>` 块内嵌 HTML 表（形态 C）与内嵌 MD 表（形态 B），产出 FTagTable/TR/TH/TD 结构节点。
3. **1.5.3 脚注**：识别 `[^id]` 行内引用 + `[^id]: ...` 定义块（含 4 空格/tab 缩进的多段落续行）。
4. **1.5.4 未闭合兜底**：用户打字中途产生半截 / 未闭合 f-标签时，解析器不吞噬后续内容，且增量解析稳定不抖动。

### 1.2 整体结论

- **路径选型**：选用 `MarkdownConfig`（`defineNodes` + `parseBlock` + `parseInline`），未降级到 `@lezer/generator` `.grammar`。
- **测试结果**：17/17 全绿（1 sanity + 3 ftag-boundary + 4 table-structure + 3 footnote + 6 unclosed-tag），`tsc --noEmit` 无错。
- **性能**：5000 行文档全量解析 13.94ms，< spec 目标 50ms。
- **D1 三项决策推荐值**：
  - 表格单元格级编辑 → **继续做**（形态 C + B 均完成，未降级）
  - 脚注多段落定义 → **本期做完整版**（多段落续行可实现，未降级）
  - `ftagSyntax.ts` 行数预估 → **~750-900 行**（spike 实测 607 行 + 生产化增量 1.2-1.5 倍）

### 1.3 关键风险

最大风险是 Task 5 的 `preScanCloseTag` 使用了 `BlockContext` 的 private API（`to` 字段 + `lineChunkAt(pos)` 方法）。生产代码不能依赖 private API，需重新设计未闭合兜底的多行预扫描机制。详见第 7 节。

---

## 2. 各子项实测复杂度

### 2.1 实测代码行数汇总

| 子项 | 实现代码行数 | 测试代码行数 | 是否按计划完成 | 降级方案 |
|------|------------|------------|--------------|---------|
| 1.5.1 f-标签边界 | `ftagSyntax.ts` 中 Task 2 部分（154 行） | `ftag-boundary.test.ts` 193 行 | 是 | — |
| 1.5.2 表格结构 | `ftagSyntax.ts` 中 Task 3 部分（+269 行，累计 423） | `table-structure.test.ts` 200 行 | 是（形态 C + B 均完成） | — |
| 1.5.3 脚注 | `footnoteSyntax.ts` 132 行 | `footnote.test.ts` 136 行 | 是（含多段落续行） | — |
| 1.5.4 未闭合兜底 | `ftagSyntax.ts` 中 Task 5 部分（+52 行，累计 475） | `unclosed-tag.test.ts` 233 行 | 是 | — |

### 2.2 实测文件清单

| 文件 | 行数 | 说明 |
| --- | ---: | --- |
| `src/ftagSyntax.ts` | 475 | Task 2（154）+ Task 3（+269）+ Task 5（+52），累计实现 |
| `src/footnoteSyntax.ts` | 132 | Task 4 脚注扩展（定义块 + 行内引用 + 多段落续行） |
| `src/index.ts` | 1 | spike 入口占位 |
| `tests/ftag-boundary.test.ts` | 193 | Task 2 三个 Scenario |
| `tests/table-structure.test.ts` | 200 | Task 3 三个 Scenario 共 4 个 it |
| `tests/footnote.test.ts` | 136 | Task 4 三个 Scenario |
| `tests/unclosed-tag.test.ts` | 233 | Task 5 两个 Scenario 共 6 个 it |
| `tests/sanity.test.ts` | 7 | spike 环境自带冒烟测试，未改动 |

**spike 总生产代码**：`ftagSyntax.ts` 475 + `footnoteSyntax.ts` 132 = **607 行**。
**spike 总测试代码**：193 + 200 + 136 + 233 + 7 = **769 行**。

---

## 3. 实现路径选型对比：MarkdownConfig vs Lezer `.grammar`

### 3.1 Spike 实际选用

**MarkdownConfig 路径**（`defineNodes` + `parseBlock` + `parseInline`），全程未触发降级到 `@lezer/generator` `.grammar`。

### 3.2 对比维度

| 维度 | MarkdownConfig（spike 选用） | Lezer `.grammar` |
|------|------------------------------|------------------|
| **表达能力** | 命令式 TS 代码，任意复杂逻辑（栈、状态机、预扫描）均可写 | 声明式 grammar，正则 + 上下文规则；多行预扫描需借 external scanner |
| **维护成本** | TS 单文件，调试器 / 类型检查友好；新增块规则即新增 BlockParser | grammar 文件需经 `@lezer/generator` 编译生成 JS；改 grammar 后需 `lezer-generator` 构建 |
| **与 `@codemirror/lang-markdown` 集成** | 一行装配：`markdown({ extensions: [ftagExtension] })` | 需把生成的 parser 通过 `parser` 配置项替换 / 包装，且要重新挂接 GFM / 代码高亮等子扩展 |
| **per-node props** | Element 不支持 per-instance props（spike 折衷用 side-table `Map`） | grammar 可声明 `NodeProp.perNode`，通过 `deserialize` 注入 |
| **预扫描多行不消费行** | 公开 API 不支持，spike 用 private API hack（Task 5 最大风险） | external scanner 可访问完整输入 `input`，无需 hack |
| **性能** | 5000 行 13.94ms，达标 | 编译产物通常更快，但 spike 已达标 |

### 3.3 生产实现路径选型建议

**主路径仍推荐 MarkdownConfig**，理由：

1. 与 `@codemirror/lang-markdown` 集成成本最低，不破坏 GFM / 链接 / 代码高亮等现有扩展。
2. spike 已证明：边界识别、表格结构、脚注、未闭合兜底四类语法在 MarkdownConfig 路径下均可表达。
3. per-node props 缺陷可通过"树外按需解析"方案绕过（拿到节点后从原文重新切串正则提取属性），零侵入。
4. **唯一痛点**是 Task 5 的多行预扫描，private API hack 不能进生产。建议生产代码采用以下任一替代方案，而非整体改走 grammar：
   - 方案 A：在 `parseBlock` 外做预扫描——预处理整个文档建立"起始标签 → 闭合标签位置"索引，BlockParser 查索引决定是否认领。预扫描在每次 parse 开始时执行，O(n) 复杂度。
   - 方案 B：与上游沟通为 `BlockContext` 增加 `lineAt(pos): string` 公开 API（最理想，但需等上游发版）。
   - 方案 C：极端情况下改走 grammar + external scanner（仅 Task 5 这一块用 grammar，其余仍 MarkdownConfig）。代价是引入 grammar 编译流程，且 external scanner 与 MarkdownConfig 协作复杂，**不推荐**。

---

## 4. 各子项坑点与解决方案 / 降级方案

### 4.1 Task 2（f-标签边界）坑点

| 坑点 | 描述 | Spike 解决 / 折衷 |
|------|------|------------------|
| **Element 不支持 per-node props**（最大坑） | `Element` 只有 `type/from/to/children`，`NodeProp` 默认挂 NodeType 由所有同名节点共享，无法表达"每个 FTagFig 各自有不同的 id/src" | 模块级 `Map<number, Record<string, string>>` side-table，以节点 `from` 为 key |
| `prevLineEnd()` 含换行符 | 原生 `HTMLBlock` 用 `to = cx.prevLineEnd()` 会把闭合标签所在行末尾 `\n` 纳入区间 | 在 `cx.nextLine()` 之前手动 `to = cx.lineStart + closeIdx + closeTag.length` |
| TS2454 控制流告警 | `let to: number;` 在 `else` 分支通过 `found` 标志赋值，TS 无法推断所有路径赋定 | `let to = 0;` 兜底 + 注释说明实际值在分支内一定被覆盖 |
| `line.pos` 与 `line.text` 关系 | `line.pos` 是 composite 标记（如 blockquote `>`）之后的非空白字符位置；计算 `from` 用 `cx.lineStart + line.pos`，扫描 caption 用完整 `line.text` | 在代码注释中明确区分两套索引 |
| `before: 'HTMLBlock'` 必须显式 | `<f-fig ...>` 形似 HTML 块起始标签，不抢在原生 `HTMLBlock` 之前注册会被先认领走 | 4 个 BlockParser 均 `before: 'HTMLBlock'` |

### 4.2 Task 3（表格结构）坑点

| 坑点 | 描述 | Spike 解决 / 折衷 |
|------|------|------------------|
| **节点名与 @lezer/markdown 内置冲突** | 内置已有 `Table/TableHeader/TableRow/TableCell`（GFM 表格），重复注册会复用 NodeType 干扰 GFM 解析 | 所有表格结构节点统一加 `FTag` 前缀（`FTagTable/FTagTHead/...`） |
| **形态 B 无 THead/TBody** | MD 表无显式 head/body 区分，只有 `| --- |` 分隔行隐含 header | 形态 B 仅产出 `FTagTable → FTagTR → FTagTH/FTagTD` 三层，无 THead/TBody |
| **跨行 HTML 标签未实现** | `<th>...</th>` 开闭标签须在同一行（与 caption 扫描一致的 spike 限制） | 跨行标签被 `scanTableHtml` 跳过；生产需评估是否实现 |
| `<th` 前缀匹配 `<thead` | `text.indexOf('<th')` 会命中 `<thead>` 中的 `<th` | 匹配后校验下一字符必须是 空白 / `>` / `/` |
| Lezer Element children 必须构造时传入 | `cx.elt(type, from, to, children)` 的 children 不可变；无法先创建父再回填子 | `buildTableTree` 用栈 + 延迟 finalize：遇不被栈顶包含的 tag 时弹栈构造 |
| 同级 children 必须有序且不重叠 | caption 与 table 各自按文档顺序产出，合并后可能乱序 | `children.sort((a, b) => a.from - b.from)` 兜底 |
| 形态 C 优先于形态 B | 一个 `<f-tbl>` 块要么是 HTML 表要么是 MD 表，不会同时存在 | 先调 `scanTableHtml`，非空则用 HTML 结构；否则调 `scanMdTable` |

### 4.3 Task 4（脚注）坑点

| 坑点 | 描述 | Spike 解决 / 折衷 |
|------|------|------------------|
| **`peekLine()` 空行与 EOF 歧义** | `peekLine()` 对空行与 EOF 都返回 `""`，无法直接区分 | "当前行空"分支里，无论 `peeked` 是 EOF 还是真空行，只要不是缩进行就停止；EOF 情形下后续 `nextLine()` 返回 false 自然退出 |
| `prevLineEnd()` 语义 | 返回"上一行末尾"（当前行起始之前的 `\n` 位置），不是"当前行末尾" | 消费第 N 行后 `cx.lineStart` 已在第 N+1 行，`prevLineEnd()` 给的是第 N 行末尾——"消费后立即取 to"用 `prevLineEnd()` 是对的 |
| `nextLine()` 返回值 | 返回 `false` 不代表失败，代表 EOF（最后一行无尾 `\n` 时）；此时 `lineStart` 已推进到 EOF，`atEnd = true`，`line.text = ""` | "消费最后一行后取 to" 依然正确，无需特判返回值 |
| 优先级（`[^id]:` vs LinkReference） | 原生 `LinkReference` 是 leaf block parser，本扩展用 eager `parse` 认领，原生看不到该行 | `before: 'LinkReference'` 显式声明顺序 |
| 优先级（`[^id]` vs Link） | 标准 Link parser 也以 `[` 起步，不 `before: 'Link'` 会被 Link 先加分隔符 | `before: 'Link'` 先认领 `[^id]` |
| **续行仅 top-level 缩进** | `isContinuation(text)` 检查原始行文本是否以 tab 或 4 空格开头；若脚注位于 blockquote/list 内，续行形如 `>     foo` 会被误判停止 | spec 三个 Scenario 均 top-level，不影响验收；生产需用 `line.indent - line.baseIndent >= 4` 判定相对缩进 |
| side-table 模式 | 沿用 Task 2 坑点，id 无法挂节点上 | `Map<number, string>` side-table（以 `from` 为 key），与 f-标签独立 |

### 4.4 Task 5（未闭合兜底）坑点

| 坑点 | 描述 | Spike 解决 / 折衷 |
|------|------|------------------|
| **`preScanCloseTag` 用 private API hack（最大风险）** | BlockContext 公开 API 仅 `peekLine()`（看下一行）+ `nextLine()`（消费一行不可回退）；要在"不消费后续行"前提下判断后续是否存在 `</f-{type}>`，必须预扫描多行 | 用 `(cx as unknown as { to: number; lineChunkAt(pos: number): string })` 访问 private 字段 `to`（文档末尾）+ private 方法 `lineChunkAt(pos)`。**生产代码必须重新设计** |
| 起始标签 `>` 同行解读 | spec 原文"标签识别仅在同行内匹配起始标签的 `>`，跨行未闭合视为不完整"有歧义：解读 A（起始标签 `>` 跨行视为不完整）vs 解读 B（闭合标签跨行视为不完整，会破坏 Task 2 多行闭合） | 采用解读 A：起始标签 `>` 必须同行，否则 FTagUnclosed；闭合标签仍允许跨行 |
| FTagUnclosed 区间两种形态 | 同行无 `>`：区间从 `<f-{type}` 到行末；同行有 `>` 但 EOF 无闭合：区间从 `<f-{type}` 到 `>` | 两种形态 `to` 计算方式不同：前者 `cx.lineStart + line.text.length`，后者 `cx.lineStart + gtIdx + 1` |
| 重复扫描 | 预扫描确认有闭合后，进入多行闭合逻辑时仍用 `nextLine()` 重新扫描查找 `closeIdx` | 预扫描只返回布尔值（找到 / 未找到），不返回位置；且 `nextLine()` 必须调用以推进状态。代价：后续行被扫描两次（spike 5000 行 13.94ms 可接受）。生产可让 `preScanCloseTag` 返回闭合位置避免重复 |
| block 注册 | FTagUnclosed 是顶层块节点，与 FTagFig 同级 | `defineNodes` 注册 `{ name: 'FTagUnclosed', block: true }`，否则会被包裹进 Paragraph |
| 不提取属性 | 半截标签属性可能未闭合引号（如 `id="x`），`parseAttrs` 提取不完整属性语义可疑 | FTagUnclosed 是叶子块节点（无 children），不解析起始标签内属性 |

---

## 5. `ftagSyntax.ts` 行数预估校准

### 5.1 spike 实测

| 文件 | spike 实测行数 |
| --- | ---: |
| `src/ftagSyntax.ts` | 475 |
| `src/footnoteSyntax.ts` | 132 |
| **生产代码合计（spike 基线）** | **607** |

### 5.2 生产化增量预估

生产代码相对 spike 需要增加的部分：

1. **错误处理**：spike 多处 `if (gtIdx < 0)` 等单分支兜底；生产需更完整的边界检查 + 日志。
2. **类型完善**：spike 用了 `(cx as unknown as {...})` 类型断言访问 private API；生产需移除 hack，类型签名更严格。
3. **文档注释**：spike 注释偏草稿；生产需补 JSDoc / 公共 API 文档。
4. **解决 private API hack**：Task 5 的 `preScanCloseTag` 需重新设计（方案 A 预处理索引 / 方案 B 上游 API / 方案 C external scanner），新增索引数据结构与维护逻辑约 +50-100 行。
5. **跨行 HTML 标签 / 跨行 caption**：若生产决定实现，新增状态机约 +30-50 行。
6. **续行相对缩进**：`line.indent - line.baseIndent >= 4` 判定（替换 top-level `isContinuation`），约 +10 行。
7. **属性存储健壮方案**：side-table → 树外按需解析（调用方工具函数），约 +20-40 行。

预估增量系数：**1.2-1.5 倍 spike 行数**。

### 5.3 校准结论

- **方案 §4.3 原预估**：~450 行（含脚注扩展）。
- **spike 实测**：607 行（ftagSyntax.ts 475 + footnoteSyntax.ts 132）。
- **生产预估（1.2-1.5 倍）**：约 **730-910 行**。
- **校准结论**：**上调至 ~750-900 行**。spike 已证伪 ~450 的乐观预估。

### 5.4 单文件 ≤ 800 行约束

AGENTS.md 规定单文件不超过 800 行。生产代码若按 spike 单文件结构（ftagSyntax.ts 475 + Task 5 生产化增量 + 表格生产化增量）可能逼近或超过 800 行。

**建议生产代码拆分**：

- `ftagSyntax.ts`：f-标签边界 + 未闭合兜底（约 400-500 行）
- `footnoteSyntax.ts`：脚注（约 150-200 行）
- `tableStructure.ts`：表格结构（HTML + MD 表，约 250-300 行）
- `ftagAttrs.ts`：属性 side-table 与树外按需解析工具（约 50-100 行）

四文件合计 ~750-900 行，每文件 ≤ 800 行，满足约束。

---

## 6. D1 三项决策推荐值与依据

### 6.1 表格单元格级编辑是否继续

- **推荐**：**继续做**
- **依据**：
  - Task 3 证明形态 C（HTML 表）+ 形态 B（MD 表）均可识别结构节点（FTagTable/FTagTR/FTagTH/FTagTD/FTagTHContent/FTagTDContent）。
  - 单元格属性（rowspan/colspan）可通过 `tableAttrsTable` side-table 提取。
  - 单元格内容区间（THContent/TDContent）严格落在 TH/TD 区间内，不重叠，可独立提取。
  - spec 允许的"整块源码态"降级方案未启用。

### 6.2 脚注多段落定义是否本期做

- **推荐**：**本期做完整版**
- **依据**：
  - Task 4 证明 4 空格/tab 缩进的多段落续行可实现（`peekLine` + 双 `nextLine` 模式）。
  - spec 允许的"仅识别单行定义"降级方案未启用。
  - 续行段落归入 FootnoteDefinition 块，不被识别为独立 Paragraph 或 IndentedCode。
  - 已知限制：续行仅 top-level 缩进（生产需改用 `line.indent - line.baseIndent >= 4`），不影响本期验收。

### 6.3 `ftagSyntax.ts` 行数预估校准

- **推荐**：**~750-900 行**
- **依据**：
  - spike 实测 607 行（ftagSyntax.ts 475 + footnoteSyntax.ts 132）。
  - 生产化增量 1.2-1.5 倍（错误处理、类型完善、文档注释、解决 private API hack、跨行支持、相对缩进、属性健壮方案）。
  - 方案 §4.3 原预估 ~450 行被 spike 证伪。
  - 生产代码建议拆分为 4 个文件（ftagSyntax.ts + footnoteSyntax.ts + tableStructure.ts + ftagAttrs.ts）以遵守单文件 ≤ 800 行约束。

---

## 7. 生产实现风险提示

### 7.1 最大风险：Task 5 `preScanCloseTag` 用 private API hack

**问题描述**：spike 中 `preScanCloseTag` 通过 `(cx as unknown as { to: number; lineChunkAt(pos: number): string })` 访问 `BlockContext` 的 private 字段 `to`（文档末尾位置）与 private 方法 `lineChunkAt(pos)`（返回 pos 所在行文本，不含 `\n`）。

**为何不能进生产**：
- private API 无契约保证，`@lezer/markdown` 升级可能改名 / 删除 / 改语义。
- 类型断言绕过了 TS 检查，运行时若 API 变更无任何编译期提示。
- 违反封装，破坏与上游的解耦。

**候选解决方案**（按推荐度排序）：
1. **方案 A（推荐）：在 parseBlock 外做预扫描**——预处理整个文档建立"起始标签 → 闭合标签位置"索引，BlockParser 查索引决定是否认领。预扫描在每次 parse 开始时执行，O(n) 复杂度。可放在 `MarkdownConfig` 的 wrapper 层（如用 `Parser` 包装）或调用方在传入文档前先建索引。
2. **方案 B（理想但等上游）**：与 `@lezer/markdown` 上游沟通为 `BlockContext` 增加 `lineAt(pos): string` 公开 API。最干净，但需等上游发版，时间不可控。
3. **方案 C（不推荐）**：改走 `@lezer/generator` `.grammar` + external scanner，仅 Task 5 这一块用 grammar，其余仍 MarkdownConfig。代价是引入 grammar 编译流程，external scanner 与 MarkdownConfig 协作复杂。

### 7.2 side-table Map 模式（属性存储）

**问题描述**：spike 用模块级 `Map<number, Record<string, string>>`（attrsTable / tableAttrsTable / footnoteDefIdTable / footnoteRefIdTable）以节点 `from` 为 key 存属性 / id。

**为何需要更健壮方案**：
- Map 在解析重入 / 增量解析时可能累积脏数据（spike 用 `clearFtagAttrs` 在测试 `beforeEach` 清空，生产需更精细的生命周期管理）。
- `from` 位置在文档编辑后会漂移，side-table 索引失效。
- 多文档 / 多 Editor 实例共享同一模块级 Map，会串数据。

**生产候选方案**：
1. **树外按需解析**（推荐）：不在树里存属性，调用方拿到 FTagFig 节点后从原文 `doc.slice(node.from, ...)` 重新正则提取属性。最简单、零侵入、无状态。
2. `NodeProp.mounted` 挂子树：过度复杂，不推荐。
3. 改走 grammar + `NodeProp.perNode` + `deserialize`：代价大，仅在此痛点升级为 blocker 时考虑。

### 7.3 跨行 HTML 标签未实现

**问题描述**：spike 中 `scanTableHtml` 要求 `<table>/<thead>/<tbody>/<tr>/<th>/<td>` 开闭标签在同一行内，跨行标签被跳过。

**生产影响**：用户在 `<f-tbl>` 块内手写跨行 HTML 表（如 `<th>\n  内容\n</th>`）时，表格结构节点不会被识别，块内回退为整块文本。

**生产建议**：评估实际使用频率。若用户主要是粘贴 MD 表（形态 B）或单行 HTML 表（形态 C），可不实现跨行；若需要支持手写跨行 HTML 表，需扩展 `scanTableHtml` 为状态机扫描（与 caption 跨行扫描一并实现）。

### 7.4 续行仅 top-level 缩进

**问题描述**：spike 中 `isContinuation(text)` 检查原始行文本是否以 tab 或 4 空格开头，不处理 composite 上下文（blockquote/list 内的相对缩进）。

**生产影响**：脚注定义位于 blockquote/list 内时，续行形如 `>     foo` 会被误判停止，多段落续行失效。

**生产建议**：改用 `line.indent - line.baseIndent >= 4` 判定相对缩进（`line.baseIndent` 是 composite 标记后的基准缩进，`line.indent` 是实际缩进）。需在 `footnoteDefBlockParser.parse` 内替换 `isContinuation(line.text)` 调用。

### 7.5 其他次要风险

- **TS2454 控制流告警**：spike 用 `let to = 0;` 兜底，生产可改用更明确的 `let to!: number;`（definite assignment）或重构控制流。
- **`prevLineEnd()` 含换行符**：spike 手动计算 `to`，生产需在注释中明确这一偏差，避免误用 `cx.prevLineEnd()`。
- **属性值含 `>`**：spike 用 `line.text.indexOf('>')` 错切，生产需引号感知扫描器（约 +20 行）。
- **自闭合标签 `<f-fig/>`**：spike 未实现，会一路扫到文档末尾。生产需起始标签正则匹配 `/>` 时直接产出单节点（约 +10 行）。

---

## 8. D1 决策表

| 决策项 | 选项 | spike 推荐 | 依据 |
|--------|------|----------|------|
| 表格单元格级编辑是否继续 | 继续 / 回退到整块源码态 | **继续** | Task 3 证明形态 C + B 均可识别结构节点，未降级 |
| 脚注多段落定义是否本期做 | 完整 / 仅单行 | **完整** | Task 4 证明多段落续行可实现，未降级 |
| `ftagSyntax.ts` 行数预估校准 | ~450 / 上调 | **~750-900** | spike 实测 607 行 + 生产化增量 1.2-1.5 倍 |

---

## 9. 验证结果

- **`pnpm test`**：17/17 通过（1 sanity + 3 ftag-boundary + 4 table-structure + 3 footnote + 6 unclosed-tag）。
- **`pnpm exec tsc --noEmit`**：无错（strict 模式，含 `(cx as unknown as {...})` 类型断言）。
- **性能**：5000 行文档全量解析 13.94ms，< spec 目标 50ms，达标。

---

## 10. 附录：spike 文件清单

```
spike/ftag-syntax/
├── src/
│   ├── ftagSyntax.ts          # 475 行（Task 2/3/5）
│   ├── footnoteSyntax.ts      # 132 行（Task 4）
│   └── index.ts               # 1 行（占位）
├── tests/
│   ├── ftag-boundary.test.ts  # 193 行（Task 2）
│   ├── table-structure.test.ts # 200 行（Task 3）
│   ├── footnote.test.ts       # 136 行（Task 4）
│   ├── unclosed-tag.test.ts   # 233 行（Task 5）
│   └── sanity.test.ts         # 7 行（spike 环境冒烟）
├── spike-report.md            # 本文档（最终版）
├── spike-report-draft.md      # 各 Task 草稿（历史保留）
├── package.json
├── tsconfig.json
├── vitest.config.ts
├── pnpm-workspace.yaml
├── pnpm-lock.yaml
└── .npmrc
```
