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
 * `temp_md` 会回写进编辑器以同步行尾等差异。预览为只读派生产物，由独立的
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
import { undo, redo, undoDepth, redoDepth } from '@codemirror/commands';
import { createEditorState, createEditorView, type EditorCallbacks } from '../codemirror/setup';
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

/** 活动行（光标行）订阅者集合。0-based 行号。 */
const _activeLineCallbacks = new Set<(line: number) => void>();
/** 文档变更订阅者集合。接收完整 MD 文本。 */
const _docChangeCallbacks = new Set<(md: string) => void>();

// ── 内部工具 ───────────────────────────────────────────────────────

function _emitActiveLine(line: number): void {
  _activeLineCallbacks.forEach((cb) => cb(line));
}

function _emitDocChange(md: string): void {
  _docChangeCallbacks.forEach((cb) => cb(md));
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
  _savedMd = md;
  _isDirty.value = false;
  _updateHistoryFlags();
}

// ── 编辑命令 ───────────────────────────────────────────────────────

/** 撤销。CM6 `undo` 在历史栈为空时为 no-op。 */
function undoEd(): void {
  if (_view) undo(_view);
}

/** 重做。CM6 `redo` 在历史栈为空时为 no-op。 */
function redoEd(): void {
  if (_view) redo(_view);
}

// ── 保存 ───────────────────────────────────────────────────────────

/**
 * 保存当前文档。
 *
 * 调用 `useProject().saveContent(md)` 进行原子保存；成功后后端返回的
 * `temp_md` 可能与原文不同（行尾归一化等），此时回写编辑器以保持同步。
 * 回写通过 `setMd` 重建状态，会丢失 undo 历史（Phase 1 取舍）。
 *
 * @returns 保存是否成功。
 */
async function save(): Promise<boolean> {
  if (!_view) return false;
  const md = getMd();
  const project = useProject();
  _isSaving.value = true;
  try {
    const success = await project.saveContent(md);
    if (success) {
      const normalized = project.tempMd.value;
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

// ── composable ─────────────────────────────────────────────────────

export function useFluenEditor() {
  return {
    // 状态（只读）
    isDirty: readonly(_isDirty),
    isSaving: readonly(_isSaving),
    canUndo: readonly(_canUndo),
    canRedo: readonly(_canRedo),

    // 生命周期
    mount,
    unmount,
    setMd,
    getMd,

    // 编辑
    undo: undoEd,
    redo: redoEd,

    // 保存
    save,

    // 大纲联动
    scrollToLine,
    onActiveLineChange,
    onDocChange,
  };
}
