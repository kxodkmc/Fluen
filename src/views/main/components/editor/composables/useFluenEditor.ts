/**
 * Fluen 编辑器状态管理 composable（单例模式）。
 *
 * 编排 CodeMirror 6 实例与 IPC 保存流程，是编辑器模块的前端核心：
 *   - 持有唯一的 CM6 `EditorView` 实例
 *   - 维护 dirty / saving / undo / redo 等响应式状态
 *   - 通过 `useProject().saveContent()` 完成原子保存
 *   - 暴露大纲联动钩子（`scrollToLine` / `onActiveLineChange` / `onDocChange`）
 *
 * CM6 是编辑器内容的唯一数据源（source of truth），保存后端返回的归一化
 * `main_md` 会回写进编辑器以同步行尾等差异。预览为只读派生产物，由独立的
 * `FluenPreview` 组件订阅 `onDocChange` 渲染。
 *
 * @example
 * ```ts
 * const { mount, unmount, save, isDirty, scrollToLine } = useFluenEditor();
 * mount(hostEl, initialMd);
 * // ... 用户编辑 ...
 * if (isDirty.value) await save();
 * ```
 */

import { ref, readonly } from 'vue';
import { EditorView } from '@codemirror/view';
import type { ChangeSpec } from '@codemirror/state';
import { syntaxTree } from '@codemirror/language';
import { undo, redo, undoDepth, redoDepth } from '@codemirror/commands';
import { createEditorState, createEditorView, type EditorCallbacks } from '../codemirror/setup';
import { applyMarkdownFormat, type MarkdownFormatKind, type HeadingLevel } from '../codemirror/formatting';
import { insertBlockSpec } from '../codemirror/blockInsert';
import { buildTableBlock, type TableSyntax } from '../codemirror/tableModel';
import { toggleCellInlineMath, insertCellMathSnippet, insertMathAtCursor, focusedTableCell } from '../codemirror/mathEditing';
import { setLivePreviewEffect } from '../codemirror/livePreview';
import { useProject } from '../../../../../composables/useProject';

// ── 模块级状态（单例） ──────────────────────────────────────────────

/** CM6 视图实例。未挂载时为 null。 */
let _view: EditorView | null = null;

/** 脏比较基线：最近一次保存（或加载）的 MD 文本。 */
let _savedMd = '';

const _isDirty = ref(false);
const _isSaving = ref(false);
const _canUndo = ref(false);
const _canRedo = ref(false);

/**
 * 半预览（live）模式偏好。跨 mount/unmount 保留——
 * 这是「用户希望的编辑形态」，与具体项目无关。
 */
const _isLivePreview = ref(false);

/**
 * 内容提供者覆盖（实验视图「预览编辑」注册）。
 * 非空时 `save()` 从这里取内容而非 CM6 文档——TipTap 编辑期间
 * 隐藏的 CM6 内容是陈旧的。由 wysiwyg 宿主组件挂载/卸载时注册/清除。
 */
let _contentProvider: (() => string) | null = null;

/**
 * 编辑命令目标（实验视图「预览编辑」注册）。
 *
 * 工具栏命令（格式切换 / 插入表格公式 / 撤销重做）优先路由到这里，
 * 由 TipTap 编辑面实现；未注册时走 CM6 实现。由 wysiwyg 宿主组件
 * 挂载/卸载时注册/清除，与 `_contentProvider` 配对。
 */
export interface EditorCommandTarget {
  /** 切换行内格式（加粗/斜体/标题等）；无对应形式的 kind（如下划线）应静默忽略。 */
  toggleFormat(kind: MarkdownFormatKind, level?: HeadingLevel): void;
  /** 插入真实表格（wysiwyg 编辑面支持原生表格编辑）。 */
  insertTable(rows: number, cols: number, syntax: TableSyntax, caption: string): boolean;
  /** 插入/切换行内公式。 */
  insertInlineMath(): boolean;
  /** 插入块级公式。 */
  insertMathBlock(): boolean;
  /** 插入 LaTeX 片段（符号/结构模板）。 */
  insertMathSnippet(latex: string): boolean;
  undo(): void;
  redo(): void;
  canUndo(): boolean;
  canRedo(): boolean;
}

