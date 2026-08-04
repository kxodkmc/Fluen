<script setup lang="ts">
/**
 * TaskQueuePanel — 任务队列展开面板。
 *
 * 点击状态栏指示器后弹出，展示全部任务：
 *   - 活跃任务（pending/running）在前，按创建时间降序（后进先出）
 *   - 已完成任务（completed/failed/cancelled）在后，默认折叠可展开
 *   - 每行：状态图标、标题、详情、进度条、状态标签
 *
 * 纯渲染组件，数据来自 useTaskQueue composable。
 */
import { ref, computed } from 'vue';
import { useTaskQueue, getProgressRatio, formatProgressText } from './useTaskQueue';
import { useI18n } from '../../../../../i18n';
import type { TaskStatus } from './types';

const emit = defineEmits<{
  (e: 'close'): void;
}>();

const { activeTasks, finishedTasks, clearFinished } = useTaskQueue();
const { t } = useI18n();

/** 已完成任务是否展开（默认折叠）。 */
const finishedExpanded = ref(false);

/** 是否有已完成任务可清除。 */
const hasFinished = computed(() => finishedTasks.value.length > 0);

/** 是否有活跃任务。 */
const hasActive = computed(() => activeTasks.value.length > 0);

/** 状态标签文本。 */
function statusLabel(status: TaskStatus): string {
  return t(`main.taskQueue.status.${status}`);
}

/** 已完成区域的折叠/展开按钮文本。 */
const finishedToggleText = computed(() =>
  finishedExpanded.value
    ? t('main.taskQueue.collapseFinished')
    : t('main.taskQueue.expandFinished', { count: finishedTasks.value.length }),
);

/** 切换已完成区域展开状态。 */
function toggleFinished(): void {
  finishedExpanded.value = !finishedExpanded.value;
}

/** 状态图标 SVG path。 */
function statusIcon(status: TaskStatus): string {
  switch (status) {
    case 'pending': return 'M12 7v5l3 3 M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0z';
    case 'running': return 'M21 12a9 9 0 1 1-6.219-8.56';
    case 'completed': return 'M5 13l4 4L19 7';
    case 'failed': return 'M18 6 6 18M6 6l12 12';
    case 'cancelled': return 'M5 12h14';
  }
}
</script>

