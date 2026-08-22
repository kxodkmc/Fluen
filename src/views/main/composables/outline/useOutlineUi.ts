/**
 * 大纲 UI 瞬态状态层（单例）。
 *
 * 管理不随文档内容持久化的临时状态：折叠集合、hover 行、活动行、以及当前处于
 * 编辑中的输入（重命名 / 新建子标题）。折叠与 hover 均按稳定 id 索引，重命名后
 * 该行 id 变化，其折叠/编辑状态随之丢失（已知取舍）。
 *
 * 结构操作（`applyRename` / `applyInsertChild`）走本地事务：调用 `headingOps`
 * 纯函数计算编辑片段后 dispatch 进 CM6 历史（可 Ctrl+Z 撤销），不再依赖后端。
 */

import { ref, shallowRef, readonly } from 'vue';
import type { FlatHeading } from './outlineParser';
import { renameHeadingAt, insertChildAt } from './headingOps';
import { useFluenEditor } from '../../components/editor/composables/useFluenEditor';

/** 编辑类型：重命名 / 新建子标题。 */
export type EditKind = 'rename' | 'child';

// ── 模块级状态（单例） ──────────────────────────────────────────────

/** 折叠节点 id 集合（按稳定 id 索引）。 */
const _collapsedIds = shallowRef<Set<string>>(new Set());
/** 当前 hover 的节点 id（用于只渲染一份操作按钮）。 */
const _hoveredId = shallowRef<string | null>(null);
/** 编辑器活动行（光标行，0-based），供 UI 高亮当前标题。 */
const _activeLine = ref<number | null>(null);

/** 当前处于编辑输入的节点 id。 */
const _editingId = ref<string | null>(null);
/** 编辑类型。 */
const _editKind = ref<EditKind | null>(null);
/** 编辑输入值。 */
const _editValue = ref<string>('');

// 订阅编辑器活动行变化（模块级、注册一次）。
useFluenEditor().onActiveLineChange((line) => {
  _activeLine.value = line;
});

/** 节点是否可折叠：有章节归属且有后代。 */
function isCollapsible(node: FlatHeading): boolean {
  return node.sectionId !== null && node.childrenCount > 0;
}

/** 节点是否折叠。 */
function isCollapsedId(id: string): boolean {
  return _collapsedIds.value.has(id);
}

/** 切换某节点的折叠状态。 */
function toggleId(id: string): void {
  const next = new Set(_collapsedIds.value);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  _collapsedIds.value = next;
}

/** 用新集合整体覆盖折叠状态（`expandAll` 传空集，`collapseAll` 传所有可折叠 id）。 */
function setCollapsed(ids: Set<string>): void {
  _collapsedIds.value = ids;
}

/** 设置当前 hover 行 id。 */
function setHovered(id: string | null): void {
  _hoveredId.value = id;
}

// ── 编辑输入 ────────────────────────────────────────────────────────

/** 进入某节点的编辑输入模式（`rename` 预设当前标题，`child` 置空）。 */
function startEdit(node: FlatHeading, kind: EditKind): void {
  _editingId.value = node.id;
  _editKind.value = kind;
  _editValue.value = kind === 'rename' ? node.text : '';
}

/** 更新编辑输入值（由输入框 v-model 或事件驱动）。 */
function setEditValue(value: string): void {
  _editValue.value = value;
}

/** 取消编辑输入。 */
function cancelEdit(): void {
  _editingId.value = null;
  _editKind.value = null;
  _editValue.value = '';
}

/** 提交当前编辑输入：按类型 dispatch 对应本地事务。 */
function commitEdit(node: FlatHeading): void {
  const value = _editValue.value;
  const kind = _editKind.value;
  clearEdit();
  if (!value.trim()) return;
  if (kind === 'rename') applyRename(node, value);
  else if (kind === 'child') applyInsertChild(node, value);
}

/** 应用重命名（本地事务）。返回是否成功。 */
function applyRename(node: FlatHeading, newText: string): boolean {
  const editor = useFluenEditor();
  const res = renameHeadingAt(editor.getMd(), node, newText);
  if (res.error || !res.changes) return false;
  return editor.dispatchChanges(res.changes);
}

/** 应用新建子标题（本地事务）。返回是否成功。 */
function applyInsertChild(node: FlatHeading, title: string): boolean {
  const editor = useFluenEditor();
  const res = insertChildAt(editor.getMd(), node, title);
  if (res.error || !res.changes) return false;
  return editor.dispatchChanges(res.changes);
}

/** 清空编辑输入状态（保留值，供提交后由渲染自然重置）。 */
function clearEdit(): void {
  _editingId.value = null;
  _editKind.value = null;
}

/** 大纲 UI 状态与操作。 */
export function useOutlineUi() {
  return {
    collapsedIds: readonly(_collapsedIds),
    hoveredId: readonly(_hoveredId),
    activeLine: readonly(_activeLine),
    editingId: readonly(_editingId),
    editKind: readonly(_editKind),
    editValue: readonly(_editValue),

    isCollapsible,
    isCollapsedId,
    toggleId,
    setCollapsed,
    setHovered,
    startEdit,
    setEditValue,
    commitEdit,
    cancelEdit,
    applyRename,
    applyInsertChild,
  };
}