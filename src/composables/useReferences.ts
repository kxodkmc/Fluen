/**
 * 文献导入与管理 composable。
 *
 * 封装 Tauri invoke 调用与 `reference:*` 事件监听，提供：
 *   - 文献列表加载 / 删除 / 重试 / 修改标题
 *   - 单文件导入（await）与批量导入（fire-and-forget）
 *   - 导入进度通过任务队列通知系统（useTaskQueue）实时展示在状态栏
 *   - 取消导入任务
 *
 * 事件流设计：
 *   - `import_reference`（单文件）：await 返回结果，进度通过事件推送
 *   - `import_references`（批量）：立即返回 `ImportJobHandle`，进度和结果仅通过事件推送
 *
 * 任务队列集成：
 *   - 批量导入开始时注册任务（category: 'import'），进度 = 已完成文件数 / 总文件数
 *   - 事件实时更新任务进度与详情（当前文件名、OCR 页数等）
 *   - 任务完成时标记 completed / failed
 *
 * @example
 * ```ts
 * const {
 *   references, isImporting, activeJobId,
 *   loadReferences, importFiles, importSingle, cancelImport, deleteReference,
 * } = useReferences();
 *
 * // 加载列表
 * await loadReferences(projectPath);
 *
 * // 批量导入（进度自动推送至状态栏任务队列）
 * const handle = await importFiles(projectPath, ['/path/a.pdf', '/path/b.pdf']);
 *
 * // 取消
 * await cancelImport(projectPath, handle.job_id);
 * ```
 */

