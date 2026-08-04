/**
 * 任务队列通知系统 — 公共 API。
 *
 * 模块结构：
 *   - types.ts              类型定义
 *   - useTaskQueue.ts       composable + 模块级状态 + 工具函数
 *   - TaskQueueIndicator.vue 状态栏紧凑指示器
 *   - TaskQueuePanel.vue    展开面板
 *
 * 使用方式：
 *   import { useTask, useTaskQueue } from './taskqueue';
 *   const { setProgress, complete } = useTask('ocr.import', { ... });
 */

export { default as TaskQueueIndicator } from './TaskQueueIndicator.vue';
export { default as TaskQueuePanel } from './TaskQueuePanel.vue';
export { useTaskQueue, useTask } from './useTaskQueue';
export {
  getProgressRatio,
  getCategoryIcon,
  formatProgressText,
} from './useTaskQueue';
export type {
  UseTaskQueueReturn,
  UseTaskReturn,
} from './useTaskQueue';
export type { TaskEntry, TaskStatus, TaskProgressStyle, TaskProgress } from './types';
