/**
 * 公式插入统一入口（单元格感知 + CM 感知）。
 *
 * 插入目标的自动判定：
 *   - 焦点位于表格单元格（contenteditable widget）→ 走 DOM 层操作
 *     （cellMath），经 tableEditing 写回文档——CM 选区不在单元格内，
 *     常规 CM 命令无法触达（表格内嵌公式插入的唯一通路）；
 *   - 否则 → 走 CM 文档层：光标处于数学上下文（InlineMath 内或 `$$`
 *     行）时直接插入 LaTeX 片段，否则以 `$...$` 包裹插入。
 */

import type { EditorView } from '@codemirror/view';
import { syntaxTree } from '@codemirror/language';
import type { SyntaxNode } from '@lezer/common';
import { insertCellText, wrapCellSelection } from './cellMath';
import { syncTableFromBox, viewFromBox } from './tableEditing';

/** 当前焦点所在的表格单元格；不在表格内时为 null。 */
export function focusedTableCell(): HTMLElement | null {
  const el = document.activeElement as HTMLElement | null;
  const cell = el?.closest?.('th,td');
  if (!(cell instanceof HTMLElement)) return null;
  const box = cell.closest('.fluen-lp-tablebox');
  return box instanceof HTMLElement ? cell : null;
}

/** 单元格操作后同步写回文档（view 定位失败时静默放弃）。 */
function syncCell(cell: HTMLElement): void {
  const box = cell.closest('.fluen-lp-tablebox');
  if (!(box instanceof HTMLElement)) return;
  const view = viewFromBox(box);
  if (view) syncTableFromBox(view, box);
}

/** 单元格内切换行内公式（`$` 包裹/取消）；焦点不在单元格时返回 false。 */
export function toggleCellInlineMath(): boolean {
  const cell = focusedTableCell();
  if (!cell) return false;
  if (!wrapCellSelection(cell, '$')) return false;
  syncCell(cell);
  return true;
}

/** 单元格光标处插入 LaTeX 片段；焦点不在单元格时返回 false。 */
export function insertCellMathSnippet(latex: string): boolean {
  const cell = focusedTableCell();
  if (!cell) return false;
  if (!insertCellText(cell, latex)) return false;
  syncCell(cell);
  return true;
}

/** 光标是否处于数学上下文（InlineMath 节点内，或 `$$` 块式行内）。 */
function isMathContext(view: EditorView, pos: number): boolean {
  let node: SyntaxNode | null = syntaxTree(view.state).resolveInner(pos, -1);
  while (node) {
    if (node.name === 'InlineMath') return true;
    node = node.parent;
  }
  const line = view.state.doc.lineAt(pos);
  return line.text.trimStart().startsWith('$$');
}

/**
 * CM 文档层插入 LaTeX 片段：数学上下文内直接插入，否则以 `$...$`
 * 包裹（选区内容作为公式体）。返回 true 表示已处理。
 */
export function insertMathAtCursor(view: EditorView, latex: string): boolean {
  const { state } = view;
  const sel = state.selection.main;
  const insert = isMathContext(view, sel.head) ? ` ${latex} ` : `$${latex}$`;
  view.dispatch({
    changes: { from: sel.from, to: sel.to, insert },
    selection: { anchor: sel.from + insert.length },
    scrollIntoView: true,
  });
  view.focus();
  return true;
}
