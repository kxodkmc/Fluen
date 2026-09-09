/**
 * 半预览模式的视图 Widget。
 *
 * 安全约定：
 *   - KaTeX 使用默认信任配置（trust:false），禁用 \href、\includegraphics 等；
 *     只有 {@link katexMathHtml} 的返回值允许进入 widget DOM，
 *     渲染失败时回退为转义后的纯文本原文——绝无原始用户输入直接 innerHTML。
 *   - 列表圆点 / 任务框为静态字符，无动态内容。
 */

import { WidgetType } from '@codemirror/view';
import type { EditorView } from '@codemirror/view';
import { i18nInstance } from '../../../../../../i18n';
import type { MarkdownTableModel, TableAlignment } from '../tableModel';
import { attachTableEvents } from '../tableEditing';
import { getCellRaw, setCellRaw, renderCellMath } from '../cellMath';
import { katexMathHtml } from './katexRender';

// KaTeX 渲染已抽至 katexRender.ts；此处保留同名导出以维持既有引用与测试
export { katexMathHtml };

/**
 * 数学公式 widget。默认通过原子区间交互：光标落在边缘即揭示原文编辑
 * （见 decorations.ts 的揭示判定），widget 本身不拦截任何事件。
 */
export class MathWidget extends WidgetType {
  constructor(
    readonly tex: string,
    readonly displayMode: boolean,
  ) {
    super();
  }

  override eq(other: MathWidget): boolean {
    return other.tex === this.tex && other.displayMode === this.displayMode;
  }

  override toDOM(): HTMLElement {
    const host = document.createElement('span');
    host.className = this.displayMode
      ? 'fluen-lp-math fluen-lp-math--display'
      : 'fluen-lp-math';
    const { ok, html } = katexMathHtml(this.tex, this.displayMode);
    host.innerHTML = html;
    if (!ok) host.classList.add('fluen-lp-math--invalid');
    return host;
  }
}

/** 无序列表圆点 widget（替换 `-`/`*`/`+` 标记）。 */
export class BulletWidget extends WidgetType {
  constructor(readonly nestedDepth = 0) {
    super();
  }

  override eq(other: BulletWidget): boolean {
    return other.nestedDepth === this.nestedDepth;
  }

  override toDOM(): HTMLElement {
    const el = document.createElement('span');
    el.className = 'fluen-lp-bullet';
    // 圆点随层级变化视觉重量：第一层实心、第二层空心圆、更深层小方块
    el.textContent = this.nestedDepth === 0 ? '•' : this.nestedDepth === 1 ? '◦' : '▪';
    el.setAttribute('aria-hidden', 'true');
    return el;
  }
}

/** 任务列表复选框 widget（只读视觉态；勾选状态的修改走源码编辑）。 */
export class TaskCheckboxWidget extends WidgetType {
  constructor(readonly checked: boolean) {
    super();
  }

  override eq(other: TaskCheckboxWidget): boolean {
    return other.checked === this.checked;
  }

  override toDOM(): HTMLElement {
    const el = document.createElement('span');
    el.className = 'fluen-lp-taskbox' + (this.checked ? ' fluen-lp-taskbox--on' : '');
    el.textContent = this.checked ? '☑' : '☐';
    el.setAttribute('aria-hidden', 'true');
    return el;
  }
}

/* ── 表格 widget（结构化编辑：渲染态单元格 + 行列手柄） ───────────── */

/** 按模型对齐标记设置单元格文本对齐。 */
function applyCellAlign(el: HTMLElement, align: TableAlignment): void {
  if (align) el.style.textAlign = align;
}

/** i18n 单例读取（widget 构建于编辑器挂载后，vue-i18n 已就绪）。 */
function tr(key: string): string {
  return i18nInstance.global.t(key) as string;
}

/** 构建一个手柄按钮（+ 插入 / × 删除），操作类型由 data-table-op 标记。 */
function handleButton(op: string, glyph: string, titleKey: string): HTMLButtonElement {
  const btn = document.createElement('button');
  btn.type = 'button';
  btn.className = 'fluen-lp-tbhandle__btn';
  btn.dataset.tableOp = op;
  btn.textContent = glyph;
  btn.title = tr(titleKey);
  return btn;
}

/**
 * 列手柄：悬停列上方显示（+ 在左侧插入列 / × 删除此列），
 * 附纵向指示线；`data-col` 由悬停定位时写入，供事件层读取操作索引。
 */