<template>
  <div class="task-panel" @click.stop>
    <!-- 头部 -->
    <div class="task-panel__header">
      <span class="task-panel__title">{{ t('main.taskQueue.title') }}</span>
      <div class="task-panel__actions">
        <button
          v-if="hasFinished"
          class="task-panel__btn"
          :title="t('main.taskQueue.clear')"
          @click="clearFinished"
        >
          {{ t('main.taskQueue.clear') }}
        </button>
        <button
          class="task-panel__btn task-panel__btn--icon"
          :title="t('main.taskQueue.close')"
          @click="emit('close')"
        >
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            <path d="M18 6 6 18M6 6l12 12" />
          </svg>
        </button>
      </div>
    </div>

    <!-- 任务列表 -->
    <div class="task-panel__list">
      <!-- 空状态 -->
      <div v-if="!hasActive && !hasFinished" class="task-panel__empty">
        {{ t('main.taskQueue.empty') }}
      </div>

      <!-- 活跃任务（后进先出） -->
      <template v-if="hasActive">
        <div
          v-for="task in activeTasks"
          :key="task.id"
          class="task-row"
          :class="`task-row--${task.status}`"
        >
          <!-- 状态图标 -->
          <div class="task-row__icon">
            <svg
              v-if="task.status === 'running'"
              class="icon-spin"
              viewBox="0 0 24 24"
              width="14"
              height="14"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
            >
              <path :d="statusIcon(task.status)" />
            </svg>
            <svg
              v-else
              viewBox="0 0 24 24"
              width="14"
              height="14"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path :d="statusIcon(task.status)" />
            </svg>
          </div>

          <!-- 主体 -->
          <div class="task-row__body">
            <div class="task-row__head">
              <span class="task-row__title" :title="task.title">{{ task.title }}</span>
              <span class="task-row__status" :class="`task-row__status--${task.status}`">
                {{ statusLabel(task.status) }}
              </span>
            </div>

            <!-- 进度条（活跃任务） -->
            <div
              v-if="task.status === 'pending' || task.status === 'running'"
              class="task-row__bar"
            >
              <div
                v-if="getProgressRatio(task.progress) != null"
                class="task-row__bar-fill"
                :style="{ width: `${(getProgressRatio(task.progress) ?? 0) * 100}%` }"
              ></div>
              <div v-else class="task-row__bar-fill task-row__bar-fill--indeterminate"></div>
            </div>

            <!-- 详情 / 进度文本 -->
            <span v-if="formatProgressText(task)" class="task-row__detail">
              {{ formatProgressText(task) }}
            </span>

            <!-- 错误信息 -->
            <span v-if="task.error" class="task-row__error" :title="task.error">{{ task.error }}</span>
          </div>
        </div>
      </template>

      <!-- 已完成任务折叠区 -->
      <template v-if="hasFinished">
        <!-- 折叠/展开按钮 -->
        <button class="finished-toggle" @click="toggleFinished">
          <svg
            class="finished-toggle__icon"
            :class="{ 'finished-toggle__icon--expanded': finishedExpanded }"
            viewBox="0 0 24 24"
            width="10"
            height="10"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
          >
            <path d="m6 9 6 6 6-6" />
          </svg>
          <span>{{ finishedToggleText }}</span>
        </button>

        <!-- 已完成任务列表（展开时显示） -->
        <template v-if="finishedExpanded">
          <div
            v-for="task in finishedTasks"
            :key="task.id"
            class="task-row task-row--finished"
            :class="`task-row--${task.status}`"
          >
            <!-- 状态图标 -->
            <div class="task-row__icon">
              <svg
                viewBox="0 0 24 24"
                width="14"
                height="14"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path :d="statusIcon(task.status)" />
              </svg>
            </div>

            <!-- 主体 -->
            <div class="task-row__body">
              <div class="task-row__head">
                <span class="task-row__title" :title="task.title">{{ task.title }}</span>
                <span class="task-row__status" :class="`task-row__status--${task.status}`">
                  {{ statusLabel(task.status) }}
                </span>
              </div>

              <!-- 详情 / 进度文本 -->
              <span v-if="formatProgressText(task)" class="task-row__detail">
                {{ formatProgressText(task) }}
              </span>

              <!-- 错误信息 -->
              <span v-if="task.error" class="task-row__error" :title="task.error">{{ task.error }}</span>
            </div>
          </div>
        </template>
      </template>
    </div>
  </div>
</template>

<style scoped>
.task-panel {
  position: absolute;
  bottom: 100%;
  right: 0;
  width: 340px;
  max-width: calc(100vw - 16px);
  max-height: 360px;
  display: flex;
  flex-direction: column;
  background: var(--fluen-canvas);
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  box-shadow: var(--fluen-shadow-modal);
  z-index: 200;
  overflow: hidden;
}

/* ── 头部 ──────────────────────────────────────────────────────────── */
.task-panel__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  border-bottom: 1px solid var(--fluen-hairline);
  background: var(--fluen-surface);
}

.task-panel__title {
  font-family: var(--fluen-font-sans);
  font-size: 0.78rem;
  font-weight: 600;
  color: var(--fluen-charcoal);
}

.task-panel__actions {
  display: flex;
  align-items: center;
  gap: 4px;
}

.task-panel__btn {
  padding: 2px 8px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--fluen-steel);
  font-family: var(--fluen-font-sans);
  font-size: 0.7rem;
  cursor: pointer;
  transition: all 0.15s ease;
}

.task-panel__btn:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.task-panel__btn--icon {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 2px;
}

