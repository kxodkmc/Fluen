/**
 * 大纲模块门面（单例）——合并导出，保持调用方 API 兼容。
 *
 * 组成：
 * - [`useOutlineDocument`]：扁平标题列表（文档同步源）；
 * - [`useOutlineUi`]：折叠 / hover / 活动行 / 编辑输入等瞬态状态；
 * - 本门面：汇总只读状态与操作，供面板组件直接消费。
 *
 * 结构操作（`renameNode` / `insertChildHeading`）走本地事务，可被 Ctrl+Z 撤销；
 * 后端 `rename_heading` / `insert_heading` 命令保留但退出交互路径。
 */

import { computed } from 'vue';
import { useProject } from '../../../../composables/useProject';
import { useFluenEditor } from '../../components/editor/composables/useFluenEditor';
import { useOutlineDocument } from './useOutlineDocument';
import { useOutlineUi } from './useOutlineUi';
import type { FlatHeading } from './outlineParser';

/** 大纲命名空间门面。 */
export function useOutline() {
  const doc = useOutlineDocument();
  const ui = useOutlineUi();
  const { hasProject, isSaving } = useProject();

  /** 当前扁平标题列表（含稳定 id）。 */
  const outline = doc.flatHeadings;
  /** 是否有标题内容。 */
  const hasOutline = computed(() => outline.value.length > 0);

  /** 跳转到编辑器指定行（0-based）。 */
  function jumpTo(line: number): void {
    useFluenEditor().scrollToLine(line);
  }

  /** 节点是否处于折叠状态。 */
  function isCollapsed(node: FlatHeading): boolean {
    return ui.isCollapsible(node) && ui.isCollapsedId(node.id);
  }

  /** 切换节点折叠状态（不可折叠节点 no-op）。 */
  function toggleCollapse(node: FlatHeading): void {
    if (ui.isCollapsible(node)) ui.toggleId(node.id);
  }

  /** 展开全部可折叠节点。 */
  function expandAll(): void {
    ui.setCollapsed(new Set());
  }

  /** 折叠全部可折叠节点。 */
  function collapseAll(): void {
    ui.setCollapsed(
      new Set(outline.value.filter((h) => ui.isCollapsible(h)).map((h) => h.id)),
    );
  }

  /** 重命名大纲节点（本地事务）。返回是否成功。 */
  function renameNode(node: FlatHeading, newTitle: string): boolean {
    return ui.applyRename(node, newTitle);
  }

  /** 在父节点下插入子标题（本地事务）。返回是否成功。 */
  function insertChildHeading(parentNode: FlatHeading, title: string): boolean {
    return ui.applyInsertChild(parentNode, title);
  }

  return {
    // 状态（只读计算属性）
    outline,
    hasOutline,
    hasProject,
    isSaving,
    activeLine: ui.activeLine,

    // 导航
    jumpTo,

    // 折叠 / 展开
    isCollapsed,
    toggleCollapse,
    expandAll,
    collapseAll,

    // 结构操作（本地事务）
    renameNode,
    insertChildHeading,
  };
}