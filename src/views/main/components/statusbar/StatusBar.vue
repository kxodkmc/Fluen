<script setup lang="ts">
/**
 * StatusBar — 底部状态栏渲染组件。
 *
 * 跨越功能区、编辑器、AI 区三段面板底部。
 * 纯渲染组件，从 useStatusBar 注册表读取数据，不包含业务逻辑。
 * 各业务模块通过 useStatusEntry / useStatusBar 注册自己的状态条目。
 *
 * 作为 MainView 布局的最后一行，flex-shrink: 0 固定高度。
 */
import { useStatusBar } from './useStatusBar';
import TaskQueueIndicator from './taskqueue/TaskQueueIndicator.vue';

const { left, right } = useStatusBar();
</script>

<template>
  <footer class="status-bar">
    <!-- ── 左侧分区 ─────────────────────────────────────────────────── -->
    <div class="status-bar__section">
      <template v-for="item in left" :key="item.id">
        <span
          class="status-bar__item"
          :class="[
            `status-bar__item--${item.tone ?? 'default'}`,
            { 'status-bar__item--clickable': item.onClick },
          ]"
          :title="item.tooltip"
          @click="item.onClick?.()"
        >
          <svg
            v-if="item.icon"
            class="status-bar__icon"
            viewBox="0 0 24 24"
            width="12"
            height="12"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path :d="item.icon" />
          </svg>
          {{ item.label }}
        </span>
      </template>
    </div>

    <!-- ── 右侧分区 ─────────────────────────────────────────────────── -->
    <div class="status-bar__section">
      <template v-for="item in right" :key="item.id">
        <span
          class="status-bar__item"
          :class="[
            `status-bar__item--${item.tone ?? 'default'}`,
            { 'status-bar__item--clickable': item.onClick },
          ]"
          :title="item.tooltip"
          @click="item.onClick?.()"
        >
          <svg
            v-if="item.icon"
            class="status-bar__icon"
            viewBox="0 0 24 24"
            width="12"
            height="12"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path :d="item.icon" />
          </svg>
          {{ item.label }}
        </span>
      </template>
      <!-- 任务队列指示器 -->
      <TaskQueueIndicator />
    </div>
  </footer>
</template>

<style scoped>
.status-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 26px;
  flex-shrink: 0;
  padding: 0 8px;
  background: var(--fluen-accent);
  color: var(--fluen-on-accent);
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  user-select: none;
}

.status-bar__section {
  display: flex;
  align-items: center;
  gap: 2px;
  min-width: 0;
}

.status-bar__item {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 0 6px;
  height: 26px;
  white-space: nowrap;
  transition: background 0.15s ease;
}

.status-bar__item--clickable {
  cursor: pointer;
  border-radius: 3px;
}

.status-bar__item--clickable:hover {
  background: rgba(255, 255, 255, 0.15);
}

.status-bar__icon {
  flex-shrink: 0;
}

/* ── 语义色调 ───────────────────────────────────────────────────────── */
.status-bar__item--info {
  color: rgba(255, 255, 255, 0.85);
}

.status-bar__item--warning {
  color: #ffd54f;
}

.status-bar__item--error {
  color: #ff8a80;
}
</style>
