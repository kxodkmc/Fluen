/**
 * 最近打开文章项目状态管理 composable（单例模式）。
 *
 * 响应式状态声明在模块级别，确保所有调用 `useRecentProjects()`
 * 的组件共享同一份状态——欢迎页、设置页、主视图中的最近项目
 * 列表始终同步。
 *
 * 状态变更后通过 `refresh()` 重新拉取后端数据。
 *
 * @example
 * ```ts
 * const { entries, refresh, recordOpen, removeEntry } = useRecentProjects();
 * await refresh();
 * await recordOpen({ project_path, title, author, opened_at });
 * ```
 */

import { ref, readonly } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type {
  RecentProjectEntry,
  RecentProjectsData,
} from '../types/recentProjects';

// ── 模块级状态（单例） ──────────────────────────────────────────────

const _entries = ref<RecentProjectEntry[]>([]);
const _isLoading = ref(false);
const _error = ref<string | null>(null);

/** 是否已至少加载过一次（用于避免重复加载）。 */
let _loaded = false;

// ── composable ─────────────────────────────────────────────────────

export function useRecentProjects() {
  /**
   * 从后端拉取最近项目列表。
   *
   * 首次调用后置 `_loaded = true`，后续调用通过 `force` 参数控制是否强制刷新。
   * 错误信息写入 `_error`，不抛出异常，避免阻塞 UI。
   */
  async function refresh(force = false): Promise<void> {
    if (_isLoading.value) return;
    if (_loaded && !force) return;

    _isLoading.value = true;
    _error.value = null;
    try {
      const data = await invoke<RecentProjectsData>('recent_projects_list');
      _entries.value = data.entries;
      _loaded = true;
    } catch (err) {
      _error.value = err instanceof Error ? err.message : String(err);
      // 加载失败时保留现有状态，避免清空已显示的列表
    } finally {
      _isLoading.value = false;
    }
  }

  /**
   * 记录一次项目打开。
   *
   * - 若 `project_path` 已存在，移除旧记录
   * - 将新记录插入到列表头部
   * - 按 `AppConfig.recent_projects_count` 自动裁剪
   *
   * 调用此方法后本地状态会立即反映变更（基于当前已知条目预判），
   * 同时通过后端持久化。错误时静默处理，不影响项目打开流程。
   */
  async function recordOpen(entry: RecentProjectEntry): Promise<void> {
    try {
      await invoke('recent_projects_record', { entry });
      // 乐观更新：本地立即反映变更
      _entries.value = _entries.value.filter(
        (e) => e.project_path !== entry.project_path,
      );
      _entries.value = [entry, ..._entries.value];
      _loaded = true;
    } catch (err) {
      // 静默处理：记录失败不应阻塞项目打开
      console.warn('[useRecentProjects] 记录打开失败:', err);
    }
  }

  /**
   * 移除一条记录。
   *
   * 路径失效或用户主动移除时调用。
   * 调用后本地状态会立即反映变更。
   */
  async function removeEntry(projectPath: string): Promise<void> {
    try {
      await invoke('recent_projects_remove', { projectPath });
      _entries.value = _entries.value.filter(
        (e) => e.project_path !== projectPath,
      );
    } catch (err) {
      console.warn('[useRecentProjects] 移除记录失败:', err);
    }
  }

  /**
   * 按最大数量裁剪列表。
   *
   * 用户在设置中调小 `recent_projects_count` 后调用。
   */
  async function trim(maxCount: number): Promise<void> {
    try {
      await invoke('recent_projects_trim', { maxCount });
      if (_entries.value.length > maxCount) {
        _entries.value = _entries.value.slice(0, maxCount);
      }
    } catch (err) {
      console.warn('[useRecentProjects] 裁剪列表失败:', err);
    }
  }

  return {
    // 状态（只读）
    entries: readonly(_entries),
    isLoading: readonly(_isLoading),
    error: readonly(_error),

    // 操作
    refresh,
    recordOpen,
    removeEntry,
    trim,
  };
}
