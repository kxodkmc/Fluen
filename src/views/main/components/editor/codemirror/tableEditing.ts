/**
 * 表格渲染态交互层：widget DOM 事件 → 模型 → 文档写回。
 *
 * 事件为何经原生监听而非 CM6 事件系统：TableWidget.ignoreEvent 为 true
 * （禁止 CM 把 widget 内部点击映射为文档光标），而 CM6 对 widget DOM 内
 * 冒泡的事件一律直接丢弃（eventBelongsToEditor → ignoreEvent），连
 * domEventHandlers 注册的处理器也不会执行。因此所有交互监听必须由
 * widget 的 toDOM 直接挂在表格容器上（见 widgets.ts）。
 *
 * 写回协议：
 *   - 单元格输入（input）→ 读取 DOM 规范模型 → 序列化 → 整表替换文档区间；
 *     文档与 DOM 已一致时跳过（不产生空事务）。
 *   - IME 组合期间冻结写回（组合态 DOM 含未定文本，写回会重建 widget 打断
 *     组词）；compositionend 后统一提交。注意 CM 的 view.composing 只跟踪
 *     contentDOM 的组合，不覆盖 widget 内的 contenteditable，因此本层自持
 *     组合状态。
 *   - 行列手柄操作 → tableModel 纯函数出新模型 → 整表写回。
 */

import { syntaxTree } from '@codemirror/language';
import { EditorView } from '@codemirror/view';
import type { SyntaxNode } from '@lezer/common';
import {
  deleteTableColumn,
  deleteTableRow,
  insertTableColumn,
  insertTableRow,
  serializeMarkdownTable,
  type MarkdownTableModel,
} from './tableModel';
import { getCellRaw, setCellRaw, restoreCellRaw, renderCellMath } from './cellMath';

/** 事件目标所在表格 widget 容器；不在表格内时为 null。 */
function tableBoxFrom(target: EventTarget | null): HTMLElement | null {
  const el = target as HTMLElement | null;
  const box = el?.closest?.('.fluen-lp-tablebox');
  return box instanceof HTMLElement ? box : null;
}

/** 解析事件所属的编辑器视图（widget DOM 已在编辑器内时）。 */
export function viewFromBox(box: HTMLElement): EditorView | null {
  const root = box.closest('.cm-editor');
  return root instanceof HTMLElement ? EditorView.findFromDOM(root) : null;
}

/** 从 widget DOM 读取规范模型（单元格读原文镜像，换行折叠为空格）。 */
function readModelFromBox(box: HTMLElement): MarkdownTableModel {
  const table = box.querySelector('table');
  const cellText = (el: Element | null): string =>
    getCellRaw(el as HTMLElement).replace(/\s*\n\s*/g, ' ').trim();
  const header = [...box.querySelectorAll('thead th')].map(cellText);
  const rows = [...(table?.querySelectorAll('tbody tr') ?? [])].map((tr) =>
    [...tr.querySelectorAll('td')].map(cellText),
  );
  let aligns: MarkdownTableModel['aligns'] = [];
  try {
    aligns = JSON.parse(box.dataset.aligns ?? '[]') as MarkdownTableModel['aligns'];
  } catch {
    aligns = [];
  }
  return { header, aligns, rows };
}

/** 定位表格 widget 对应的当前文档区间（posAtDOM + 语法树上溯）。 */
function locateTableRange(
  view: EditorView,
  box: HTMLElement,
): { from: number; to: number } | null {
  const table = box.querySelector('table');
  if (!table) return null;
  let pos: number;
  try {
    pos = view.posAtDOM(table, 0);
  } catch {
    return null; // widget 已脱离文档（重建竞态），忽略本次同步
  }
  // 裸 GFM 表为 Table；<f-tbl> 内嵌表为 tableStructure 产出的 FTagTable。
  // side=1：表格起点处向内下钻（side=-1 会落在外层 FTagTbl 上，上溯永远
  // 到不了 FTagTable，写回静默失败）
  let node: SyntaxNode | null = syntaxTree(view.state).resolveInner(pos, 1);
  while (node && node.name !== 'Table' && node.name !== 'FTagTable') node = node.parent;
  if (!node) return null;
  return { from: node.from, to: node.to };
}

/**
 * 将 DOM 编辑态写回文档。文档已与 DOM 一致时跳过（不产生空事务）。
 * 调用方保证非 IME 组合期（组合期间写回会打断组词）。
 * 导出供公式插入层（mathEditing）在单元格 DOM 操作后同步文档。
 */