let _commandTarget: EditorCommandTarget | null = null;

/** 活动行（光标行）订阅者集合。0-based 行号。 */
const _activeLineCallbacks = new Set<(line: number) => void>();
/** 文档变更订阅者集合。接收完整 MD 文本。 */
const _docChangeCallbacks = new Set<(md: string) => void>();
/** 选区变化订阅者集合。接收选区偏移（from === to 表示空选区）。 */
const _selectionCallbacks = new Set<(from: number, to: number) => void>();

// ── 内部工具 ───────────────────────────────────────────────────────

function _emitActiveLine(line: number): void {
  _activeLineCallbacks.forEach((cb) => cb(line));
}

function _emitDocChange(md: string): void {
  _docChangeCallbacks.forEach((cb) => cb(md));
}

function _emitSelectionChange(from: number, to: number): void {
  _selectionCallbacks.forEach((cb) => cb(from, to));
}

/** 根据 CM6 history 字段刷新 undo/redo 可用性。 */
function _updateHistoryFlags(): void {
  if (!_view) return;
  _canUndo.value = undoDepth(_view.state) > 0;
  _canRedo.value = redoDepth(_view.state) > 0;
}

// ── CM6 回调 ───────────────────────────────────────────────────────
// 函数声明会被提升，因此 `callbacks` 在模块级初始化时即可引用
// `save` / `_emitActiveLine` / `_emitDocChange` / `_updateHistoryFlags`。

const callbacks: EditorCallbacks = {
  onSave: () => { void save(); },
  onActiveLineChange: (line) => { _emitActiveLine(line); },
  onDocChange: (md) => {
    _isDirty.value = md !== _savedMd;
    _emitDocChange(md);
    // 在微任务中刷新 history 标志，确保 CM6 已完成事务提交。
    queueMicrotask(() => _updateHistoryFlags());
  },
  onSelectionChange: (from, to) => { _emitSelectionChange(from, to); },
};

// ── 生命周期 ───────────────────────────────────────────────────────

/**
 * 在指定宿主元素上挂载 CM6 编辑器。
 *
 * 若已有实例存在，先卸载之。挂载后 `_savedMd` 设为 `md`，dirty 状态清零。
 *
 * @param el  CM6 DOM 挂载点。
 * @param md  初始 MD 文本。
 */
function mount(el: HTMLElement, md: string): void {
  if (_view) {
    unmount();
  }
  _savedMd = md;
  _isDirty.value = false;
  const state = createEditorState(md, callbacks);
  _view = createEditorView(el, state);
  // 记忆的半预览偏好跨项目保留：挂载后立即以 effect 恢复开关
  if (_isLivePreview.value) {
    _view.dispatch({ effects: setLivePreviewEffect.of(true) });
  }
  _updateHistoryFlags();
}

/** 卸载 CM6 实例并清空所有编辑状态。 */
function unmount(): void {
  if (_view) {
    _view.destroy();
    _view = null;
  }
  _savedMd = '';
  _isDirty.value = false;
  _canUndo.value = false;
  _canRedo.value = false;
}

// ── 内容读写 ───────────────────────────────────────────────────────

/** 获取当前文档全文。未挂载时返回空串。 */
function getMd(): string {
  return _view ? _view.state.doc.toString() : '';
}

/**
 * 设置文档内容。
 *
 * 重建 `EditorState`，丢弃 undo 历史（Phase 1 取舍）。用于项目切换、保存归一化
 * 后的回写。
 */
