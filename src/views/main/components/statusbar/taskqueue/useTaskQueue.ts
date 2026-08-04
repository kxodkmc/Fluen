/**
 * 任务队列通知系统 — composable。
 *
 * 模块级 reactive Map 维护全部任务，跨组件共享：
 *   - useTaskQueue()：获取响应式任务列表 + 操作函数
 *   - useTask(id, initial)：声明式注册，作用域销毁时自动移除
 *
 * 设计要点：
 *   - register 做 upsert（首次 = 注册，再次 = 全量更新，保留原 createdAt）
 *   - update 做局部更新（高频进度更新只传 progress / detail）
 *   - 便捷生命周期：start / complete / fail / cancel
 *   - 已完成任务自动保留最近 MAX_FINISHED 条，超出时淘汰最早的
 *
 * 使用示例：
 * ```ts
 * const { setProgress, complete } = useTask('ocr.import', {
 *   title: 'OCR 识别中',
 *   status: 'running',
 *   category: 'ocr',
 *   progress: { current: 0, total: 30 },
 * });
 * setProgress(12, 30);
 * complete();
 * ```
 */

import { reactive, computed, onScopeDispose } from 'vue';
import type { TaskEntry, TaskProgress } from './types';

/* ── 配置 ─────────────────────────────────────────────────────────────── */

/** 已完成任务最大保留数量（超出时淘汰最早的）。 */
const MAX_FINISHED = 20;

/* ── 分类图标 SVG path（24×24 viewBox）───────────────────────────────── */

const CATEGORY_ICONS: Record<string, string> = {
  ocr: 'M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z M14 2v6h6',
  knowledge: 'M4 19.5A2.5 2.5 0 0 1 6.5 17H20 M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z',
  import: 'M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4 M7 10l5 5 5-5 M12 15V3',
};

/** 默认任务图标（时钟）。 */
const DEFAULT_TASK_ICON = 'M12 7v5l3 3 M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0z';

/* ── 工具函数 ────────────────────────────────────────────────────────── */

/** 判断是否为活跃任务（待执行或执行中）。 */
function isActive(task: TaskEntry): boolean {
  return task.status === 'pending' || task.status === 'running';
}

/** 淘汰超量的已完成任务。 */
function pruneFinished(): void {
  const finished = [...tasks.values()]
    .filter((t) => !isActive(t))
    .sort((a, b) => (b.finishedAt ?? b.createdAt) - (a.finishedAt ?? a.createdAt));
  for (let i = MAX_FINISHED; i < finished.length; i++) {
    tasks.delete(finished[i].id);
  }
}

/* ── 公共工具函数 ────────────────────────────────────────────────────── */

/** 计算进度比例 0..1，无明确进度时返回 null（不确定进度）。 */
export function getProgressRatio(progress?: TaskProgress): number | null {
  if (!progress) return null;
  if (progress.ratio != null) return Math.min(1, Math.max(0, progress.ratio));
  if (progress.current != null && progress.total) {
    return Math.min(1, Math.max(0, progress.current / progress.total));
  }
  return null;
}

/** 获取分类图标 SVG path。 */
export function getCategoryIcon(category?: string): string {
  if (category && CATEGORY_ICONS[category]) return CATEGORY_ICONS[category];
  return DEFAULT_TASK_ICON;
}

/** 格式化进度文本（如「12/30」或「45%」），无明确进度时返回 null。 */
export function formatProgressText(task: TaskEntry): string | null {
  if (task.detail) return task.detail;
  if (!task.progress) return null;
  if (task.progress.current != null && task.progress.total) {
    return `${task.progress.current}/${task.progress.total}`;
  }
  const r = getProgressRatio(task.progress);
  return r != null ? `${Math.round(r * 100)}%` : null;
}

/* ── 模块级单例状态 ──────────────────────────────────────────────────── */

const tasks = reactive(new Map<string, TaskEntry>());

/* ── 操作函数 ────────────────────────────────────────────────────────── */

/** 注册任务（upsert，保留原 createdAt）。 */
function register(id: string, initial: Omit<TaskEntry, 'id' | 'createdAt'>): void {
  const existing = tasks.get(id);
  const createdAt = existing?.createdAt ?? Date.now();
  tasks.set(id, { ...initial, id, createdAt });
}

/** 局部更新任务（不存在则忽略并 warn）。 */
function update(id: string, partial: Partial<Omit<TaskEntry, 'id' | 'createdAt'>>): void {
  const existing = tasks.get(id);
  if (!existing) {
    if (import.meta.env.DEV) {
      console.warn(`[taskqueue] update 失败：任务 "${id}" 不存在`);
    }
    return;
  }
  Object.assign(existing, partial);
}

/** 更新进度（便捷函数）。 */
function setProgress(id: string, current: number, total: number): void {
  update(id, { progress: { current, total } });
}

/** 更新标准化进度比例（便捷函数）。 */
function setRatio(id: string, ratio: number): void {
  update(id, { progress: { ratio } });
}

