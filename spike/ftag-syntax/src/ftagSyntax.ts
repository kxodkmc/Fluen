// Fluen f-标签边界识别 spike —— CodeMirror 6 / @lezer/markdown 扩展
// 路径选型：MarkdownConfig（defineNodes + parseBlock）
// 验证目标：识别 <f-fig>/<f-tbl>/<f-eq>/<f-claim> 块级标签对 + 嵌套 <f-caption>

import {
  MarkdownConfig,
  BlockContext,
  Line,
  BlockParser,
  Element,
} from '@lezer/markdown';

const FTAG_TYPES = ['fig', 'tbl', 'eq', 'claim'] as const;
type FtagType = (typeof FTAG_TYPES)[number];

const FTAG_NODE_NAMES: Record<FtagType, string> = {
  fig: 'FTagFig',
  tbl: 'FTagTbl',
  eq: 'FTagEq',
  claim: 'FTagClaim',
};

/**
 * 属性 side-table。
 *
 * 坑点：Lezer 的 Element（MarkdownConfig.parseBlock 产出的单元）不携带 per-node
 * props——NodeProp 默认挂在 NodeType 上，而 NodeType 由所有同名节点共享，无法
 * 表达“每个 FTagFig 各自有不同的 id/src”。Tree.build 虽支持 perNode props，
 * 但 Buffer.finish 路径并未把 props 透传给 Element。
 *
 * Spike 折衷：用模块级 Map 以节点 from 为 key 存属性。生产代码需重新设计
 * （方案候选：① 在树外按需从原文重新切串解析；② 用 NodeProp.mounted 挂子树；
 *   ③ 改走 @lezer/generator  grammar，借助 NodeProp.perNode + deserialize）。
 */
const attrsTable = new Map<number, Record<string, string>>();

export function getFtagAttrs(from: number): Record<string, string> | undefined {
  return attrsTable.get(from);
}

/**
 * 表格单元格属性 side-table（rowspan/colspan 等）。
 * 沿用 attrsTable 的 side-table 模式（见上方坑点 3.1），以 TH/TD 节点 from 为 key。
 */
const tableAttrsTable = new Map<number, Record<string, string>>();

export function getTableAttrs(
  from: number,
): Record<string, string> | undefined {
  return tableAttrsTable.get(from);
}

export function clearFtagAttrs(): void {
  attrsTable.clear();
  tableAttrsTable.clear();
}

/**
 * 表格结构节点名。统一加 FTag 前缀避免与 @lezer/markdown 内置节点名
 * （如 Table/TableHeader/TableRow/TableCell）冲突。
 */
const TABLE_NODE_NAMES = {
  table: 'FTagTable',
  thead: 'FTagTHead',
  tbody: 'FTagTBody',
  tr: 'FTagTR',
  th: 'FTagTH',
  td: 'FTagTD',
  thContent: 'FTagTHContent',
  tdContent: 'FTagTDContent',
} as const;

function parseAttrs(tagBody: string): Record<string, string> {
  const attrs: Record<string, string> = {};
  const attrRegex = /(\w+)\s*=\s*"([^"]*)"/g;
  let m: RegExpExecArray | null;
  while ((m = attrRegex.exec(tagBody)) !== null) {
    attrs[m[1]] = m[2];
  }
  return attrs;
}

interface LineInfo {
  start: number;
  text: string;
}

// ===== 表格结构节点识别（SubTask 3.1 / 3.2 / 3.3）=====

interface TagInstance {
  name: string;
  from: number;
  to: number;
  attrs?: Record<string, string>;
  contentFrom?: number; // TH/TD 单元格内容区间
  contentTo?: number;
}

/** HTML 标签对：[节点名, 起始标签前缀, 闭合标签]。前缀匹配后需校验下一个字符。 */
const HTML_TAG_PAIRS: Array<[string, string, string]> = [
  [TABLE_NODE_NAMES.table, '<table', '</table>'],
  [TABLE_NODE_NAMES.thead, '<thead', '</thead>'],
  [TABLE_NODE_NAMES.tbody, '<tbody', '</tbody>'],
  [TABLE_NODE_NAMES.tr, '<tr', '</tr>'],
  [TABLE_NODE_NAMES.th, '<th', '</th>'],
  [TABLE_NODE_NAMES.td, '<td', '</td>'],
];

/**
 * 形态 C：扫描块内各行，识别 HTML 表格标签对。
 * spike 限制：标签对须在同一行内闭合（与 caption 扫描一致）；跨行标签会被跳过。
 */