function setMd(md: string): void {
  if (!_view) return;
  const state = createEditorState(md, callbacks);
  _view.setState(state);
  // 重建的状态其字段回到默认（关闭）——恢复半预览偏好，保持视图模式不漂移
  if (_isLivePreview.value) {
    _view.dispatch({ effects: setLivePreviewEffect.of(true) });
  }
  _savedMd = md;
  _isDirty.value = false;
  _updateHistoryFlags();
}

/**
 * 以本地事务方式应用编辑片段（供大纲结构操作等调用）。
 *
 * dispatch 进入 CM6 事件循环，更新会经 `onDocChange` 触发大纲等下游一致更新；
 * 同时进入 history，支持 Ctrl+Z 撤销。未挂载时返回 false。
 *
 * @param changes 来自 `headingOps` 等纯函数层计算的编辑片段。
 * @returns 是否成功 dispatch。
 */
function dispatchChanges(changes: ChangeSpec[]): boolean {
  if (!_view) return false;
  _view.dispatch({ changes });
  return true;
}

// ── 编辑命令 ───────────────────────────────────────────────────────

/**
 * 切换半预览（live）渲染开关。
 *
 * 以 StateEffect 实现，不重建 EditorState：撤销历史、光标位置、
 * 滚动偏移全部保留。未挂载时仅记录偏好，下次挂载自动生效。
 *
 * @param enabled true 进入半预览渲染形态，false 回到源码形态。
 */
function setLivePreview(enabled: boolean): void {
  if (_isLivePreview.value === enabled) return;
  _isLivePreview.value = enabled;
  _view?.dispatch({ effects: setLivePreviewEffect.of(enabled) });
}

/** 撤销。CM6 `undo` 在历史栈为空时为 no-op。 */
function undoEd(): void {
  if (_commandTarget) {
    _commandTarget.undo();
    return;
  }
  if (_view) undo(_view);
}

/** 重做。CM6 `redo` 在历史栈为空时为 no-op。 */
function redoEd(): void {
  if (_commandTarget) {
    _commandTarget.redo();
    return;
  }
  if (_view) redo(_view);
}

/**
 * 切换 Markdown 格式（加粗 / 斜体 / 标题）。
 *
 * 委托给纯函数层 {@link applyMarkdownFormat} 计算文档变更与选区，
 * 再通过 CM6 dispatch 应用，并保持选区方向、聚焦编辑器。
 * 工具栏按钮与后续快捷键共用此入口。
 *
 * @param kind  格式类型。
 * @param level 标题级别（1-6）；仅 kind === 'heading' 时生效，默认 1。
 */
function toggleFormat(kind: MarkdownFormatKind, level: HeadingLevel = 1): void {
  if (_commandTarget) {
    _commandTarget.toggleFormat(kind, level);
    return;
  }
  if (!_view) return;
  const sel = _view.state.selection.main;
  const from = Math.min(sel.anchor, sel.head);
  const to = Math.max(sel.anchor, sel.head);
  const result = applyMarkdownFormat(_view.state.doc.toString(), from, to, kind, level);
  const reversed = sel.head < sel.anchor;
  _view.dispatch({
    changes: result.changes,
    // 保持原选区方向（反向框选时 anchor/head 互换）
    selection: reversed
      ? { anchor: result.selection.head, head: result.selection.anchor }
      : result.selection,
    scrollIntoView: true,
  });
  _view.focus();
}

/** 在语法树中定位文档区间内首个表格节点（裸 GFM 表或 f-tbl 内嵌表）的起点。 */
function findTableStart(view: EditorView, from: number, to: number): number | null {
  let found: number | null = null;
  syntaxTree(view.state).iterate({
    from,
    to,
    enter: (node) => {
      if (node.name === 'Table' || node.name === 'FTagTable') {
        found = node.from;
        return false;
      }
      return node.name === 'Document' || node.name.startsWith('FTag');
    },
  });
  return found;
}

