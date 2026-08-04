<script setup lang="ts">
/**
 * Sidebar — 侧边栏内容容器。
 *
 * 紧邻活动栏右侧，根据 `activeActivity` 渲染对应面板（OutlinePanel 等）。
 * 折叠时宽度过渡到 0，内容 overflow hidden，避免布局跳跃。
 *
 * 设计要点：
 *   - 外层 `.sidebar` 宽度受 `collapsed` 控制，走 CSS width 过渡，保证丝滑
 *   - 内层 `.sidebar__inner` 固定为展开宽度，使折叠过程中内容不被挤压，
 *     仅被外层裁剪 —— 视觉上像窗帘收拢，符合 VSCode 体验
 *   - 面板按 `activeActivity` 切换，新增视图只需在此处加一个分支
 */
import { computed } from 'vue';
import OutlinePanel from './panels/OutlinePanel.vue';
import ReferencesPanel from './panels/ReferencesPanel.vue';

const props = defineProps<{
  /** 当前激活的活动栏项 id */
  activeActivity: string;
  /** 是否收起（收起时宽度过渡到 0） */
  collapsed: boolean;
  /** 侧边栏内容区宽度（px，不含活动栏） */
  width: number;
}>();

/** 外层宽度：折叠时为 0，展开时为传入宽度。 */
const outerWidth = computed(() => (props.collapsed ? 0 : props.width));
</script>

<template>
  <aside
    class="sidebar"
    :class="{ 'sidebar--collapsed': collapsed }"
    :style="{ width: `${outerWidth}px` }"
  >
    <div class="sidebar__inner" :style="{ width: `${width}px` }">
      <div v-show="activeActivity === 'outline'" class="sidebar__section">
        <OutlinePanel />
      </div>
      <div v-show="activeActivity === 'references'" class="sidebar__section">
        <ReferencesPanel />
      </div>
      <!-- 后续扩展：search / wiki 等面板 -->
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  height: 100%;
  background: var(--fluen-surface);
  overflow: hidden;
  flex-shrink: 0;
  transition: width 0.18s ease;
}

.sidebar__inner {
  display: flex;
  flex-direction: column;
  height: 100%;
  /* 内层固定宽度，仅被外层裁剪 —— 折叠时内容不挤压 */
  overflow: hidden;
}

.sidebar__section {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}
</style>
