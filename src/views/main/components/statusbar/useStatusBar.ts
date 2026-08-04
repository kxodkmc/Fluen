/**
 * 状态栏系统 — composable。
 *
 * 模块级 reactive Map 维护全部条目，跨组件共享：
 *   - useStatusBar()：获取只读的 left / right 响应式列表 + 操作函数
 *   - useStatusEntry(id, initial)：声明式注册，组件作用域销毁时自动移除
 *
 * 设计要点：
 *   - set 做 upsert（首次 = 注册，再次 = 全量更新）
 *   - patch 做局部更新（高频场景如光标移动只传 label）
 *   - remove 直接删除（隐藏 = 移除，不留垃圾状态）
 *   - sort 在 computed 内部显式拷贝，不依赖外部行为
 */

import { reactive, computed, onScopeDispose } from 'vue';
import type { StatusEntry, StatusTone } from './types';

/* ── 模块级单例状态 ────────────────────────────────────────────────────── */
const entries = reactive(new Map<string, StatusEntry>());

/* ── 内部工具 ──────────────────────────────────────────────────────────── */

/** 按 order 升序排列，返回新数组。 */
function sorted(list: StatusEntry[]): StatusEntry[] {
  return [...list].sort((a, b) => (a.order ?? 0) - (b.order ?? 0));
}

/* ── 操作函数 ──────────────────────────────────────────────────────────── */

/** 注册或更新条目（upsert）。 */
function set(id: string, entry: Omit<StatusEntry, 'id'>): void {
  entries.set(id, { ...entry, id });
}

/** 局部更新已注册条目（不存在则忽略并 warn）。 */
function patch(
  id: string,
  partial: Partial<Omit<StatusEntry, 'id' | 'position'>>,
): void {
  const existing = entries.get(id);
  if (!existing) {
    if (import.meta.env.DEV) {
      console.warn(`[statusbar] patch 失败：条目 "${id}" 不存在`);
    }
    return;
  }
  Object.assign(existing, partial);
}

/** 移除条目。 */
function remove(id: string): void {
  entries.delete(id);
}

/** 查询条目是否已注册。 */
function has(id: string): boolean {
  return entries.has(id);
}

/* ── Composable ────────────────────────────────────────────────────────── */

export interface UseStatusBarReturn {
  /** 左侧条目（只读响应式，按 order 升序）。 */
  left: Readonly<typeof left>;
  /** 右侧条目（只读响应式，按 order 升序）。 */
  right: Readonly<typeof right>;
  /** 注册或更新条目（upsert）。 */
  set: typeof set;
  /** 局部更新条目。 */
  patch: typeof patch;
  /** 移除条目。 */
  remove: typeof remove;
  /** 查询条目是否存在。 */
  has: typeof has;
}

const left = computed(() =>
  sorted(
    [...entries.values()].filter((e) => e.position === 'left'),
  ),
);

const right = computed(() =>
  sorted(
    [...entries.values()].filter((e) => e.position === 'right'),
  ),
);

/** 状态栏 composable — 获取响应式列表与操作函数。 */
export function useStatusBar(): UseStatusBarReturn {
  return { left, right, set, patch, remove, has };
}

/* ── 声明式注册 ────────────────────────────────────────────────────────── */

export interface UseStatusEntryReturn {
  /** 局部更新此条目。 */
  patch: (partial: Partial<Omit<StatusEntry, 'id' | 'position'>>) => void;
  /** 手动移除此条目（通常不需要，作用域销毁时自动清理）。 */
  remove: () => void;
  /** 便捷更新 label（最高频操作）。 */
  setLabel: (label: string) => void;
  /** 便捷更新 tone。 */
  setTone: (tone: StatusTone) => void;
}

/**
 * 声明式注册状态栏条目。
 *
 * 在组件 setup 或 effect scope 中调用，注册条目；
 * 当作用域销毁时自动移除条目，无需手动清理。
 *
 * @example
 * ```ts
 * const { setLabel } = useStatusEntry('editor.cursor', {
 *   label: 'Ln 1, Col 1',
 *   position: 'right',
 *   order: 100,
 * });
 * // 光标移动时
 * setLabel(`Ln ${line}, Col ${col}`);
 * ```
 */
export function useStatusEntry(
  id: string,
  initial: Omit<StatusEntry, 'id'>,
): UseStatusEntryReturn {
  set(id, initial);
  onScopeDispose(() => remove(id));

  return {
    patch: (partial) => patch(id, partial),
    remove: () => remove(id),
    setLabel: (label) => patch(id, { label }),
    setTone: (tone) => patch(id, { tone }),
  };
}