/**
 * 若插入区间内含表格块，将焦点定位到渲染态表格的首单元格。
 *
 * 表格在半预览中以 contenteditable widget 呈现，CM 选区无法进入；
 * 按语法树定位表格起点，经 domAtPos 找到 widget DOM 并聚焦首单元格，
 * 实现「插入即可直接输入」。未开启半预览（无 widget）时静默跳过。
 */
function focusInsertedTableCell(view: EditorView, from: number, to: number): void {
  const tablePos = findTableStart(view, from, to);
  if (tablePos === null) return;
  try {
    const resolved = view.domAtPos(tablePos, 1);
    const el = resolved.node instanceof Element
      ? resolved.node
      : resolved.node.parentElement;
    const box = el?.closest('.fluen-lp-tablebox');
    const cell = box?.querySelector<HTMLElement>('th,td');
    cell?.focus();
  } catch {
    /* 位置解析失败（装饰未渲染等）：保持块后光标 */
  }
}

/**
 * 在光标处插入块级内容（表格、公式等），自动保证块间空行隔离。
 *
 * 位置与空行计算委托给纯函数层 {@link insertBlockSpec}；插入后光标
 * 定位到块内容之后并聚焦编辑器；`caretInBlock` 指定光标落在块内
 * 相对偏移（如块级公式需落在两个 `$$` 之间）。
 * 未挂载时返回 false。
 *
 * @param block        块级文本（不含首尾空行）。
 * @param caretInBlock 光标在 block 内的相对偏移；缺省为块后。
 * @returns 是否成功插入。
 */
function insertBlock(block: string, caretInBlock?: number): boolean {
  if (!_view) return false;
  const pos = _view.state.selection.main.head;
  const spec = insertBlockSpec(_view.state.doc.toString(), pos, block);
  const change = spec.changes[0];
  const selection = caretInBlock === undefined
    ? spec.selection
    : { anchor: spec.blockStart + caretInBlock, head: spec.blockStart + caretInBlock };
  _view.dispatch({
    changes: spec.changes,
    selection,
    scrollIntoView: true,
  });
  _view.focus();
  if (change) {
    // 焦点定位须在编辑器聚焦之后（否则会被 focus() 抢回）
    focusInsertedTableCell(_view, change.from, change.from + change.insert.length);
  }
  return true;
}

/**
 * 在光标处插入表格。
 *
 * CM6 编辑面：经 {@link buildTableBlock} 生成 `<f-tbl>` 块后走 insertBlock；
 * 预览编辑（wysiwyg）编辑面：命令目标插入原生表格（GFM 往返，支持直接编辑）。
 *
 * @param rows    行数（含表头）。
 * @param cols    列数。
 * @param syntax  语法形态（仅 CM6 路由使用；wysiwyg 一律原生表格）。
 * @param caption 表格题注（仅 CM6 路由使用）。
 * @returns 是否成功插入。
 */
function insertTable(rows: number, cols: number, syntax: TableSyntax, caption: string): boolean {
  if (_commandTarget) {
    return _commandTarget.insertTable(rows, cols, syntax, caption);
  }
  return insertBlock(buildTableBlock(syntax, rows, cols, caption));
}

/**
 * 插入行内公式（单元格感知）。
 *
 * 焦点在表格单元格时经 DOM 层包裹选区（CM 选区不在单元格内，常规
 * 命令无法触达）；否则走 CM 行内格式切换（包裹/取消三态）。
 */
function insertInlineMath(): boolean {
  if (_commandTarget) {
    return _commandTarget.insertInlineMath();
  }
  if (focusedTableCell()) return toggleCellInlineMath();
  if (!_view) return false;
  toggleFormat('inlineMath');
  return true;
}

/**
 * 插入块级公式（单元格感知）。
 *
 * 表格单元格为单行内容（Enter 已收敛），块式 `$$` 在其中无法渲染，
 * 故单元格内退化为行内公式切换；文档层经 insertBlock 插入 `$$\n\n$$`
 * 并将光标定位到两定界符之间。
 */