import { ref, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { useI18n } from '../i18n';
import { useTaskQueue } from '../views/main/components/statusbar/taskqueue/useTaskQueue';
import type {
  ConsistencyReport,
  ImportJobHandle,
  ImportProgressPayload,
  ImportStartedPayload,
  ImportCompletedPayload,
  ImportFailedPayload,
  JobCompletedPayload,
  ReferenceEntry,
} from '../types/references';

// ---------------------------------------------------------------------------
// 事件名常量（与后端 commands.rs 保持一致）
// ---------------------------------------------------------------------------

const EVENT_IMPORT_STARTED = 'reference:import_started';
const EVENT_IMPORT_PROGRESS = 'reference:import_progress';
const EVENT_IMPORT_COMPLETED = 'reference:import_completed';
const EVENT_IMPORT_FAILED = 'reference:import_failed';
const EVENT_JOB_COMPLETED = 'reference:job_completed';

/**
 * 检测当前是否运行在 Tauri 环境中。
 */
function isTauriEnvironment(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export function useReferences() {
  const { t } = useI18n();
  const { register, update, complete, fail: failTask, cancel: cancelTask } = useTaskQueue();

  /* ── 响应式状态 ─────────────────────────────────────────────────────── */

  /** 文献列表。 */
  const references = ref<ReferenceEntry[]>([]);

  /** 当前活跃的导入任务 ID（无活跃任务时为 null）。 */
  const activeJobId = ref<string | null>(null);

  /** 是否正在导入（单文件 await 或批量任务进行中）。 */
  const isImporting = ref(false);

  /** 最后一次错误信息。 */
  const error = ref<string | null>(null);

  /* ── 内部跟踪（不暴露，用于任务队列进度计算）────────────────────────── */

  /** reference_id → filename 映射（来自 import_started 事件）。 */
  const fileNames = new Map<string, string>();

  /** 当前任务在任务队列中的 ID。 */
  let currentTaskId: string | null = null;

  /** 总文件数。 */
  let totalCount = 0;

  /** 已完成文件数。 */
  let completedCount = 0;

  /** 已失败文件数。 */
  let failedCount = 0;

  /* ── 事件监听管理 ───────────────────────────────────────────────────── */

  /** 已注册的事件取消监听函数列表。 */
  const unlistenFns: UnlistenFn[] = [];

  /** 注册所有 `reference:*` 事件监听。 */
  async function setupEventListeners(): Promise<void> {
    if (!isTauriEnvironment()) return;

    unlistenFns.push(
      await listen<ImportStartedPayload>(EVENT_IMPORT_STARTED, (e) => {
        const { reference_id, filename } = e.payload;
        fileNames.set(reference_id, filename);
        if (currentTaskId) {
          update(currentTaskId, { detail: filename });
        }
      }),
    );

    unlistenFns.push(
      await listen<ImportProgressPayload>(EVENT_IMPORT_PROGRESS, (e) => {
        if (!currentTaskId) return;
        const { reference_id, stage, ocr_progress } = e.payload;
        const filename = fileNames.get(reference_id) ?? '';
        let detail = filename;
        if (stage === 'ocr' && ocr_progress?.extracted_pages != null && ocr_progress?.total_pages != null) {
          detail = `${filename} · ${t('main.sidebar.references.stageOcr')} ${ocr_progress.extracted_pages}/${ocr_progress.total_pages}`;
        } else if (stage === 'saving') {
          detail = `${filename} · ${t('main.sidebar.references.stageSaving')}`;
        }
        update(currentTaskId, { detail });
      }),
    );

    unlistenFns.push(
      await listen<ImportCompletedPayload>(EVENT_IMPORT_COMPLETED, (e) => {
        const { reference_id, entry } = e.payload;
        fileNames.delete(reference_id);
        completedCount++;
        if (currentTaskId) {
          update(currentTaskId, {
            progress: { current: completedCount + failedCount, total: totalCount },
          });
        }
        // 同步更新文献列表
        updateReferenceInList(entry);
      }),
    );

    unlistenFns.push(
      await listen<ImportFailedPayload>(EVENT_IMPORT_FAILED, (e) => {
        const { reference_id } = e.payload;
        fileNames.delete(reference_id);
        failedCount++;
        if (currentTaskId) {
          update(currentTaskId, {
            progress: { current: completedCount + failedCount, total: totalCount },
          });
        }
      }),
    );

    unlistenFns.push(
      await listen<JobCompletedPayload>(EVENT_JOB_COMPLETED, (e) => {
        if (currentTaskId) {
          const { completed, failed } = e.payload;
          if (failed > 0 && completed === 0) {
            failTask(currentTaskId);
          } else {
            complete(currentTaskId);
          }
          currentTaskId = null;
        }
        isImporting.value = false;
        activeJobId.value = null;
      }),
    );
  }

  /** 清除所有事件监听。 */
  function cleanupEventListeners(): void {
    for (const unlisten of unlistenFns) {
      unlisten();
    }
    unlistenFns.length = 0;
  }

  /** 重置导入状态（开始新任务前调用）。 */
  function resetImportState(): void {
    fileNames.clear();
    totalCount = 0;
    completedCount = 0;
    failedCount = 0;
    error.value = null;
  }

  /** 更新列表中的某条文献（已完成时调用）。 */
  function updateReferenceInList(entry: ReferenceEntry): void {
    const idx = references.value.findIndex((r) => r.id === entry.id);
    if (idx >= 0) {
      references.value[idx] = entry;
    } else {
      references.value.push(entry);
    }
  }

  /* ── 命令封装 ───────────────────────────────────────────────────────── */

  /**
   * 加载项目所有文献列表。
   *
   * @param projectPath 项目根路径
   */
  async function loadReferences(projectPath: string): Promise<ReferenceEntry[]> {
    if (!isTauriEnvironment()) {
      references.value = [];
      return [];
    }
    try {
      const list = await invoke<ReferenceEntry[]>('list_references', {
        projectPath,
      });
      references.value = list;
      return list;
    } catch (err) {
      const msg = typeof err === 'string' ? err : String(err);
      error.value = msg;
      console.error('[useReferences] 加载文献列表失败:', err);
      return [];
    }
  }

  /**
   * 导入单个文献（await 语义，等待完成）。
   *
   * @param projectPath 项目根路径
   * @param filePath 文件路径
   * @param force 是否强制导入（跳过去重）
   */
  async function importSingle(
    projectPath: string,
    filePath: string,
    force = false,
  ): Promise<ReferenceEntry | null> {
    if (!isTauriEnvironment()) {
      throw new Error('文献导入功能仅在 Tauri 环境下可用');
    }

    resetImportState();
    totalCount = 1;
    isImporting.value = true;
    error.value = null;

    // 确保事件监听已注册
    if (unlistenFns.length === 0) {
      await setupEventListeners();
    }

    // 注册任务队列
    currentTaskId = `import.single.${Date.now()}`;
    register(currentTaskId, {
      title: t('main.sidebar.references.import'),
      status: 'running',
      category: 'import',
      progress: { current: 0, total: 1 },
    });

    try {
      const entry = await invoke<ReferenceEntry>('import_reference', {
        filePath,
        projectPath,
        force,
      });
      if (currentTaskId) {
        complete(currentTaskId);
        currentTaskId = null;
      }
      return entry;
    } catch (err) {
      const msg = typeof err === 'string' ? err : String(err);
      error.value = msg;
      if (currentTaskId) {
        failTask(currentTaskId, msg);
        currentTaskId = null;
      }
      throw err;
    } finally {
      isImporting.value = false;
    }
  }

  /**
   * 批量导入文献（fire-and-forget 语义）。
   *
   * 立即返回任务句柄，进度和结果通过事件推送。
   * 进度同时注册到任务队列通知系统，在状态栏实时展示。
   *
   * @param projectPath 项目根路径
   * @param filePaths 文件路径列表
   * @returns 任务句柄（含 job_id 和预分配的 reference_ids）
   */
  async function importFiles(
    projectPath: string,
    filePaths: string[],
  ): Promise<ImportJobHandle> {
    if (!isTauriEnvironment()) {
      throw new Error('文献导入功能仅在 Tauri 环境下可用');
    }

    resetImportState();
    totalCount = filePaths.length;
    isImporting.value = true;

    // 确保事件监听已注册
    if (unlistenFns.length === 0) {
      await setupEventListeners();
    }

    try {
      const handle = await invoke<ImportJobHandle>('import_references', {
        filePaths,
        projectPath,
      });
      activeJobId.value = handle.job_id;

      // 注册任务到队列
      currentTaskId = `import.job.${handle.job_id}`;
      register(currentTaskId, {
        title: t('main.sidebar.references.import'),
        status: 'running',
        category: 'import',
        progress: { current: 0, total: totalCount },
      });

      return handle;
    } catch (err) {
      isImporting.value = false;
      const msg = typeof err === 'string' ? err : String(err);
      error.value = msg;
      throw err;
    }
  }

  /**
   * 取消指定导入任务。
   *
   * @param projectPath 项目根路径
   * @param jobId 任务 ID
   */
  async function cancelImport(projectPath: string, jobId: string): Promise<void> {
    if (!isTauriEnvironment()) return;
    try {
      await invoke('cancel_import', { projectPath, jobId });
      if (currentTaskId) {
        cancelTask(currentTaskId);
        currentTaskId = null;
      }
    } catch (err) {
      console.error('[useReferences] 取消导入失败:', err);
    }
  }

  /**
   * 取消所有进行中的导入任务。
   *
   * @param projectPath 项目根路径
   * @returns 已取消的任务数
   */
  async function cancelAllImports(projectPath: string): Promise<number> {
    if (!isTauriEnvironment()) return 0;
    try {
      const count = await invoke<number>('cancel_all_imports', { projectPath });
      if (currentTaskId) {
        cancelTask(currentTaskId);
        currentTaskId = null;
      }
      isImporting.value = false;
      activeJobId.value = null;
      return count;
    } catch (err) {
      console.error('[useReferences] 取消所有导入失败:', err);
      return 0;
    }
  }

  /**
   * 删除文献（同时删除 raw/md/resource 文件）。
   *
   * @param projectPath 项目根路径
   * @param referenceId 文献 ID
   */
  async function deleteReference(
    projectPath: string,
    referenceId: string,
  ): Promise<void> {
    if (!isTauriEnvironment()) return;
    try {
      await invoke('delete_reference', { projectPath, referenceId });
      references.value = references.value.filter((r) => r.id !== referenceId);
    } catch (err) {
      const msg = typeof err === 'string' ? err : String(err);
      error.value = msg;
      throw err;
    }
  }

  /**
   * 重试失败的导入。
   *
   * @param projectPath 项目根路径
   * @param referenceId 文献 ID
   * @returns 更新后的文献条目
   */
  async function retryImport(
    projectPath: string,
    referenceId: string,
  ): Promise<ReferenceEntry | null> {
    if (!isTauriEnvironment()) return null;

    isImporting.value = true;
    error.value = null;

    try {
      const entry = await invoke<ReferenceEntry>('retry_import', {
        projectPath,
        referenceId,
      });
      updateReferenceInList(entry);
      return entry;
    } catch (err) {
      const msg = typeof err === 'string' ? err : String(err);
      error.value = msg;
      throw err;
    } finally {
      isImporting.value = false;
    }
  }

  /**
   * 更新文献标题（手动修正）。
   *
   * @param projectPath 项目根路径
   * @param referenceId 文献 ID
   * @param title 新标题
   * @returns 更新后的文献条目
   */
  async function updateTitle(
    projectPath: string,
    referenceId: string,
    title: string,
  ): Promise<ReferenceEntry | null> {
    if (!isTauriEnvironment()) return null;
    try {
      const entry = await invoke<ReferenceEntry>('update_reference_title', {
        projectPath,
        referenceId,
        title,
      });
      updateReferenceInList(entry);
      return entry;
    } catch (err) {
      const msg = typeof err === 'string' ? err : String(err);
      error.value = msg;
      throw err;
    }
  }

  /**
   * 一致性校验——扫描孤儿文件与缺失条目。
   *
   * @param projectPath 项目根路径
   * @returns 校验报告
   */
  async function checkConsistency(
    projectPath: string,
  ): Promise<ConsistencyReport | null> {
    if (!isTauriEnvironment()) return null;
    try {
      return await invoke<ConsistencyReport>('check_references_consistency', {
        projectPath,
      });
    } catch (err) {
      const msg = typeof err === 'string' ? err : String(err);
      error.value = msg;
      console.error('[useReferences] 一致性校验失败:', err);
      return null;
    }
  }

  /* ── 生命周期清理 ───────────────────────────────────────────────────── */

  onUnmounted(() => {
    cleanupEventListeners();
  });

  return {
    // 响应式状态
    references,
    activeJobId,
    isImporting,
    error,
    // 命令
    loadReferences,
    importSingle,
    importFiles,
    cancelImport,
    cancelAllImports,
    deleteReference,
    retryImport,
    updateTitle,
    checkConsistency,
    // 事件管理
    setupEventListeners,
    cleanupEventListeners,
  };
}