function scanTableHtml(lineStarts: LineInfo[]): TagInstance[] {
  const tags: TagInstance[] = [];
  for (const { start, text } of lineStarts) {
    for (const [nodeName, openTag, closeTag] of HTML_TAG_PAIRS) {
      let searchFrom = 0;
      for (;;) {
        const oIdx = text.indexOf(openTag, searchFrom);
        if (oIdx < 0) break;
        // 校验 openTag 之后是 空白/>// 之一，避免 <th 误匹配 <thead
        const after = text[oIdx + openTag.length];
        if (after !== undefined && !/[\s>\/]/.test(after)) {
          searchFrom = oIdx + 1;
          continue;
        }
        const cIdx = text.indexOf(closeTag, oIdx + openTag.length);
        if (cIdx < 0) break; // 跨行标签，spike 跳过

        const eltFrom = start + oIdx;
        const eltTo = start + cIdx + closeTag.length;

        if (
          nodeName === TABLE_NODE_NAMES.th ||
          nodeName === TABLE_NODE_NAMES.td
        ) {
          // 定位起始标签的 '>'，提取属性 + 内容区间
          const gtIdx = text.indexOf('>', oIdx);
          if (gtIdx < 0 || gtIdx >= cIdx) {
            searchFrom = cIdx + closeTag.length;
            continue;
          }
          const tagBody = text.slice(oIdx, gtIdx + 1);
          tags.push({
            name: nodeName,
            from: eltFrom,
            to: eltTo,
            attrs: parseAttrs(tagBody),
            contentFrom: start + gtIdx + 1,
            contentTo: start + cIdx,
          });
        } else {
          tags.push({ name: nodeName, from: eltFrom, to: eltTo });
        }
        searchFrom = cIdx + closeTag.length;
      }
    }
  }
  return tags;
}

/** 把扁平的 TagInstance 列表按区间包含关系构建成 Lezer Element 树。 */
function buildTableTree(
  cx: BlockContext,
  tags: TagInstance[],
): Element[] {
  // 按 from 升序、to 降序排列：外层节点先处理，同 from 时大区间在前
  tags.sort((a, b) => a.from - b.from || b.to - a.to);

  const stack: { tag: TagInstance; children: Element[] }[] = [];
  const roots: Element[] = [];

  const finalize = (frame: {
    tag: TagInstance;
    children: Element[];
  }): void => {
    const elt = makeTableElement(cx, frame.tag, frame.children);
    if (stack.length === 0) roots.push(elt);
    else stack[stack.length - 1].children.push(elt);
  };

  for (const tag of tags) {
    // 弹栈直到栈顶包含当前 tag（区间包含）
    while (stack.length > 0) {
      const top = stack[stack.length - 1].tag;
      if (top.from <= tag.from && tag.to <= top.to) break;
      finalize(stack.pop()!);
    }
    stack.push({ tag, children: [] });
  }
  while (stack.length > 0) finalize(stack.pop()!);
  return roots;
}

function makeTableElement(
  cx: BlockContext,
  tag: TagInstance,
  children: Element[],
): Element {
  if (
    tag.name === TABLE_NODE_NAMES.th ||
    tag.name === TABLE_NODE_NAMES.td
  ) {
    const contentName =
      tag.name === TABLE_NODE_NAMES.th
        ? TABLE_NODE_NAMES.thContent
        : TABLE_NODE_NAMES.tdContent;
    const contentElt = cx.elt(contentName, tag.contentFrom!, tag.contentTo!);
    if (tag.attrs && Object.keys(tag.attrs).length > 0) {
      tableAttrsTable.set(tag.from, tag.attrs);
    }
    return cx.elt(tag.name, tag.from, tag.to, [contentElt]);
  }
  return cx.elt(tag.name, tag.from, tag.to, children);
}

interface MdCell {
  from: number; // 单元格区域起止（含周围空格，不含 | 分隔符）
  to: number;
  contentFrom: number; // 去空白后内容起止
  contentTo: number;
}

/** 解析 MD 表格行 `| a | b |` 为单元格区间列表。 */
function parseMdRow(text: string): MdCell[] {
  const cells: MdCell[] = [];
  let i = 0;
  if (text[i] === '|') i++; // 跳过行首 |
  while (i < text.length) {
    let j = i;
    while (j < text.length && text[j] !== '|') j++;
    const cellText = text.slice(i, j);
    const lead = cellText.length - cellText.trimStart().length;
    const trail = cellText.length - cellText.trimEnd().length;
    cells.push({
      from: i,
      to: j,
      contentFrom: i + lead,
      contentTo: j - trail,
    });
    i = j + 1; // 跳过 |
  }
  // 移除行尾 | 产生的空单元格
  while (cells.length > 0) {
    const last = cells[cells.length - 1];
    if (last.contentFrom >= last.contentTo) cells.pop();
    else break;
  }
  return cells;
}