function insertMathBlock(): boolean {
  if (_commandTarget) {
    return _commandTarget.insertMathBlock();
  }
  if (focusedTableCell()) return toggleCellInlineMath();
  return insertBlock('$$\n\n$$', 3);
}

/**
 * 插入 LaTeX 片段（符号/结构模板），目标自动判定：
 * 焦点在表格单元格 → 插入单元格；否则插入文档光标处（数学上下文内
 * 直接插入，包裹于 `$...$` 中插入）。
 */
function insertMathSnippet(latex: string): boolean {
  if (_commandTarget) {
    return _commandTarget.insertMathSnippet(latex);
  }
  if (focusedTableCell()) return insertCellMathSnippet(latex);
  if (!_view) return false;
  return insertMathAtCursor(_view, latex);
}

/**
 * 注册/清除内容提供者（供预览编辑视图等非 CM6 编辑面使用）。
 *
 * @param getMd 内容提供函数；传 null 清除。
 */
function registerContentProvider(getMd: (() => string) | null): void {
  _contentProvider = getMd;
}

/**
 * 注册/清除编辑命令目标（供预览编辑视图等非 CM6 编辑面使用）。
 *
 * @param target 命令实现；传 null 清除。
 */
function registerCommandTarget(target: EditorCommandTarget | null): void {
  _commandTarget = target;
}

/**
 * 广播一次非 CM6 来源的文档更新（预览编辑视图专用）。
 *
 * 刷新共享脏标记、按命令目标刷新撤销历史标志（工具栏按钮可用性），
 * 并广播 doc-change（大纲/预览订阅者保持一致），不触碰 CM6 实例——
 * CM6 的内容由 ContentPanel 在离开 wysiwyg 视图时回灌。
 */
function notifyExternalDocChange(md: string): void {
  _isDirty.value = md !== _savedMd;
  if (_commandTarget) {
    _canUndo.value = _commandTarget.canUndo();
    _canRedo.value = _commandTarget.canRedo();
  }
  _emitDocChange(md);
}

// ── 保存 ───────────────────────────────────────────────────────────

/**
 * 保存当前文档。
 *
 * 调用 `useProject().saveContent(md)` 进行原子保存；成功后后端返回的
 * `main_md` 可能与原文不同（行尾归一化等），此时回写编辑器以保持同步。
 * 回写通过 `setMd` 重建状态，会丢失 undo 历史（Phase 1 取舍）。
 *
 * 防重入：保存进行中（`_isSaving` 为 true）再次调用直接返回 false。
 * 全局快捷键（capture 阶段）与 CM6 内部 keymap 都可能触发保存，
 * 双通道并发时靠该检查避免重复 IPC。
 *
 * @returns 保存是否成功。
 */
async function save(): Promise<boolean> {
  if (_isSaving.value) return false;
  // 预览编辑视图激活时从 provider 取内容（CM6 是陈旧的）；两者皆无则不可保存
  if (!_contentProvider && !_view) return false;
  const md = _contentProvider ? _contentProvider() : getMd();
  const project = useProject();
  _isSaving.value = true;
  try {
    const success = await project.saveContent(md);
    if (success) {
      const normalized = project.mainMd.value;
      if (normalized && normalized !== md) {
        // 后端归一化了内容（如行尾），同步编辑器。
        setMd(normalized);
      } else {
        _savedMd = md;
        _isDirty.value = false;
      }
    }
    return success;
  } finally {
    _isSaving.value = false;
  }
}

// ── 大纲联动 ───────────────────────────────────────────────────────

/**
 * 滚动到指定行并聚焦编辑器。
 *
 * @param line  0-based 行号（来自 `outlineParser`）。CM6 内部为 1-based。
 */
function scrollToLine(line: number): void {
  if (!_view) return;
  const lineNum = line + 1;
  const lineCount = _view.state.doc.lines;
  if (lineNum < 1 || lineNum > lineCount) return;
  const targetLine = _view.state.doc.line(lineNum);
  _view.dispatch({
    selection: { anchor: targetLine.from },
    scrollIntoView: true,
  });
  _view.focus();
}

