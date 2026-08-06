/**
 * 文献导入与管理 composable。
 *
 * 封装 Tauri invoke 调用与 `reference:*` 事件监听，提供：
 *   - 文献列表加载 / 删除 / 重试 / 修改标题
 *   - 批量导入（队列入队）：每个文件入队一个 `reference_import` 任务，
 *     由后端任务队列（task_queue）按 FIFO 串行执行，先到先导；
 *     导入按钮始终可用，多次导入全部排队
 *   - 任务状态展示：每个任务在任务队列 UI 注册一条独立记录
 *   - 取消导入任务
 *
 * 队列设计（与后端 `task_queue` 对应）：
 *   - `references_enqueue_imports` 一次入队全部文件，立即返回任务记录列表
 *   - 任务状态（pending/running/completed/failed/cancelled）持久化在
 *     `{project}/data/task-queue.json`，App 崩溃后重启自动续跑（幂等）
 *   - 执行进度与结果通过 `reference:import_started|progress|completed|failed`
 *     事件推送，事件的 `job_id` 即任务队列的 `task_id`
 *   - 单个文件失败不阻塞队列，失败原因记录在任务 `error` 字段与文献
 *     条目 `error` 字段
 *   - 竞态补偿：任务入队后可能立即终态（事件先于 invoke 返回），入队
 *     返回后查询一次任务队列状态并刷新文献列表，补齐丢失的事件
 *
 * @example
 * ```ts
 * const { references, isImporting, loadReferences, importFiles, cancelAllImports } = useReferences();
 *
 * await loadReferences(projectPath);
 * const records = await importFiles(projectPath, ['/path/a.pdf', '/path/b.pdf']);
 * ```
 */