/* ── 列表 ──────────────────────────────────────────────────────────── */
.task-panel__list {
  flex: 1;
  overflow-y: auto;
}

.task-panel__empty {
  padding: 24px 12px;
  text-align: center;
  font-size: 0.75rem;
  color: var(--fluen-stone);
}

/* ── 任务行 ────────────────────────────────────────────────────────── */
.task-row {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--fluen-hairline-soft);
}

.task-row:last-child {
  border-bottom: none;
}

/* 已完成任务行：略带淡化效果 */
.task-row--finished {
  opacity: 0.85;
}

/* ── 已完成折叠区 ──────────────────────────────────────────────────── */
.finished-toggle {
  display: flex;
  align-items: center;
  gap: 4px;
  width: 100%;
  padding: 6px 12px;
  border: none;
  border-top: 1px solid var(--fluen-hairline-soft);
  background: var(--fluen-surface);
  color: var(--fluen-steel);
  font-family: var(--fluen-font-sans);
  font-size: 0.7rem;
  cursor: pointer;
  transition: background 0.15s ease;
}

.finished-toggle:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.finished-toggle__icon {
  flex-shrink: 0;
  transition: transform 0.2s ease;
}

.finished-toggle__icon--expanded {
  transform: rotate(180deg);
}

.task-row__icon {
  flex-shrink: 0;
  margin-top: 1px;
  color: var(--fluen-steel);
}

.task-row--running .task-row__icon {
  color: var(--fluen-accent);
}

.task-row--completed .task-row__icon {
  color: var(--fluen-success-text);
}

.task-row--failed .task-row__icon {
  color: var(--fluen-error);
}

.task-row--cancelled .task-row__icon {
  color: var(--fluen-stone);
}

.task-row__body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.task-row__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.task-row__title {
  font-family: var(--fluen-font-sans);
  font-size: 0.75rem;
  color: var(--fluen-charcoal);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.task-row__status {
  flex-shrink: 0;
  font-size: 0.65rem;
  padding: 1px 6px;
  border-radius: 9999px;
  white-space: nowrap;
}

.task-row__status--pending {
  background: var(--fluen-surface-deep);
  color: var(--fluen-steel);
}

.task-row__status--running {
  background: var(--fluen-info-bg);
  color: var(--fluen-accent);
}

.task-row__status--completed {
  background: var(--fluen-success-bg);
  color: var(--fluen-success-text);
}

.task-row__status--failed {
  background: var(--fluen-error-bg);
  color: var(--fluen-error);
}

.task-row__status--cancelled {
  background: var(--fluen-surface-deep);
  color: var(--fluen-stone);
}

/* ── 进度条 ────────────────────────────────────────────────────────── */
.task-row__bar {
  height: 3px;
  border-radius: 9999px;
  background: var(--fluen-hairline);
  overflow: hidden;
}

.task-row__bar-fill {
  height: 100%;
  border-radius: 9999px;
  background: var(--fluen-accent);
  transition: width 0.2s ease;
}

.task-row--completed .task-row__bar-fill {
  background: var(--fluen-success-text);
}

.task-row--failed .task-row__bar-fill {
  background: var(--fluen-error);
}

.task-row__bar-fill--indeterminate {
  width: 40%;
  animation: task-indeterminate 1.4s ease-in-out infinite;
}

@keyframes task-indeterminate {
  0% { transform: translateX(-100%); }
  100% { transform: translateX(250%); }
}

/* ── 详情 / 错误 ───────────────────────────────────────────────────── */
.task-row__detail {
  font-size: 0.68rem;
  color: var(--fluen-stone);
}

.task-row__error {
  font-size: 0.68rem;
  color: var(--fluen-error);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* ── 旋转动画 ──────────────────────────────────────────────────────── */
.icon-spin {
  animation: task-spin 1s linear infinite;
}

@keyframes task-spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}
</style>
