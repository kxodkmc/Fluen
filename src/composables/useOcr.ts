/**
 * OCR 执行 composable。
 *
 * 封装 `ocr_recognize` 命令调用与 `ocr:progress` 事件监听，
 * 提供响应式的进度状态与取消能力。
 *
 * @example
 * ```ts
 * const { recognize, cancel, progress, isRunning } = useOcr();
 *
 * // 监听进度
 * watch(progress, (p) => {
 *   if (p) console.log(p.state, p.extracted_pages);
 * });
 *
 * // 执行识别
 * try {
 *   const result = await recognize('/path/to/file.pdf');
 *   console.log(result.pages);
 * } catch (err) {
 *   console.error('OCR 失败:', err);
 * }
 *
 * // 取消
 * await cancel();
 * ```
 */

import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { OcrProgress, OcrResult } from '../types/aiServices';

/** OCR 事件名。 */
const EVENT_OCR_PROGRESS = 'ocr:progress';

export function useOcr() {
  /** 当前进度（`null` 表示无活跃任务）。 */
  const progress = ref<OcrProgress | null>(null);
  /** 是否正在执行。 */
  const isRunning = ref(false);
  /** 最后一次错误信息。 */
  const error = ref<string | null>(null);

  /** 事件监听取消函数。 */
  let unlistenFn: UnlistenFn | null = null;

  /**
   * 执行 OCR 识别。
   *
   * @param filePath 本地文件路径或 URL
   * @returns OCR 结果
   */
  async function recognize(filePath: string): Promise<OcrResult> {
    if (!('__TAURI_INTERNALS__' in window)) {
      throw new Error('OCR 功能仅在 Tauri 环境下可用');
    }

    // 重置状态
    progress.value = null;
    error.value = null;
    isRunning.value = true;

    // 注册事件监听
    unlistenFn = await listen<OcrProgress>(EVENT_OCR_PROGRESS, (e) => {
      progress.value = e.payload;
    });

    try {
      const result = await invoke<OcrResult>('ocr_recognize', { filePath });
      return result;
    } catch (err) {
      const msg = typeof err === 'string' ? err : String(err);
      error.value = msg;
      throw err;
    } finally {
      isRunning.value = false;
      // 注销事件监听
      if (unlistenFn) {
        unlistenFn();
        unlistenFn = null;
      }
    }
  }

  /**
   * 取消当前活跃的 OCR 任务。
   */
  async function cancel(): Promise<void> {
    if (!('__TAURI_INTERNALS__' in window)) return;
    try {
      await invoke('ocr_cancel');
    } catch (err) {
      console.error('[useOcr] 取消失败:', err);
    }
  }

  return {
    progress,
    isRunning,
    error,
    recognize,
    cancel,
  };
}
