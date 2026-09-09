/**
 * 表格单元格的公式支持（DOM 层，无 CM 依赖）。
 *
 * 半预览表格为 contenteditable widget：单元格编辑态必须保持原文
 * （`$...$` 纯文本可直接输入/组词），失焦态渲染 KaTeX。原文与渲染态
 * 的桥接约定：
 *   - `data-raw`：单元格原文（编辑态 textContent 的镜像）。渲染态的
 *     textContent 含 KaTeX MathML 重复文本，不可作为模型来源；
 *     模型读取（tableEditing）一律经 {@link getCellRaw}。
 *   - `data-rendered`：已渲染态的原文指纹，避免每次模型同步都重建 KaTeX。
 *
 * 生命周期（事件接线见 tableEditing.attachTableEvents）：
 *   focusin → {@link restoreCellRaw}（还原原文供编辑）
 *   input   → {@link setCellRaw}（镜像编辑态）
 *   focusout / 初始构建 / 模型同步 → {@link renderCellMath}
 */

import { isValidMathInterior } from './livePreview/mathSyntax';
import { katexMathHtml } from './livePreview/katexRender';

/** 单元格内容段：普通文本或行内公式。 */
export interface CellSegment {
  kind: 'text' | 'math';
  value: string;
}

/** 读取单元格原文（渲染态下 textContent 不可信，优先 data-raw）。 */
export function getCellRaw(cell: HTMLElement): string {
  return cell.dataset.raw ?? cell.textContent ?? '';
}

/** 写入单元格原文镜像。 */
export function setCellRaw(cell: HTMLElement, raw: string): void {
  cell.dataset.raw = raw;
}

/** 聚焦编辑前还原原文（渲染态 → 编辑态）。 */
export function restoreCellRaw(cell: HTMLElement): void {
  const raw = cell.dataset.raw;
  if (raw !== undefined && cell.textContent !== raw) {
    cell.textContent = raw;
  }
}

/**
 * 解析单元格原文为 text/math 段序列。
 *
 * 识别未转义的 `$...$`（内容经 isValidMathInterior 校验，与正文行内式
 * 同一口径）；`\$` 视为字面量。
 */
export function parseCellSegments(raw: string): CellSegment[] {
  const segments: CellSegment[] = [];
  let text = '';
  let i = 0;
  const pushText = (): void => {
    if (text) {
      segments.push({ kind: 'text', value: text });
      text = '';
    }
  };
  while (i < raw.length) {
    if (raw[i] === '\\' && raw[i + 1] === '$') {
      text += '$';
      i += 2;
      continue;
    }
    if (raw[i] === '$') {
      const close = findUnescapedDollar(raw, i + 1);
      if (close > i + 1 && isValidMathInterior(raw.slice(i + 1, close))) {
        pushText();
        segments.push({ kind: 'math', value: raw.slice(i + 1, close) });
        i = close + 1;
        continue;
      }
    }
    text += raw[i];
    i++;
  }
  pushText();
  return segments;
}

/** 从 `from` 起查找下一个未转义的 `$`，未找到返回 -1。 */
function findUnescapedDollar(raw: string, from: number): number {
  let i = from;
  while (i < raw.length) {
    if (raw[i] === '\\' && raw[i + 1] === '$') {
      i += 2;
      continue;
    }
    if (raw[i] === '$') return i;
    i++;
  }
  return -1;
}

/**
 * 将单元格渲染为公式排版态（仅未聚焦时执行；聚焦态保持原文供编辑）。
 * 无公式段时为无操作。`data-rendered` 指纹命中时跳过重建。
 */