/**
 * 形态 B：在 <f-tbl> 块内检测 MD 表语法（| 分隔 + |---| 分隔行），
 * 产出与形态 C 等价的 FTagTable/FTagTR/FTagTH/FTagTD 结构节点。
 * 成功产出返回 true。
 */
function scanMdTable(
  cx: BlockContext,
  lineStarts: LineInfo[],
  children: Element[],
): boolean {
  // 收集连续的 pipe 行
  const pipeLines: LineInfo[] = [];
  for (const line of lineStarts) {
    const t = line.text.trim();
    const isPipe = t.startsWith('|') && t.endsWith('|') && t.length >= 2;
    if (isPipe) pipeLines.push(line);
    else if (pipeLines.length > 0) break; // 连续段结束
  }

  if (pipeLines.length < 3) return false;

  // 第 2 行必须是分隔行 | --- | --- |
  const sepRe = /^\s*\|[\s\-:|]+\|\s*$/;
  if (!sepRe.test(pipeLines[1].text)) return false;

  const trElts: Element[] = [];
  for (let r = 0; r < pipeLines.length; r++) {
    if (r === 1) continue; // 跳过分隔行
    const { start, text } = pipeLines[r];
    const isHeader = r === 0;
    const cellType = isHeader ? TABLE_NODE_NAMES.th : TABLE_NODE_NAMES.td;
    const contentType = isHeader
      ? TABLE_NODE_NAMES.thContent
      : TABLE_NODE_NAMES.tdContent;

    const cells = parseMdRow(text);
    const cellElts: Element[] = cells.map((c) =>
      cx.elt(
        cellType,
        start + c.from,
        start + c.to,
        [cx.elt(contentType, start + c.contentFrom, start + c.contentTo)],
      ),
    );
    trElts.push(
      cx.elt(TABLE_NODE_NAMES.tr, start, start + text.length, cellElts),
    );
  }

  const tableFrom = pipeLines[0].start;
  const last = pipeLines[pipeLines.length - 1];
  const tableTo = last.start + last.text.length;
  children.push(cx.elt(TABLE_NODE_NAMES.table, tableFrom, tableTo, trElts));
  return true;
}

/**
 * SubTask 5.1: 预扫描后续行查找闭合标签，不消费行。
 *
 * 坑点：BlockContext 公开 API 仅 peekLine()（看下一行）与 nextLine()（消费一行，
 * 不可回退）。要在"不消费后续行"的前提下判断后续是否存在 </f-{type}>，必须
 * 预扫描多行。spike 折衷：用 (cx as any) 访问 private 字段 `to`（文档末尾）与
 * private 方法 `lineChunkAt(pos)`（返回 pos 所在行文本，不含 \n）。生产代码需
 * 重新设计（候选：改走 @lezer/generator grammar，或在 parseBlock 外做预扫描）。
 */
function preScanCloseTag(
  cx: BlockContext,
  line: Line,
  closeTag: string,
): boolean {
  const cxAny = cx as unknown as {
    to: number;
    lineChunkAt(pos: number): string;
  };
  const docEnd: number = cxAny.to;
  // 当前行末尾（不含 \n）+ 1 = 下一行起始
  let scanFrom = cx.lineStart + line.text.length + 1;
  while (scanFrom < docEnd) {
    const lineText: string = cxAny.lineChunkAt(scanFrom);
    if (lineText.indexOf(closeTag) >= 0) return true;
    scanFrom += lineText.length + 1; // +1 跳过 \n
  }
  return false;
}

