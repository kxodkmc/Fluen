/**
 * useReferenceMarks —— 文献标记的 CRUD 与状态管理。
 *
 * 职责：
 *   1. 加载指定文献的全部标记（`reference_list_marks`）
 *   2. 创建标记（`reference_create_mark`）
 *   3. 更新附注 / 颜色（`reference_update_mark`）
 *   4. 删除标记（`reference_delete_mark`）
 *
 * 状态为模块级单例（跨组件共享），与 useReferenceReader 配合：
 * ReferenceReader 加载文献后调用 loadMarks，MarksPanel 直接读取 state.marks。
 *
 * @module composables/useReferenceMarks
 */

import { reactive, readonly } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type {
  Mark,
  MarkAnchor,
  MarkColor,
} from '../types/marks';
import { useProject } from './useProject';

// ---------------------------------------------------------------------------
// 状态
// ---------------------------------------------------------------------------

interface MarksState {
  /** 是否正在加载。 */
  loading: boolean;
  /** 错误消息（null 表示无错误）。 */
  error: string | null;
  /** 当前文献的全部标记。 */
  marks: Mark[];
  /** 当前加载的文献 ID（用于判断是否切换文献）。 */
  referenceId: string | null;
}

const _state = reactive<MarksState>({
  loading: false,
  error: null,
  marks: [],
  referenceId: null,
});

// ---------------------------------------------------------------------------
// composable
// ---------------------------------------------------------------------------

export function useReferenceMarks() {
  const { currentProject } = useProject();

  /**
   * 加载指定文献的全部标记。
   *
   * 切换文献时自动清空旧数据。文件不存在时返回空数组。
   */
  async function loadMarks(referenceId: string): Promise<void> {
    if (!currentProject.value) {
      _state.error = '未打开项目';
      return;
    }

    _state.loading = true;
    _state.error = null;
    _state.referenceId = referenceId;
    _state.marks = [];

    try {
      _state.marks = await invoke<Mark[]>('reference_list_marks', {
        projectPath: currentProject.value.project_path,
        referenceId,
      });
    } catch (err) {
      _state.error = err instanceof Error ? err.message : String(err);
    } finally {
      _state.loading = false;
    }
  }

  /**
   * 创建标记。
   *
   * @param referenceId 文献 ID
   * @param anchor 锚点（含 block_key / fingerprint / range）
   * @param text 划线文本快照
   * @param color 颜色
   * @returns 创建后的 Mark（含服务端生成的 id）
   */
  async function createMark(
    referenceId: string,
    anchor: MarkAnchor,
    text: string,
    color: MarkColor,
  ): Promise<Mark | null> {
    if (!currentProject.value) {
      _state.error = '未打开项目';
      return null;
    }
    try {
      const created = await invoke<Mark>('reference_create_mark', {
        projectPath: currentProject.value.project_path,
        referenceId,
        anchor,
        text,
        color,
      });
      _state.marks.push(created);
      return created;
    } catch (err) {
      _state.error = err instanceof Error ? err.message : String(err);
      return null;
    }
  }

  /**
   * 更新标记（附注 / 颜色）。
   */
  async function updateMark(
    referenceId: string,
    markId: string,
    note: string | null,
    color: MarkColor,
  ): Promise<Mark | null> {
    if (!currentProject.value) {
      _state.error = '未打开项目';
      return null;
    }
    try {
      const updated = await invoke<Mark>('reference_update_mark', {
        projectPath: currentProject.value.project_path,
        referenceId,
        markId,
        note,
        color,
      });
      const idx = _state.marks.findIndex((m) => m.id === markId);
      if (idx >= 0) _state.marks[idx] = updated;
      return updated;
    } catch (err) {
      _state.error = err instanceof Error ? err.message : String(err);
      return null;
    }
  }

  /**
   * 删除标记。
   */
  async function deleteMark(referenceId: string, markId: string): Promise<boolean> {
    if (!currentProject.value) {
      _state.error = '未打开项目';
      return false;
    }
    try {
      await invoke('reference_delete_mark', {
        projectPath: currentProject.value.project_path,
        referenceId,
        markId,
      });
      _state.marks = _state.marks.filter((m) => m.id !== markId);
      return true;
    } catch (err) {
      _state.error = err instanceof Error ? err.message : String(err);
      return false;
    }
  }

  /** 清空状态（关闭阅读器时调用）。 */
  function clear(): void {
    _state.loading = false;
    _state.error = null;
    _state.marks = [];
    _state.referenceId = null;
  }

  return {
    state: readonly(_state),
    loadMarks,
    createMark,
    updateMark,
    deleteMark,
    clear,
  };
}
