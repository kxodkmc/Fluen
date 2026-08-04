<script setup lang="ts">
/**
 * FunctionPanel — 左侧功能区容器。
 *
 * VSCode 风格的双层结构：
 *   ┌──────┬────────────────────┐
 *   │ Activ│     Sidebar        │
 *   │ Bar  │  (按激活项渲染面板)  │
 *   │ 48px │  可收起（宽度→0）   │
 *   └──────┴────────────────────┘
 *
 * 职责拆分：
 *   - ActivityBar：图标导航，始终可见，外观稳定
 *   - Sidebar：按 activeActivity 渲染对应面板，宽度可过渡
 *
 * 折叠时仅活动栏可见（侧边栏宽度过渡到 0），整体几何稳定，过渡丝滑。
 * 新增视图只需在 constants.ACTIVITY_ITEMS 增条目 + Sidebar 内加分支。
 */
import { computed } from 'vue';
import ActivityBar from './functionpanel/ActivityBar.vue';
import Sidebar from './functionpanel/Sidebar.vue';
import { ACTIVITY_BAR_WIDTH } from '../constants';

const props = defineProps<{
  /** 当前激活的活动栏项 id */
  activeActivity: string;
  /** 是否处于折叠状态（侧边栏收起） */
  collapsed: boolean;
  /** 展开时面板总宽度（px，含活动栏）；折叠时由组件内部固定为活动栏宽度 */
  width: number;
}>();

defineEmits<{
  (e: 'select-activity', id: string): void;
}>();

/** 侧边栏内容区宽度 = 总宽度 - 活动栏宽度，下限 0。 */
const sidebarWidth = computed(() =>
  Math.max(0, props.width - ACTIVITY_BAR_WIDTH),
);

/** 容器总宽度：折叠时仅活动栏宽度，展开时为传入 width。 */
const panelWidth = computed(() =>
  props.collapsed ? ACTIVITY_BAR_WIDTH : props.width,
);
</script>

<template>
  <div
    class="function-panel"
    :class="{ 'function-panel--collapsed': collapsed }"
    :style="{ width: `${panelWidth}px` }"
  >
    <ActivityBar
      :active-activity="activeActivity"
      :collapsed="collapsed"
      @select-activity="$emit('select-activity', $event)"
    />
    <Sidebar
      :active-activity="activeActivity"
      :collapsed="collapsed"
      :width="sidebarWidth"
    />
  </div>
</template>

<style scoped>
.function-panel {
  display: flex;
  height: 100%;
  background: var(--fluen-surface);
  overflow: hidden;
  flex-shrink: 0;
  /* 与子组件 width 过渡保持同节奏，避免拖拽或折叠时布局跳变 */
  transition: width 0.18s ease;
}

.function-panel--collapsed {
  background: transparent;
}
</style>
