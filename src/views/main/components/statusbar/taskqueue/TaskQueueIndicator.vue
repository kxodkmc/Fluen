<script setup lang="ts">
/**
 * TaskQueueIndicator — 状态栏任务队列指示器（常驻）。
 *
 * 位于状态栏右侧，始终显示：
 *   - 有活跃任务时：图标 + 标题 + 进度条 + 剩余任务数
 *   - 无活跃任务时：空闲图标 + 队列标题
 *   - 点击展开 TaskQueuePanel 查看全部任务
 *
 * 交互：
 *   - 点击指示器：切换面板展开状态
 *   - 点击面板外部 / ESC：关闭面板
 */
import { ref, onMounted, onUnmounted, computed } from 'vue';
import { useTaskQueue, getProgressRatio, getCategoryIcon } from './useTaskQueue';
import { useI18n } from '../../../../../i18n';
import TaskQueuePanel from './TaskQueuePanel.vue';

const { latestActiveTask, activeCount } = useTaskQueue();
const { t } = useI18n();

const expanded = ref(false);

/** 其他活跃任务数（排除最新任务）。 */
const extraCount = computed(() => Math.max(0, activeCount.value - 1));

/** 最新任务的进度比例。 */
const ratio = computed(() =>
  latestActiveTask.value ? getProgressRatio(latestActiveTask.value.progress) : null,
);

/** 指示器显示的标题：有任务用任务标题，无任务用队列名称。 */
const label = computed(() =>
  latestActiveTask.value ? latestActiveTask.value.title : t('main.taskQueue.title'),
);

/** 指示器显示的图标 path：有任务用分类图标，无任务用默认队列图标。 */
const iconPath = computed(() =>
  latestActiveTask.value
    ? getCategoryIcon(latestActiveTask.value.category)
    : getCategoryIcon('knowledge'),
);

/** 切换面板展开。 */
function toggle(): void {
  expanded.value = !expanded.value;
}

/** 文档点击 — 点击外部时关闭面板。 */
function onDocumentClick(): void {
  expanded.value = false;
}

/** ESC 键关闭面板。 */
function onKeydown(e: KeyboardEvent): void {
  if (e.key === 'Escape') expanded.value = false;
}

onMounted(() => {
  document.addEventListener('click', onDocumentClick);
  document.addEventListener('keydown', onKeydown);
});

onUnmounted(() => {
  document.removeEventListener('click', onDocumentClick);
  document.removeEventListener('keydown', onKeydown);
});
</script>

<template>
  <div class="task-queue" @click.stop>
    <!-- 紧凑指示器（常驻） -->
    <span
      class="task-queue__indicator"
      :class="{ 'task-queue__indicator--active': expanded }"
      :title="t('main.taskQueue.title')"
      @click="toggle"
    >
      <!-- 分类图标 -->
      <svg
        class="task-queue__icon"
        :class="{ 'icon-spin': latestActiveTask?.status === 'running' }"
        viewBox="0 0 24 24"
        width="12"
        height="12"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
      >
        <path :d="iconPath" />
      </svg>

      <!-- 标题 -->
      <span class="task-queue__label">{{ label }}</span>

      <!-- 剩余任务数 -->
      <span
        v-if="extraCount > 0"
        class="task-queue__count"
        :title="t('main.taskQueue.moreTasks', { count: extraCount })"
      >
        +{{ extraCount }}
      </span>

      <!-- 底部进度条（仅活跃任务时显示） -->
      <span v-if="latestActiveTask" class="task-queue__bar">
        <span
          v-if="ratio != null"
          class="task-queue__bar-fill"
          :style="{ width: `${ratio * 100}%` }"
        ></span>
        <span v-else class="task-queue__bar-fill task-queue__bar-fill--indeterminate"></span>
      </span>
    </span>

    <!-- 展开面板 -->
    <TaskQueuePanel v-if="expanded" @close="expanded = false" />
  </div>
</template>

<style scoped>
.task-queue {
  position: relative;
  display: flex;
  align-items: center;
}

/* ── 紧凑指示器 ────────────────────────────────────────────────────── */
.task-queue__indicator {
  position: relative;
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 0 6px;
  height: 26px;
  cursor: pointer;
  border-radius: 3px;
  transition: background 0.15s ease;
}

.task-queue__indicator:hover,
.task-queue__indicator--active {
  background: rgba(255, 255, 255, 0.15);
}

.task-queue__icon {
  flex-shrink: 0;
}

.task-queue__label {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 180px;
}

.task-queue__count {
  flex-shrink: 0;
  padding: 0 5px;
  border-radius: 9999px;
  background: rgba(255, 255, 255, 0.2);
  font-size: 10px;
  line-height: 16px;
}

/* ── 底部进度条 ────────────────────────────────────────────────────── */
.task-queue__bar {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  height: 2px;
  background: rgba(255, 255, 255, 0.2);
  overflow: hidden;
}

.task-queue__bar-fill {
  display: block;
  height: 100%;
  background: rgba(255, 255, 255, 0.9);
  transition: width 0.2s ease;
}

.task-queue__bar-fill--indeterminate {
  width: 40%;
  animation: task-queue-indeterminate 1.4s ease-in-out infinite;
}

@keyframes task-queue-indeterminate {
  0% { transform: translateX(-100%); }
  100% { transform: translateX(250%); }
}

/* ── 旋转动画 ──────────────────────────────────────────────────────── */
.icon-spin {
  animation: task-queue-spin 1s linear infinite;
}

@keyframes task-queue-spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}
</style>