export function syncTableFromBox(view: EditorView, box: HTMLElement): boolean {
  const range = locateTableRange(view, box);
  if (!range) return false;
  const insert = serializeMarkdownTable(readModelFromBox(box));
  if (view.state.sliceDoc(range.from, range.to) === insert) return false;
  view.dispatch({ changes: { from: range.from, to: range.to, insert } });
  return true;
}

/** 应用行列手柄操作（data-table-op + 手柄上的行/列索引）后写回。 */
function applyTableHandleOp(view: EditorView, target: HTMLElement): boolean {
  const btn = target.closest('[data-table-op]');
  if (!(btn instanceof HTMLElement)) return false;
  const box = tableBoxFrom(btn);
  if (!box) return false;

  const op = btn.dataset.tableOp;
  let model = readModelFromBox(box);
  if (op === 'insert-col') {
    model = insertTableColumn(model, Number(btn.parentElement?.dataset.col ?? '0'));
  } else if (op === 'delete-col') {
    model = deleteTableColumn(model, Number(btn.parentElement?.dataset.col ?? '0'));
  } else if (op === 'insert-row') {
    model = insertTableRow(model, Number(btn.parentElement?.dataset.row ?? '0'));
  } else if (op === 'delete-row') {
    model = deleteTableRow(model, Number(btn.parentElement?.dataset.row ?? '0'));
  } else {
    return false;
  }

  const range = locateTableRange(view, box);
  if (!range) return false;
  const insert = serializeMarkdownTable(model);
  if (view.state.sliceDoc(range.from, range.to) === insert) return true; // 操作被边界护栏拒绝（如删至最后一列）
  view.dispatch({ changes: { from: range.from, to: range.to, insert } });
  return true;
}

/** 单元格内 Enter 收敛为单行（换行会破坏表格结构）；Tab 在单元格间跳转。 */
function onTableKeydown(event: KeyboardEvent): boolean {
  const box = tableBoxFrom(event.target);
  if (!box) return false;
  if (event.key === 'Enter') {
    event.preventDefault();
    return true;
  }
  if (event.key === 'Tab') {
    const cells = [...box.querySelectorAll<HTMLElement>('th,td')];
    const index = cells.indexOf(event.target as HTMLElement);
    if (index === -1) return false;
    event.preventDefault();
    cells[event.shiftKey ? Math.max(index - 1, 0) : Math.min(index + 1, cells.length - 1)]?.focus();
    return true;
  }
  return false;
}

/** 单元格公式渲染的生命周期事件（focusin 还原原文 / focusout 渲染公式）。 */
function onCellFocusChange(event: FocusEvent, box: HTMLElement): void {
  const target = event.target;
  if (!(target instanceof HTMLElement)) return;
  const cell = target.closest('th,td');
  if (!(cell instanceof HTMLElement) || !box.contains(cell)) return;
  if (event.type === 'focusin') {
    restoreCellRaw(cell);
  } else {
    renderCellMath(cell);
  }
}

/**
 * 为表格 widget 容器挂载全部交互监听（由 TableWidget.toDOM 调用）。
 *
 * 输入/组合事件由本层自持的 IME 状态守护：组合期间 input 被吞掉，
 * compositionend 时统一写回一次。
 * 单元格公式（cellMath）：编辑态保持原文，失焦渲染 KaTeX，原文镜像
 * 经 data-raw 维护（模型读取不依赖渲染态 DOM）。
 */
export function attachTableEvents(box: HTMLElement): void {
  let composing = false;

  box.addEventListener('compositionstart', () => {
    composing = true;
  });
  box.addEventListener('compositionend', () => {
    composing = false;
    const view = viewFromBox(box);
    if (view) syncTableFromBox(view, box);
  });
  box.addEventListener('input', (event) => {
    if (composing) return;
    // 编辑态原文镜像先行更新（模型读取依赖 data-raw）
    const target = event.target;
    if (target instanceof HTMLElement) {
      const cell = target.closest('th,td');
      if (cell instanceof HTMLElement) setCellRaw(cell, cell.textContent ?? '');
    }
    const view = viewFromBox(box);
    if (view) syncTableFromBox(view, box);
  });
  box.addEventListener('focusin', (event) => onCellFocusChange(event, box));
  box.addEventListener('focusout', (event) => onCellFocusChange(event, box));
  box.addEventListener('mousedown', (event) => {
    const target = event.target;
    if (!(target instanceof HTMLElement) || !target.closest('[data-table-op]')) return;
    const view = viewFromBox(box);
    if (view && applyTableHandleOp(view, target)) event.preventDefault();
  });
  box.addEventListener('keydown', (event) => {
    onTableKeydown(event);
  });
}