function buildColHandle(): HTMLDivElement {
  const box = document.createElement('div');
  box.className = 'fluen-lp-tbhandle fluen-lp-tbhandle--col';
  box.hidden = true;
  box.appendChild(handleButton('insert-col', '+', 'main.content.toolbar.handleInsertCol'));
  box.appendChild(handleButton('delete-col', '×', 'main.content.toolbar.handleDeleteCol'));
  box.appendChild(Object.assign(document.createElement('span'), { className: 'fluen-lp-tbhandle__line' }));
  return box;
}

/** 行手柄：悬停行左侧显示（+ 在上方插入行 / × 删除此行），附横向指示线。 */
function buildRowHandle(): HTMLDivElement {
  const box = document.createElement('div');
  box.className = 'fluen-lp-tbhandle fluen-lp-tbhandle--row';
  box.hidden = true;
  box.appendChild(handleButton('insert-row', '+', 'main.content.toolbar.handleInsertRow'));
  box.appendChild(handleButton('delete-row', '×', 'main.content.toolbar.handleDeleteRow'));
  box.appendChild(Object.assign(document.createElement('span'), { className: 'fluen-lp-tbhandle__line' }));
  return box;
}

/** 将手柄定位到悬停单元格所在行/列（offsetParent 即 tablebox）。 */
function showHandles(box: HTMLElement, cell: HTMLElement): void {
  const colHandle = box.querySelector<HTMLDivElement>('.fluen-lp-tbhandle--col');
  const rowHandle = box.querySelector<HTMLDivElement>('.fluen-lp-tbhandle--row');
  if (!colHandle || !rowHandle) return;

  const row = cell.parentElement as HTMLTableRowElement;
  colHandle.dataset.col = String((cell as HTMLTableCellElement).cellIndex);
  colHandle.style.left = `${cell.offsetLeft}px`;
  colHandle.style.width = `${cell.offsetWidth}px`;
  colHandle.hidden = false;

  rowHandle.dataset.row = String(row.sectionRowIndex);
  rowHandle.style.top = `${cell.offsetTop}px`;
  rowHandle.style.height = `${cell.offsetHeight}px`;
  rowHandle.hidden = false;
}

/**
 * 表格 widget：渲染为可直接编辑的结构化表格。
 *
 * Word 式交互（半预览中不揭示 Markdown 源码，源码编辑走仅源码视图）：
 *   - 单元格 contenteditable 直接输入，input 事件经 tableEditing 层写回
 *     文档（事件为 widget DOM 原生监听——CM6 会丢弃 ignoreEvent widget 内
 *     冒泡的事件，domEventHandlers 收不到）；
 *   - 悬停单元格显示行/列手柄（+ 插入 / × 删除），点击由 tableEditing 层
 *     应用 {@link tableModel} 的结构操作后整体写回；
 *   - 单元格内容一律走 textContent 读写，无 HTML 注入面。
 *
 * DOM 复用两段式：
 *   - `eq` 模型相等 → 整个 DOM 直接复用；
 *   - 模型不等（键入写回 / 撤销等）→ `updateDOM` 原地同步单元格文本并复用
 *     DOM，焦点与输入法状态得以保留——这是「编辑不闪断」的关键；行列数
 *     变化（手柄增删）时返回 false 交由 CM 重建。
 */
export class TableWidget extends WidgetType {
  constructor(readonly model: MarkdownTableModel) {
    super();
  }

  override eq(other: TableWidget): boolean {
    return JSON.stringify(other.model) === JSON.stringify(this.model);
  }

  /**
   * 结构一致时原地同步单元格文本，复用现有 DOM。
   *
   * 键入写回路径：新模型源自 DOM 读取，焦点单元格文本必然一致、不被触碰，
   * 焦点与输入法状态因此保留。仅撤销/外部变更等 DOM 与模型不一致时才会
   * 重写单元格文本（焦点单元格光标位置会重置，属可接受代价）。
   */
  override updateDOM(box: HTMLElement, _view: EditorView, from: this): boolean {
    const table = box.querySelector('table');
    if (!table) return false;
    const head = [...box.querySelectorAll<HTMLElement>('thead th')];
    const bodyRows = [...table.querySelectorAll<HTMLElement>('tbody tr')];
    if (
      head.length !== this.model.header.length ||
      bodyRows.length !== this.model.rows.length ||
      from.model.aligns.length !== this.model.aligns.length
    ) {
      return false; // 行列结构变化：交由 CM 重建
    }

    const sync = (el: HTMLElement, text: string, align: TableAlignment): void => {
      // 以原文镜像比较（渲染态 textContent 含 KaTeX 重复文本，不可直接比较）
      if (getCellRaw(el) !== text) {
        el.textContent = text;
        setCellRaw(el, text);
      }
      if (el.style.textAlign !== (align ?? '')) applyCellAlign(el, align);
      renderCellMath(el);
    };
    head.forEach((th, i) => sync(th, this.model.header[i] ?? '', this.model.aligns[i] ?? null));
    for (let r = 0; r < bodyRows.length; r++) {
      const cells = [...bodyRows[r]!.querySelectorAll<HTMLElement>('td')];
      if (cells.length !== this.model.header.length) return false;
      const row = this.model.rows[r] ?? [];
      cells.forEach((td, i) => sync(td, row[i] ?? '', this.model.aligns[i] ?? null));
    }
    box.dataset.aligns = JSON.stringify(this.model.aligns);
    return true;
  }