export function renderCellMath(cell: HTMLElement): void {
  if (cell === document.activeElement) return;
  const raw = getCellRaw(cell);
  const hasMath = raw.includes('$');
  if (!hasMath) {
    if (cell.dataset.rendered !== undefined) {
      delete cell.dataset.rendered;
      if (cell.textContent !== raw) cell.textContent = raw;
    }
    return;
  }
  if (cell.dataset.rendered === raw) return;

  const frag = document.createDocumentFragment();
  for (const seg of parseCellSegments(raw)) {
    if (seg.kind === 'text') {
      frag.appendChild(document.createTextNode(seg.value));
      continue;
    }
    const span = document.createElement('span');
    span.className = 'fluen-lp-cell-math';
    const { ok, html } = katexMathHtml(seg.value, false);
    span.innerHTML = html;
    if (!ok) span.classList.add('fluen-lp-cell-math--invalid');
    frag.appendChild(span);
  }
  cell.replaceChildren(frag);
  cell.dataset.rendered = raw;
}

/**
 * 在单元格光标处插入文本（沿用现有 DOM 选区），插入后更新原文镜像。
 * 单元格未聚焦或选区不在其中时返回 false。
 */
export function insertCellText(cell: HTMLElement, text: string): boolean {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return false;
  const range = sel.getRangeAt(0);
  if (!cell.contains(range.commonAncestorContainer)) return false;

  range.deleteContents();
  const node = document.createTextNode(text);
  range.insertNode(node);
  range.setStartAfter(node);
  range.collapse(true);
  sel.removeAllRanges();
  sel.addRange(range);
  setCellRaw(cell, cell.textContent ?? '');
  return true;
}

/**
 * 用标记包裹单元格当前选区（如 `$` → `$选区$`）；无选区时插入空标记对
 * 并将光标置于中间。操作后更新原文镜像。选区不在单元格内时返回 false。
 */
export function wrapCellSelection(cell: HTMLElement, marker: string): boolean {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return false;
  const range = sel.getRangeAt(0);
  if (!cell.contains(range.commonAncestorContainer)) return false;

  const selected = range.toString();
  if (selected.length > 0) {
    const node = document.createTextNode(`${marker}${selected}${marker}`);
    range.deleteContents();
    range.insertNode(node);
    // 选区收缩到内部文本（便于连续编辑）
    range.setStart(node, marker.length);
    range.setEnd(node, node.length - marker.length);
  } else {
    const node = document.createTextNode(marker + marker);
    range.insertNode(node);
    range.setStart(node, marker.length);
    range.setEnd(node, marker.length);
  }
  sel.removeAllRanges();
  sel.addRange(range);
  setCellRaw(cell, cell.textContent ?? '');
  return true;
}

// ===== 单元测试（守卫：仅在 vitest 环境运行，生产构建被 define 树摇）=====

if (import.meta.vitest) {
  const { describe, it, expect } = await import('vitest');

  describe('cellMath: parseCellSegments', () => {
    it('纯文本无公式段', () => {
      expect(parseCellSegments('普通文本')).toEqual([{ kind: 'text', value: '普通文本' }]);
    });

    it('识别单个行内公式段', () => {
      expect(parseCellSegments('速率 $v=at$ 米/秒')).toEqual([
        { kind: 'text', value: '速率 ' },
        { kind: 'math', value: 'v=at' },
        { kind: 'text', value: ' 米/秒' },
      ]);
    });

    it('多个公式段各自识别', () => {
      const segs = parseCellSegments('$a$ 与 $b^2$');
      expect(segs).toEqual([
        { kind: 'math', value: 'a' },
        { kind: 'text', value: ' 与 ' },
        { kind: 'math', value: 'b^2' },
      ]);
    });

    it('转义 \\$ 视为字面量', () => {
      expect(parseCellSegments('成本 \\$5')).toEqual([{ kind: 'text', value: '成本 $5' }]);
    });

    it('首尾空白的伪公式不识别（防价格误判）', () => {
      expect(parseCellSegments('$5 到 $6')).toEqual([
        { kind: 'text', value: '$5 到 $6' },
      ]);
    });

    it('内部空格为合法公式', () => {
      expect(parseCellSegments('$a b$')).toEqual([{ kind: 'math', value: 'a b' }]);
    });

    it('空公式不识别', () => {
      expect(parseCellSegments('$$')).toEqual([{ kind: 'text', value: '$$' }]);
    });
  });
}