/** 标记任务为执行中。 */
function start(id: string): void {
  update(id, { status: 'running' });
}

/** 标记任务完成。 */
function complete(id: string): void {
  const existing = tasks.get(id);
  if (!existing) return;
  existing.status = 'completed';
  existing.finishedAt = Date.now();
  pruneFinished();
}

/** 标记任务失败。 */
function fail(id: string, error?: string): void {
  const existing = tasks.get(id);
  if (!existing) return;
  existing.status = 'failed';
  existing.error = error;
  existing.finishedAt = Date.now();
  pruneFinished();
}

/** 取消任务。 */
function cancel(id: string): void {
  const existing = tasks.get(id);
  if (!existing) return;
  existing.status = 'cancelled';
  existing.finishedAt = Date.now();
  pruneFinished();
}

/** 移除任务。 */
function remove(id: string): void {
  tasks.delete(id);
}

/** 清除所有已完成任务。 */
function clearFinished(): void {
  for (const [id, task] of tasks) {
    if (!isActive(task)) tasks.delete(id);
  }
}

/* ── 响应式派生 ──────────────────────────────────────────────────────── */

/** 活跃任务列表（按创建时间降序）。 */
const activeTasks = computed(() =>
  [...tasks.values()]
    .filter(isActive)
    .sort((a, b) => b.createdAt - a.createdAt),
);

/** 已完成任务列表（按完成时间降序）。 */
const finishedTasks = computed(() =>
  [...tasks.values()]
    .filter((t) => !isActive(t))
    .sort((a, b) => (b.finishedAt ?? 0) - (a.finishedAt ?? 0)),
);

/** 最新活跃任务（无活跃任务时为 null）。 */
const latestActiveTask = computed(() => activeTasks.value[0] ?? null);

/** 活跃任务数。 */
const activeCount = computed(() => activeTasks.value.length);

/* ── Composable ──────────────────────────────────────────────────────── */

export interface UseTaskQueueReturn {
  /** 活跃任务列表（按创建时间降序）。 */
  activeTasks: Readonly<typeof activeTasks>;
  /** 已完成任务列表（按完成时间降序）。 */
  finishedTasks: Readonly<typeof finishedTasks>;
  /** 最新活跃任务。 */
  latestActiveTask: Readonly<typeof latestActiveTask>;
  /** 活跃任务数。 */
  activeCount: Readonly<typeof activeCount>;
  /** 注册任务（upsert）。 */
  register: typeof register;
  /** 局部更新任务。 */
  update: typeof update;
  /** 更新进度。 */
  setProgress: typeof setProgress;
  /** 更新标准化进度比例。 */
  setRatio: typeof setRatio;
  /** 标记为执行中。 */
  start: typeof start;
  /** 标记完成。 */
  complete: typeof complete;
  /** 标记失败。 */
  fail: typeof fail;
  /** 取消任务。 */
  cancel: typeof cancel;
  /** 移除任务。 */
  remove: typeof remove;
  /** 清除已完成任务。 */
  clearFinished: typeof clearFinished;
}

/** 任务队列 composable — 获取响应式任务列表与操作函数。 */
export function useTaskQueue(): UseTaskQueueReturn {
  return {
    activeTasks,
    finishedTasks,
    latestActiveTask,
    activeCount,
    register,
    update,
    setProgress,
    setRatio,
    start,
    complete,
    fail,
    cancel,
    remove,
    clearFinished,
  };
}

/* ── 声明式注册 ──────────────────────────────────────────────────────── */

export interface UseTaskReturn {
  /** 局部更新此任务。 */
  update: (partial: Partial<Omit<TaskEntry, 'id' | 'createdAt'>>) => void;
  /** 更新进度。 */
  setProgress: (current: number, total: number) => void;
  /** 更新标准化进度比例。 */
  setRatio: (ratio: number) => void;
  /** 标记为执行中。 */
  start: () => void;
  /** 标记完成。 */
  complete: () => void;
  /** 标记失败。 */
  fail: (error?: string) => void;
  /** 取消任务。 */
  cancel: () => void;
  /** 手动移除（通常不需要，作用域销毁时自动清理）。 */
  remove: () => void;
}

/**
 * 声明式注册任务。
 *
 * 在组件 setup 或 effect scope 中调用，注册任务；
 * 当作用域销毁时自动移除任务。
 */
export function useTask(
  id: string,
  initial: Omit<TaskEntry, 'id' | 'createdAt'>,
): UseTaskReturn {
  register(id, initial);
  onScopeDispose(() => remove(id));

  return {
    update: (partial) => update(id, partial),
    setProgress: (current, total) => setProgress(id, current, total),
    setRatio: (ratio) => setRatio(id, ratio),
    start: () => start(id),
    complete: () => complete(id),
    fail: (error) => fail(id, error),
    cancel: () => cancel(id),
    remove: () => remove(id),
  };
}