  /** 事件全部由原生监听处理（见 tableEditing.ts；CM 不感知 widget 内事件）。 */
  override ignoreEvent(): boolean {
    return true;
  }

  override toDOM(): HTMLElement {
    const box = document.createElement('div');
    box.className = 'fluen-lp-tablebox';
    box.dataset.aligns = JSON.stringify(this.model.aligns);
    box.appendChild(this.buildTable());
    box.appendChild(buildColHandle());
    box.appendChild(buildRowHandle());
    attachTableEvents(box);

    box.addEventListener('mouseover', (e) => {
      const cell = (e.target as HTMLElement).closest('th,td');
      if (cell instanceof HTMLElement && box.contains(cell)) showHandles(box, cell);
    });
    box.addEventListener('mouseleave', () => {
      const col = box.querySelector<HTMLDivElement>('.fluen-lp-tbhandle--col');
      const row = box.querySelector<HTMLDivElement>('.fluen-lp-tbhandle--row');
      if (col) col.hidden = true;
      if (row) row.hidden = true;
    });
    return box;
  }

  /** 渲染三线表 DOM：单元格内容为 textContent，可编辑；含公式的单元格失焦渲染。 */
  private buildTable(): HTMLTableElement {
    const table = document.createElement('table');
    table.className = 'fluen-lp-table';

    const thead = table.createTHead();
    const headRow = thead.insertRow();
    this.model.header.forEach((cell, i) => {
      const th = document.createElement('th');
      th.textContent = cell;
      th.setAttribute('contenteditable', 'plaintext-only');
      applyCellAlign(th, this.model.aligns[i] ?? null);
      setCellRaw(th, cell);
      headRow.appendChild(th);
    });

    const tbody = table.createTBody();
    for (const row of this.model.rows) {
      const tr = tbody.insertRow();
      this.model.header.forEach((_, i) => {
        const td = document.createElement('td');
        td.textContent = row[i] ?? '';
        td.setAttribute('contenteditable', 'plaintext-only');
        applyCellAlign(td, this.model.aligns[i] ?? null);
        setCellRaw(td, row[i] ?? '');
        tr.appendChild(td);
      });
    }
    // 构建完成后统一渲染公式（构建期单元格必然未聚焦）
    for (const cell of table.querySelectorAll<HTMLElement>('th,td')) {
      renderCellMath(cell);
    }
    return table;
  }
}

// ===== 单元测试（守卫：仅在 vitest 环境运行，生产构建被 define 树摇） =====

if (import.meta.vitest) {
  const { describe, it, expect } = import.meta.vitest;

  describe('widgets: katexMathHtml 安全渲染', () => {
    it('普通公式正常渲染', () => {
      const r = katexMathHtml('E=mc^2', false);
      expect(r.ok).toBe(true);
      expect(r.html).toContain('katex');
    });

    it('\\href 在 trust:false 下不可产生超链接', () => {
      const r = katexMathHtml(String.raw`\href{https://evil.example}{x}`, false);
      // 要么整体失败回退纯文本，要么成功但不含 href 属性——两者都安全
      if (r.ok) {
        expect(r.html).not.toMatch(/href\s*=/);
      }
    });

    it('HTML 注入 payload 不产生可执行标签', () => {
      const r = katexMathHtml('<img src=x onerror=alert(1)>', false);
      // KaTeX 将整串当作文本排版：不得出现真实 <img> 标签或 onerror 属性形态
      expect(r.html.toLowerCase()).not.toMatch(/<img/i);
      expect(r.html.toLowerCase()).not.toMatch(/<[a-z][^>]*\sonerror/i);
    });

    it('脚本注入 payload 被转义', () => {
      const r = katexMathHtml('<script>alert(1)</script>', false);
      expect(r.html.toLowerCase()).not.toMatch(/<script/i);
    });

    it('引号注入被转义为实体', () => {
      const r = katexMathHtml(String.raw`\text{" onload="alert(1)" x="`, false);
      expect(r.html).not.toContain('" onload');
    });
  });
}