function makeFtagBlockParser(type: FtagType): BlockParser {
  const nodeName = FTAG_NODE_NAMES[type];
  // 起始标签：行首（在 line.pos 之后）出现 <f-{type} 后紧跟 空白/> 三者之一
  const openRegex = new RegExp(`^<f-${type}(?=[\\s>/])`);
  const closeTag = `</f-${type}>`;
  const captionOpen = '<f-caption>';
  const captionClose = '</f-caption>';

  return {
    name: nodeName,
    before: 'HTMLBlock', // 抢在原生 HTMLBlock 之前认领 <f-xxx>
    parse(cx: BlockContext, line: Line): boolean {
      const rest = line.text.slice(line.pos);
      if (!openRegex.test(rest)) return false;

      const from = cx.lineStart + line.pos;

      // 起始标签的 '>'（spike 假设起始标签单行、属性值内不含 '>'）
      const gtIdx = line.text.indexOf('>', line.pos);

      // ===== SubTask 5.1 / 5.2: 未闭合兜底 =====
      // 规则：起始标签的 '>' 必须在同一行，否则视为未闭合（半截标签）。
      if (gtIdx < 0) {
        // 同行无 '>'：属性值未闭合引号 / 标签未闭合 >。
        // 产出 FTagUnclosed，区间仅该行（从 <f-{type} 到行末），不消费后续行。
        const to = cx.lineStart + line.text.length;
        cx.nextLine();
        cx.addElement(cx.elt('FTagUnclosed', from, to));
        return true;
      }

      const tagBody = line.text.slice(line.pos, gtIdx + 1);
      const attrs = parseAttrs(tagBody);
      const tagEnd = cx.lineStart + gtIdx + 1; // 起始标签 '>' 之后的位置

      const closeIdx = line.text.indexOf(closeTag, gtIdx + 1);

      // 同行有 '>' 但同行无闭合标签：预扫描后续行判断是否块未闭合
      if (closeIdx < 0 && !preScanCloseTag(cx, line, closeTag)) {
        // SubTask 5.1: 后续未找到闭合标签，视为块未闭合。
        // 产出 FTagUnclosed，区间从 <f-{type} 到 '>'，不消费后续行。
        cx.nextLine();
        cx.addElement(cx.elt('FTagUnclosed', from, tagEnd));
        return true;
      }

      // 记录块内每一行的绝对起点与文本，供后续 caption 扫描使用
      const lineStarts: LineInfo[] = [
        { start: cx.lineStart, text: line.text },
      ];

      let to = 0; // 实际值在下方分支赋定，这里仅为满足 TS 控制流分析

      if (closeIdx >= 0) {
        // 闭合标签与起始标签同行
        to = cx.lineStart + closeIdx + closeTag.length;
        cx.nextLine();
      } else {
        // 多行闭合：预扫描已确认后续存在闭合标签
        let found = false;
        while (cx.nextLine()) {
          lineStarts.push({ start: cx.lineStart, text: line.text });
          const idx = line.text.indexOf(closeTag);
          if (idx >= 0) {
            to = cx.lineStart + idx + closeTag.length;
            found = true;
            break;
          }
        }
        if (!found) {
          // 不应到达（预扫描已确认有闭合），兜底防退化
          to = cx.prevLineEnd();
        } else {
          cx.nextLine(); // 跳过闭合标签所在行
        }
      }

      // 嵌套 <f-caption>...</f-caption> 识别（spike：仅处理单行 caption）
      const children: Element[] = [];
      for (const { start, text } of lineStarts) {
        let searchFrom = 0;
        for (;;) {
          const co = text.indexOf(captionOpen, searchFrom);
          if (co < 0) break;
          const cc = text.indexOf(captionClose, co + captionOpen.length);
          if (cc < 0) break; // 跨行 caption 不在 spike 范围
          const captionFrom = start + co;
          const captionTo = start + cc + captionClose.length;
          const textFrom = start + co + captionOpen.length;
          const textTo = start + cc;
          const textChild = cx.elt('FTagCaptionText', textFrom, textTo);
          children.push(
            cx.elt('FTagCaption', captionFrom, captionTo, [textChild]),
          );
          searchFrom = cc + captionClose.length;
        }
      }

      // 表格结构节点识别（仅 tbl 块）：形态 C（HTML）优先，否则尝试形态 B（MD 表）
      if (type === 'tbl') {
        const tableTags = scanTableHtml(lineStarts);
        if (tableTags.length > 0) {
          children.push(...buildTableTree(cx, tableTags));
        } else {
          scanMdTable(cx, lineStarts, children);
        }
        // caption 与 table 元素合并后按 from 排序，保证 Lezer siblings 有序
        children.sort((a, b) => a.from - b.from);
      }

      attrsTable.set(from, attrs);
      cx.addElement(cx.elt(nodeName, from, to, children));
      return true;
    },
  };
}

export const ftagExtension: MarkdownConfig = {
  defineNodes: [
    { name: 'FTagFig', block: true },
    { name: 'FTagTbl', block: true },
    { name: 'FTagEq', block: true },
    { name: 'FTagClaim', block: true },
    { name: 'FTagCaption' }, // 嵌套子节点，非 block
    { name: 'FTagCaptionText' }, // caption 内纯文本
    // 表格结构节点（Task 3）—— FTag 前缀避免与 @lezer/markdown 内置 Table 等冲突
    { name: 'FTagTable' },
    { name: 'FTagTHead' },
    { name: 'FTagTBody' },
    { name: 'FTagTR' },
    { name: 'FTagTH' },
    { name: 'FTagTD' },
    { name: 'FTagTHContent' }, // <th>单元格内容文本区间
    { name: 'FTagTDContent' }, // <td>单元格内容文本区间
    // Task 5: 未闭合起始标签兜底节点（半截标签 / 块未闭合），block 以便顶层可见
    { name: 'FTagUnclosed', block: true },
  ],
  parseBlock: FTAG_TYPES.map((t) => makeFtagBlockParser(t)),
};
