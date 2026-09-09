/**
 * f-tbl 块内 Markdown 表格结构识别。
 *
 * 在 `ftagSyntax` 的 tbl 块解析中调用：扫描块内连续的 Markdown 表格行，
 * 产出结构化 Lezer 节点（FTagTable / FTagTHead / FTagTBody / FTagTR / FTagTH /
 * FTagTD / FTagTHContent / FTagTDContent），供语法高亮与后续渲染消费。
 *
 * 识别规则（Markdown 表格子集）：
 *   - 行以 `|` 开头（允许前导空白）
 *   - 第二行为分隔符行（`|---|---|` 或 `|:--|:-:|` 等），存在则区分 thead/tbody
 *   - 其余行为数据行
 *
 * 节点区间均为绝对文档偏移；单元格内容区间排除 `|` 与前后空白。
 */

import type { BlockContext, Element } from '@lezer/markdown';

/** 块内一行：绝对起点 + 文本。 */
export interface LineInfo {
  start: number;
  text: string;
}

/** 表格行：单元格文本数组 + 行绝对起点 + 行文本。 */
interface TableRow {
  start: number;
  text: string;
  cells: string[];
}

const CELL_SPLIT = /\|/;

/**
 * 判断一行是否为表格分隔符行（如 `|---|:--:|---|`）。
 * 仅当含至少一个 `-` 且无 `|` 之外的字母数字内容时为 true。
 */
function isSeparatorRow(text: string): boolean {
  const stripped = text.replace(/\s/g, '');
  if (!stripped) return false;
  // 仅由 |、-、: 组成，且至少含一个 -
  return /^[|:\-]+$/.test(stripped) && stripped.includes('-');
}

/**
 * 将一行解析为单元格数组。仅处理首尾 `|` 包裹的标准 Markdown 表格行。
 * 返回 null 表示该行不是表格行。
 */
function parseTableRow(line: LineInfo): TableRow | null {
  const text = line.text;
  // 允许前导空白，但行必须含 |
  const trimmed = text.trimStart();
  if (!trimmed.startsWith('|')) return null;
  // 去掉首尾 | 后按 | 切分
  let inner = trimmed;
  if (inner.endsWith('|')) {
    inner = inner.slice(0, -1);
  }
  inner = inner.startsWith('|') ? inner.slice(1) : inner;
  const cells = inner.split(CELL_SPLIT).map((c) => c.trim());
  return {
    start: line.start,
    text,
    cells,
  };
}

/**
 * 扫描 tbl 块内的表格结构，将结构化节点追加到 `children`。
 *
 * 仅识别连续的表格行；遇到非表格行即终止扫描。若无分隔符行，所有行视为 tbody。
 *
 * @param cx          BlockContext（用于 `cx.elt` 构建节点）
 * @param lineStarts  块内所有行（绝对起点 + 文本）
 * @param children    追加目标（调用方负责后续排序）
 */
export function scanTableStructure(
  cx: BlockContext,
  lineStarts: LineInfo[],
  children: Element[],
): void {
  if (lineStarts.length === 0) return;

  // 跳过块内前导非表格行（<f-tbl> 起始标签行、<f-caption> 题注行、空行等）
  let i = 0;
  while (i < lineStarts.length && !parseTableRow(lineStarts[i])) i++;

  // 解析连续表格行段（遇到非表格行即终止）
  const rows: TableRow[] = [];
  while (i < lineStarts.length) {
    const row = parseTableRow(lineStarts[i]);
    if (!row) break;
    rows.push(row);
    i++;
  }
  if (rows.length === 0) return;

  // 表格整体区间：首行起点 → 末行末尾
  const tableFrom = rows[0].start;
  const tableTo = rows[rows.length - 1].start + rows[rows.length - 1].text.length;

  // 识别分隔符行位置（通常为第二行）
  let separatorIdx = -1;
  for (let r = 0; r < rows.length; r++) {
    if (isSeparatorRow(rows[r].text)) {
      separatorIdx = r;
      break;
    }
  }

  const tableChildren: Element[] = [];

  if (separatorIdx > 0) {
    // 有 thead：分隔符前的行为表头，分隔符后的行为表体
    const headRows = rows.slice(0, separatorIdx);
    const bodyRows = rows.slice(separatorIdx + 1);

    if (headRows.length > 0) {
      const theadEl = buildRowsElement(cx, headRows, 'FTagTHead', true);
      if (theadEl) tableChildren.push(theadEl);
    }
    if (bodyRows.length > 0) {
      const tbodyEl = buildRowsElement(cx, bodyRows, 'FTagTBody', false);
      if (tbodyEl) tableChildren.push(tbodyEl);
    }
  } else {
    // 无分隔符：全部为 tbody
    const tbodyEl = buildRowsElement(cx, rows, 'FTagTBody', false);
    if (tbodyEl) tableChildren.push(tbodyEl);
  }

  if (tableChildren.length > 0) {
    children.push(cx.elt('FTagTable', tableFrom, tableTo, tableChildren));
  }
}