/**
 * 请求 CM6 立即重新测量布局。
 *
 * 编辑器容器经 `v-show` 在隐藏/显示间切换（如视图模式切换）后，
 * 尺寸可能从 0 恢复，调用本方法让 CM6 立刻重测而非依赖 ResizeObserver 异步触发。
 */
function requestMeasure(): void {
  _view?.requestMeasure();
}

/**
 * 订阅活动行变化。
 *
 * @returns 取消订阅函数。
 */
function onActiveLineChange(cb: (line: number) => void): () => void {
  _activeLineCallbacks.add(cb);
  return () => { _activeLineCallbacks.delete(cb); };
}

/**
 * 订阅文档内容变化。
 *
 * @returns 取消订阅函数。
 */
function onDocChange(cb: (md: string) => void): () => void {
  _docChangeCallbacks.add(cb);
  return () => { _docChangeCallbacks.delete(cb); };
}

/**
 * 订阅选区变化（含选区清空，from === to 表示空选区）。
 *
 * @returns 取消订阅函数。
 */
function onSelectionChange(cb: (from: number, to: number) => void): () => void {
  _selectionCallbacks.add(cb);
  return () => { _selectionCallbacks.delete(cb); };
}

/**
 * 获取当前框选文本。无选区或未挂载时返回空串。
 */
function getSelectionText(): string {
  if (!_view) return '';
  const sel = _view.state.selection.main;
  if (sel.empty) return '';
  return _view.state.doc.sliceString(
    Math.min(sel.anchor, sel.head),
    Math.max(sel.anchor, sel.head),
  );
}

/**
 * 获取当前选区在视口中的坐标（供划选工具栏定位）。
 *
 * 返回选区首行上方中点的视口坐标；未挂载或选区为空时返回 null。
 * 坐标随滚动即时失效，调用方需在滚动时重算或隐藏工具栏。
 */
function getSelectionViewportCoords(): { left: number; top: number } | null {
  if (!_view) return null;
  const sel = _view.state.selection.main;
  if (sel.empty) return null;
  const from = Math.min(sel.anchor, sel.head);
  const to = Math.max(sel.anchor, sel.head);
  try {
    const a = _view.coordsAtPos(from);
    const b = _view.coordsAtPos(to);
    if (!a || !b) return null;
    return { left: (a.left + b.left) / 2, top: Math.min(a.top, b.top) };
  } catch {
    return null;
  }
}

/**
 * 清除当前选区（光标落在选区起点，不改变文档内容）。
 */
function clearSelection(): void {
  if (!_view) return;
  const sel = _view.state.selection.main;
  if (sel.empty) return;
  _view.dispatch({ selection: { anchor: Math.min(sel.anchor, sel.head) } });
}

// ── composable ─────────────────────────────────────────────────────

export function useFluenEditor() {
  return {
    // 状态（只读）
    isDirty: readonly(_isDirty),
    isSaving: readonly(_isSaving),
    canUndo: readonly(_canUndo),
    canRedo: readonly(_canRedo),
    isLivePreview: readonly(_isLivePreview),

    // 生命周期
    mount,
    unmount,
    setMd,
    getMd,

    // 编辑
    undo: undoEd,
    redo: redoEd,
    toggleFormat,
    insertBlock,
    insertTable,
    insertInlineMath,
    insertMathBlock,
    insertMathSnippet,
    dispatchChanges,
    setLivePreview,

    // 保存
    save,
    registerContentProvider,
    registerCommandTarget,
    notifyExternalDocChange,

    // 大纲联动
    scrollToLine,
    requestMeasure,
    onActiveLineChange,
    onDocChange,
    onSelectionChange,
    getSelectionText,
    getSelectionViewportCoords,
    clearSelection,
  };
}
