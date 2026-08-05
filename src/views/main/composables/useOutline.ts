/**
 * 大纲状态管理 composable（单例模式）。
 *
 * 数据源：
 *   - 主：订阅 `useFluenEditor().onDocChange`，捕获编辑器实时编辑（每次按键）。
 *   - 辅：`watch(useProject().tempMd)`，处理"项目打开但编辑器尚未挂载"的窗口期
 *     （`tempMd` 仅在打开/保存时更新，编辑器挂载后由 `onDocChange` 接管）。
 *
 * 管理层级筛选状态；提供跳转目标供 ContentPanel 响应；
 * 提供活动行（光标行）供 UI 高亮当前标题；
 * 提供标题重命名与子标题插入能力（通过后端文本匹配，不依赖行号）。
 *
 * @example
 * ```ts
 * const { outline, minLevel, maxLevel, jumpTarget, jumpTo, renameNode, activeLine } = useOutline();
 * jumpTo(node.line);
 * await renameNode(node, '新标题');
 * ```
 */

import { ref, shallowRef, computed, watch, readonly } from 'vue';
import { useProject } from '../../../composables/useProject';
import { useFluenEditor } from '../components/editor/composables/useFluenEditor';
import { parseOutline, filterByLevel, type OutlineNode } from './outlineParser';

// ── 模块级状态（单例） ──────────────────────────────────────────────

const _minLevel = ref(1);
const _maxLevel = ref(6);

/** 跳转目标行号。ContentPanel watch 此值并滚动到对应位置。 */
const _jumpTarget = ref<{ line: number; timestamp: number } | null>(null);

/** 活动行（光标行，0-based）。由 `useFluenEditor().onActiveLineChange` 推送，供 UI 高亮。 */
const _activeLine = ref<number | null>(null);

/**
 * 折叠节点集合（按 sectionId 索引，跨大纲重新解析保持稳定）。
 * 使用 shallowRef + 新 Set 实例触发响应式更新。
 */
const _collapsedSet = shallowRef<Set<string>>(new Set());

// ── 内部缓存：完整大纲（未筛选） ────────────────────────────────────
// 使用 shallowRef 确保 computed（outline / hasOutline）能响应大纲变化。
// 此前的 plain let 变量不会被 Vue 追踪，导致项目打开后大纲面板不刷新。
const _fullOutline = shallowRef<OutlineNode[]>([]);

// ── 编辑器联动订阅（模块级，随单例生命周期存在；不显式清理） ──────────

/**
 * 订阅编辑器文档变化（主数据源）。
 *
 * 编辑器挂载并 `setMd` 后会立即触发一次，覆盖 `watch(tempMd)` 的初始解析；
 * 后续每次按键均触发，保证大纲与编辑器内容实时同步。
 * 返回的 unsubscribe 函数随单例生命周期保留，不显式调用。
 */
useFluenEditor().onDocChange((md) => {
  _fullOutline.value = md ? parseOutline(md) : [];
});

/**
 * 订阅编辑器活动行变化，更新 `_activeLine` 供 UI 高亮当前标题。
 */
useFluenEditor().onActiveLineChange((line) => {
  _activeLine.value = line;
});

// ── composable ─────────────────────────────────────────────────────