/**
 * 构建一组行（thead 或 tbody）的节点，含每行 TR 与单元格 TH/TD。
 *
 * @param cx          BlockContext
 * @param rows        该段的行
 * @param sectionName 'FTagTHead' 或 'FTagTBody'
 * @param isHead      true 则单元格为 FTagTH/FTagTHContent，否则 FTagTD/FTagTDContent
 */
function buildRowsElement(
  cx: BlockContext,
  rows: TableRow[],
  sectionName: 'FTagTHead' | 'FTagTBody',
  isHead: boolean,
): Element | null {
  if (rows.length === 0) return null;
  const sectionFrom = rows[0].start;
  const sectionTo = rows[rows.length - 1].start + rows[rows.length - 1].text.length;
  const cellNodeName = isHead ? 'FTagTH' : 'FTagTD';
  const contentNodeName = isHead ? 'FTagTHContent' : 'FTagTDContent';

  const trEls: Element[] = [];
  for (const row of rows) {
    const trFrom = row.start;
    const trTo = row.start + row.text.length;
    const cellEls: Element[] = buildCells(cx, row, cellNodeName, contentNodeName);
    trEls.push(cx.elt('FTagTR', trFrom, trTo, cellEls));
  }

  return cx.elt(sectionName, sectionFrom, sectionTo, trEls);
}

/**
 * 为一行构建单元格节点数组。
 *
 * 单元格区间按 `|` 分隔符在原行文本中定位；内容区间为单元格文本去除前后空白后的范围。
 */
function buildCells(
  cx: BlockContext,
  row: TableRow,
  cellNodeName: string,
  contentNodeName: string,
): Element[] {
  const cells: Element[] = [];
  const text = row.text;
  // 逐个 | 定位单元格
  let searchFrom = 0;
  let cellIdx = 0;
  while (searchFrom < text.length) {
    const barIdx = text.indexOf('|', searchFrom);
    if (barIdx < 0) break;
    const nextBar = text.indexOf('|', barIdx + 1);
    const cellEnd = nextBar < 0 ? text.length : nextBar;
    // 单元格文本区间（含前后空白）
    const cellAbsFrom = row.start + barIdx + 1;
    const cellAbsTo = row.start + cellEnd;
    if (cellAbsTo <= cellAbsFrom) {
      searchFrom = barIdx + 1;
      continue;
    }
    // 内容区间：去前后空白（全空白时两侧相抵，收敛为零宽区间，
    // 防止产出 from > to 的逆序元素损坏语法树）
    const raw = text.slice(barIdx + 1, cellEnd);
    const leading = raw.length - raw.trimStart().length;
    const trailing = raw.length - raw.trimEnd().length;
    let contentAbsFrom = cellAbsFrom + leading;
    let contentAbsTo = cellAbsTo - trailing;
    if (contentAbsTo < contentAbsFrom) {
      contentAbsFrom = contentAbsTo = cellAbsTo;
    }

    const contentEl = cx.elt(contentNodeName, contentAbsFrom, contentAbsTo);
    cells.push(cx.elt(cellNodeName, cellAbsFrom, cellAbsTo, [contentEl]));
    searchFrom = nextBar < 0 ? text.length : nextBar;
    cellIdx++;
  }
  // 过滤空单元格（行首/行尾的 | 产生的空区间）
  return cells.filter((c) => c.to > c.from);
}
