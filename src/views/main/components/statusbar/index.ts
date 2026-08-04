/**
 * 状态栏系统 — 公共 API。
 *
 * 模块结构：
 *   - types.ts        类型定义
 *   - useStatusBar.ts composable + 模块级状态
 *   - StatusBar.vue   纯渲染组件
 *
 * 使用方式：
 *   import { useStatusBar, useStatusEntry } from './statusbar';
 *   const { set, patch, remove } = useStatusBar();
 *   const { setLabel } = useStatusEntry('editor.cursor', { ... });
 */

export { default as StatusBar } from './StatusBar.vue';
export { useStatusBar, useStatusEntry } from './useStatusBar';
export { useProjectStatus } from './useProjectStatus';
export type {
  UseStatusBarReturn,
  UseStatusEntryReturn,
} from './useStatusBar';
export type { StatusEntry, StatusPosition, StatusTone } from './types';

/* ── 任务队列通知系统 ───────────────────────────────────────────────── */
export {
  TaskQueueIndicator,
  TaskQueuePanel,
  useTaskQueue,
  useTask,
  getProgressRatio,
  getCategoryIcon,
  formatProgressText,
} from './taskqueue';
export type {
  UseTaskQueueReturn,
  UseTaskReturn,
  TaskEntry,
  TaskStatus,
  TaskProgressStyle,
  TaskProgress,
} from './taskqueue';