export function useOutline() {
  const { tempMd, hasProject, isSaving, renameHeading, insertHeading } = useProject();

  // 辅数据源：处理"项目打开但编辑器尚未挂载"的窗口期。
  // 编辑器挂载后 onDocChange 接管实时更新；此处仅在 tempMd 变化时补一次解析。
  watch(
    tempMd,
    (md) => {
      _fullOutline.value = md ? parseOutline(md) : [];
    },
    { immediate: true },
  );

  /** 筛选后的大纲树。 */
  const outline = computed(() =>
    filterByLevel(_fullOutline.value, _minLevel.value, _maxLevel.value),
  );

  /** 是否有大纲内容。 */
  const hasOutline = computed(() => _fullOutline.value.length > 0);

  /** 触发跳转到指定行。同时驱动 ContentPanel 滚动（_jumpTarget）与编辑器滚动（scrollToLine）。 */
  function jumpTo(line: number): void {
    _jumpTarget.value = { line, timestamp: Date.now() };
    useFluenEditor().scrollToLine(line);
  }

  /** 手动设置活动行（正常流程由编辑器回调驱动；暴露此 API 供测试/扩展使用）。 */
  function setActive(line: number): void {
    _activeLine.value = line;
  }

  /** 设置最小层级。 */
  function setMinLevel(level: number): void {
    _minLevel.value = Math.max(1, Math.min(6, level));
  }

  /** 设置最大层级。 */
  function setMaxLevel(level: number): void {
    _maxLevel.value = Math.max(1, Math.min(6, level));
  }

  /** 重置层级筛选为 1-6（全部显示）。 */
  function resetFilter(): void {
    _minLevel.value = 1;
    _maxLevel.value = 6;
  }

  // ── 折叠/展开 ───────────────────────────────────────────────────

  /** 节点是否处于折叠状态（无 sectionId 的节点永不折叠）。 */
  function isCollapsed(node: OutlineNode): boolean {
    if (!node.sectionId || node.children.length === 0) return false;
    return _collapsedSet.value.has(node.sectionId);
  }

  /** 切换节点折叠状态。 */
  function toggleCollapse(node: OutlineNode): void {
    if (!node.sectionId || node.children.length === 0) return;
    const next = new Set(_collapsedSet.value);
    if (next.has(node.sectionId)) {
      next.delete(node.sectionId);
    } else {
      next.add(node.sectionId);
    }
    _collapsedSet.value = next;
  }

  /** 展开所有节点。 */
  function expandAll(): void {
    _collapsedSet.value = new Set();
  }

  /** 折叠所有有子节点的节点。 */
  function collapseAll(): void {
    const ids: string[] = [];
    function walk(nodes: OutlineNode[]): void {
      for (const n of nodes) {
        if (n.sectionId && n.children.length > 0) {
          ids.push(n.sectionId);
        }
        walk(n.children);
      }
    }
    walk(_fullOutline.value);
    _collapsedSet.value = new Set(ids);
  }

  /**
   * 重命名大纲节点。
   *
   * 通过后端文本匹配定位标题行（`sectionId` + `level` + `oldText`），
   * 不依赖行号，避免编辑器偏移导致错位。
   * 成功后 `tempMd` 自动变化，大纲自动重新解析。
   */
  async function renameNode(node: OutlineNode, newTitle: string): Promise<boolean> {
    if (!node.sectionId) return false;
    return renameHeading(node.sectionId, node.level, node.text, newTitle);
  }

  /**
   * 插入子标题。
   *
   * 在父节点的作用域末尾插入 `#{level+1} {title}`。
   * 父节点作为锚点，后端通过文本匹配定位插入位置。
   * 成功后 `tempMd` 自动变化，大纲自动重新解析。
   */
  async function insertChildHeading(parentNode: OutlineNode, title: string): Promise<boolean> {
    if (!parentNode.sectionId) return false;
    const childLevel = Math.min(6, parentNode.level + 1);
    return insertHeading(
      parentNode.sectionId,
      parentNode.level,
      parentNode.text,
      childLevel,
      title,
    );
  }

  return {
    // 状态（只读）
    minLevel: readonly(_minLevel),
    maxLevel: readonly(_maxLevel),
    jumpTarget: readonly(_jumpTarget),
    activeLine: readonly(_activeLine),

    // 计算属性
    outline,
    hasOutline,
    hasProject,
    isSaving,

    // 操作
    jumpTo,
    setActive,
    setMinLevel,
    setMaxLevel,
    resetFilter,
    renameNode,
    insertChildHeading,

    // 折叠/展开
    isCollapsed,
    toggleCollapse,
    expandAll,
    collapseAll,
  };
}