import { ref, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { useI18n } from '../i18n';
import { useLogger } from './useLogger';
import { useTaskQueue } from '../views/main/components/statusbar/taskqueue/useTaskQueue';
import type { TaskRecord } from '../types/taskQueue';
import type {
  ConsistencyReport,
  ImportProgressPayload,
  ImportStartedPayload,
  ImportCompletedPayload,
  ImportFailedPayload,
  ReferenceEntry,
} from '../types/references';

// ---------------------------------------------------------------------------
// 事件名常量（与后端 references/events.rs 保持一致）
// ---------------------------------------------------------------------------

const EVENT_IMPORT_STARTED = 'reference:import_started';
const EVENT_IMPORT_PROGRESS = 'reference:import_progress';
const EVENT_IMPORT_COMPLETED = 'reference:import_completed';
const EVENT_IMPORT_FAILED = 'reference:import_failed';

/** 后端取消任务时的错误文案（与 runner.rs 保持一致）。 */
const CANCEL_MESSAGE = '已取消';

/**
 * 检测当前是否运行在 Tauri 环境中。
 */
function isTauriEnvironment(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** 从文献导入任务提取源文件名（UI 展示用）。 */
function filenameFromTask(task: TaskRecord): string {
  if (task.kind.kind === 'reference_import') {
    const p = task.kind.file_path;
    const idx = Math.max(p.lastIndexOf('/'), p.lastIndexOf('\\'));
    return idx >= 0 ? p.slice(idx + 1) : p;
  }
  return '导入任务';
}

export function useReferences() {
  const { t } = useI18n();
  const log = useLogger('references');
  const { register, update, complete, fail: failTask, cancel: cancelTask } = useTaskQueue();

  /* ── 响应式状态 ─────────────────────────────────────────────────────── */

  /** 文献列表。 */
  const references = ref<ReferenceEntry[]>([]);

  /** 是否正在导入（当前批次尚有未终态任务）。 */
  const isImporting = ref(false);

  /** 最后一次错误信息。 */
  const error = ref<string | null>(null);

  /* ── 内部跟踪（不暴露）──────────────────────────────────────────────── */

  /** reference_id → filename 映射（来自 import_started 事件，供进度展示）。 */
  const fileNames = new Map<string, string>();

  /** 当前批次所有任务 ID（用于取消）。 */
  let batchTaskIds: string[] = [];

  /** 当前批次未终态的任务 ID（isImporting 依据）。 */
  let pendingTaskIds: string[] = [];

  /* ── 事件监听管理 ───────────────────────────────────────────────────── */

  /** 已注册的事件取消监听函数列表。 */
  const unlistenFns: UnlistenFn[] = [];

  /** 任务终态：更新未终态集合，全部结束时复位导入状态。 */
  function markTaskFinished(taskId: string | null): void {
    if (!taskId) return;
    pendingTaskIds = pendingTaskIds.filter((id) => id !== taskId);
    if (pendingTaskIds.length === 0) {
      isImporting.value = false;
    }
  }

  /** 注册所有 `reference:*` 事件监听（按 `job_id` 路由到对应 UI 任务）。 */
  async function setupEventListeners(): Promise<void> {
    if (!isTauriEnvironment()) return;

    unlistenFns.push(
      await listen<ImportStartedPayload>(EVENT_IMPORT_STARTED, (e) => {
        const { reference_id, filename, job_id } = e.payload;
        fileNames.set(reference_id, filename);
        log.debug('导入开始', { job_id, reference_id, filename });
        if (job_id) {
          update(job_id, { detail: filename });
        }
      }),
    );

    unlistenFns.push(
      await listen<ImportProgressPayload>(EVENT_IMPORT_PROGRESS, (e) => {
        const { reference_id, stage, ocr_progress, job_id } = e.payload;
        if (!job_id) return;
        const filename = fileNames.get(reference_id) ?? '';
        let detail = filename;
        if (stage === 'ocr' && ocr_progress?.extracted_pages != null && ocr_progress?.total_pages != null) {
          detail = `${filename} · ${t('main.sidebar.references.stageOcr')} ${ocr_progress.extracted_pages}/${ocr_progress.total_pages}`;
        } else if (stage === 'saving') {
          detail = `${filename} · ${t('main.sidebar.references.stageSaving')}`;
        }
        log.trace('导入进度', { job_id, reference_id, stage, ocr_progress });
        update(job_id, { detail });
      }),
    );

    unlistenFns.push(
      await listen<ImportCompletedPayload>(EVENT_IMPORT_COMPLETED, (e) => {
        const { reference_id, entry, job_id } = e.payload;
        fileNames.delete(reference_id);
        // 列表同步无条件执行：含启动恢复等历史任务，完成即刷新条目
        updateReferenceInList(entry);
        log.info('导入完成', { job_id, reference_id, title: entry.title });
        if (job_id) {
          complete(job_id);
          markTaskFinished(job_id);
        }
      }),
    );

    unlistenFns.push(
      await listen<ImportFailedPayload>(EVENT_IMPORT_FAILED, (e) => {
        const { reference_id, error: err, job_id } = e.payload;
        const filename = fileNames.get(reference_id);
        fileNames.delete(reference_id);
        log.error('导入失败', { job_id, reference_id, filename, error: err });
        if (job_id) {
          // 取消以「已取消」文案表达，统一展示为 cancelled 状态
          if (err === CANCEL_MESSAGE) {
            cancelTask(job_id);
          } else {
            failTask(job_id, err);
          }
          markTaskFinished(job_id);
        }
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
      log.debug('加载文献列表成功', { count: list.length, projectPath });
      return list;
    } catch (err) {
      const msg = typeof err === 'string' ? err : String(err);
      error.value = msg;
      log.error('加载文献列表失败', { projectPath, error: msg });
      return [];
    }
  }

  /**
   * 批量导入文献——全部入队，由后端队列按顺序串行执行。
   *
   * 每个文件入队一个 `reference_import` 任务（先到先导），立即返回任务记录。
   * 入队返回后查询一次任务状态并刷新文献列表，补齐"入队后立即终态"
   * 导致的竞态丢失；此后进度由 `reference:*` 事件驱动。
   *
   * @param projectPath 项目根路径
   * @param filePaths 文件路径列表
   * @returns 入队的任务记录列表
   */
  async function importFiles(
    projectPath: string,
    filePaths: string[],
  ): Promise<TaskRecord[]> {
    if (!isTauriEnvironment()) {
      throw new Error('文献导入功能仅在 Tauri 环境下可用');
    }
    if (filePaths.length === 0) return [];

    // 确保事件监听已注册
    if (unlistenFns.length === 0) {
      await setupEventListeners();
    }

    log.info('批量导入请求（入队）', { count: filePaths.length, filePaths, projectPath });

    const records = await invoke<TaskRecord[]>('references_enqueue_imports', {
      projectPath,
      filePaths,
    });

    batchTaskIds = records.map((r) => r.id);
    pendingTaskIds = [...batchTaskIds];
    isImporting.value = true;
    fileNames.clear();
    log.info('批量导入任务已入队', { task_ids: batchTaskIds, count: batchTaskIds.length });

    // 竞态补偿：入队后任务可能已立即终态（事件先于 invoke 返回），
    // 查询任务队列最新状态按终态注册 UI 任务，并刷新文献列表补齐条目。
    try {
      const all = await invoke<TaskRecord[]>('task_queue_list', {
        projectPath,
        statusFilter: null,
      });
      const fresh = new Map(all.map((r) => [r.id, r]));
      for (const r of records) {
        const latest = fresh.get(r.id);
        const status = latest?.status;
        const title = filenameFromTask(r);
        if (status === 'completed') {
          register(r.id, { title, status: 'completed', category: 'import' });
          markTaskFinished(r.id);
        } else if (status === 'failed') {
          register(r.id, {
            title,
            status: 'failed',
            error: latest?.error ?? undefined,
            category: 'import',
          });
          markTaskFinished(r.id);
        } else {
          // pending / running / 状态未知：等待事件驱动
          register(r.id, { title, status: 'running', category: 'import' });
        }
      }
      // 刷新列表：竞态窗口内完成的条目（事件未处理）在此补齐
      await loadReferences(projectPath);
    } catch (err) {
      log.warn('同步任务状态失败，等待事件驱动', { error: String(err) });
      for (const r of records) {
        register(r.id, {
          title: filenameFromTask(r),
          status: 'running',
          category: 'import',
        });
      }
    }

    return records;
  }

  /**
   * 取消当前批次所有导入任务。
   *
   * 仅取消本批次入队且尚未终态的任务；已执行完的忽略。
   * 取消不产生完整事件流（Pending 任务被静默置为 Cancelled），
   * 因此在取消命令返回后主动将 UI 任务标记为 cancelled。
   *
   * @param projectPath 项目根路径
   */
  async function cancelAllImports(projectPath: string): Promise<void> {
    if (!isTauriEnvironment() || batchTaskIds.length === 0) return;
    const taskIds = [...batchTaskIds];
    log.info('取消导入批次', { task_ids: taskIds });
    await Promise.all(
      taskIds.map(async (taskId) => {
        try {
          await invoke('task_queue_cancel', { taskId, projectPath });
        } catch (err) {
          const msg = typeof err === 'string' ? err : String(err);
          log.error('取消导入任务失败', { taskId, error: msg });
        }
        cancelTask(taskId);
        markTaskFinished(taskId);
      }),
    );
    batchTaskIds = [];
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
      log.debug('删除文献成功', { referenceId });
    } catch (err) {
      const msg = typeof err === 'string' ? err : String(err);
      error.value = msg;
      log.error('删除文献失败', { referenceId, error: msg });
      throw err;
    }
  }

  /**
   * 重试失败的导入——重新入队一个 `reference_import` 任务（跳过文件去重）。
   *
   * @param projectPath 项目根路径
   * @param referenceId 文献 ID
   * @returns 入队的任务记录
   */
  async function retryImport(
    projectPath: string,
    referenceId: string,
  ): Promise<TaskRecord | null> {
    if (!isTauriEnvironment()) return null;
    if (unlistenFns.length === 0) {
      await setupEventListeners();
    }
    log.info('重试导入请求（入队）', { referenceId, projectPath });

    const record = await invoke<TaskRecord>('retry_import', {
      projectPath,
      referenceId,
    });

    batchTaskIds = [record.id];
    pendingTaskIds = [record.id];
    isImporting.value = true;
    fileNames.clear();

    // 竞态补偿（同 importFiles）
    try {
      const all = await invoke<TaskRecord[]>('task_queue_list', {
        projectPath,
        statusFilter: null,
      });
      const latest = all.find((r) => r.id === record.id);
      const status = latest?.status;
      const title = filenameFromTask(record);
      if (status === 'completed') {
        register(record.id, { title, status: 'completed', category: 'import' });
        markTaskFinished(record.id);
      } else if (status === 'failed') {
        register(record.id, {
          title,
          status: 'failed',
          error: latest?.error ?? undefined,
          category: 'import',
        });
        markTaskFinished(record.id);
      } else {
        register(record.id, { title, status: 'running', category: 'import' });
      }
      await loadReferences(projectPath);
    } catch (err) {
      log.warn('同步任务状态失败，等待事件驱动', { error: String(err) });
      register(record.id, {
        title: filenameFromTask(record),
        status: 'running',
        category: 'import',
      });
    }

    return record;
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
      log.debug('更新文献标题成功', { referenceId, title: entry.title });
      return entry;
    } catch (err) {
      const msg = typeof err === 'string' ? err : String(err);
      error.value = msg;
      log.error('更新文献标题失败', { referenceId, error: msg });
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
      const report = await invoke<ConsistencyReport>('check_references_consistency', {
        projectPath,
      });
      log.debug('一致性校验完成', { projectPath, report });
      return report;
    } catch (err) {
      const msg = typeof err === 'string' ? err : String(err);
      error.value = msg;
      log.error('一致性校验失败', { projectPath, error: msg });
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
    isImporting,
    error,
    // 命令
    loadReferences,
    importFiles,
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
