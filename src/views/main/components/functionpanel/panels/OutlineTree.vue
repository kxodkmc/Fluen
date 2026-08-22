<script setup lang="ts">
/**
 * OutlineTree — 大纲扁平列表渲染层。
 *
 * 取代原递归组件 `OutlineNodeItem`：以 `v-for` 渲染可见行，避免整棵递归树重建；
 * 折叠通过一次线性扫描计算可见行（`visibleRows`），缩进由 `depth` 驱动。
 *
 * 状态接线：
 * - 折叠 / hover / 活动行 / 编辑输入 来自 `useOutlineUi`（单例）；
 * - 导航与结构操作由行事件上抛，本层将其打包为对应操作后统一派发给面板。
 */
import { computed } from 'vue';
import type { FlatHeading } from '../../../composables/outline/outlineParser';
import { useOutline } from '../../../composables/outline';
import { useOutlineUi } from '../../../composables/outline/useOutlineUi';
import OutlineRow from './OutlineRow.vue';

const props = defineProps<{
  /** 全部扁平标题（含被折叠隐藏的后代）。 */
  headings: readonly FlatHeading[];
}>();

const { activeLine, isCollapsed } = useOutline();
const ui = useOutlineUi();

const emit = defineEmits<{
  (e: 'navigate', line: number): void;
}>();

/** 可见行：线性扫描，最近折叠祖先层级作为屏障，屏障内的后代被隐藏。 */
const visibleRows = computed<FlatHeading[]>(() => {
  const rows: FlatHeading[] = [];
  let barrier = Infinity;
  for (const h of props.headings) {
    if (h.level <= barrier) barrier = Infinity;
    if (h.level > barrier) continue;
    rows.push(h);
    if (isCollapsed(h)) barrier = h.level;
  }
  return rows;
});
</script>

<template>
  <div class="outline-tree">
    <OutlineRow
      v-for="node in visibleRows"
      :key="node.id"
      :node="node"
      :active="activeLine === node.line"
      :collapsed="isCollapsed(node)"
      :editing="ui.editingId.value === node.id ? ui.editKind.value : null"
      :edit-value="ui.editValue.value"
      :hovered="ui.hoveredId.value === node.id"
      :editable="node.sectionId !== null"
      @navigate="emit('navigate', $event)"
      @toggle-collapse="ui.toggleId(node.id)"
      @start-rename="ui.startEdit(node, 'rename')"
      @start-add-child="ui.startEdit(node, 'child')"
      @update:edit-value="ui.setEditValue"
      @commit-edit="ui.commitEdit(node)"
      @cancel-edit="ui.cancelEdit"
      @hover="ui.setHovered"
    />
  </div>
</template>

<style scoped>
.outline-tree {
  display: flex;
  flex-direction: column;
}
</style>