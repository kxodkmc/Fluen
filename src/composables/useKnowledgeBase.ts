/**
 * 知识库构建 composable。
 *
 * 封装 Tauri `knowledge_*` 命令调用与 `kb-build:*` 事件监听，提供：
 *   - 启动构建任务（`knowledge_build_start`）
 *   - 监听 `kb-build:started|progress|completed|failed|cancelled` 事件
 *   - 维护 `ref_id → KnowledgeBuildStatus` 的全局响应式映射（跨组件共享）
 *   - 加载项目知识库条目，回填已存在文献的 `added` 状态
 *   - 进度同时注册到任务队列通知系统（useTaskQueue）
 *
 * 设计要点：
 *   - **模块级单例状态**：`buildStatusMap` 跨组件共享，
 *     ReferencesPanel 与 ReferenceReader 等均可读取同一份构建状态。
 *   - **事件监听单例**：首次调用 `setupEventListeners` 注册全局监听，
 *     重复调用幂等（通过 `listenersReady` 标记保护）。
 *
 * @example
 * ```ts
 * const { buildStatusOf, startBuild, loadExistingStatus } = useKnowledgeBase();
 *
 * // 加载项目知识库，回填每个文献的构建状态
 * await loadExistingStatus(projectPath);
 *
 * // 触发构建
 * await startBuild(projectPath, refId);
 *
 * // 读取状态（响应式）
 * const status = buildStatusOf(refId);
 * ```
 */

import { reactive, ref, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { useI18n } from '../i18n';
import { useTaskQueue } from '../views/main/components/statusbar/taskqueue/useTaskQueue';
import type {
  KbBuildCancelledPayload,
  KbBuildCompletedPayload,
  KbBuildFailedPayload,
  KbBuildProgressPayload,
  KbBuildStartedPayload,
  KnowledgeBuildOptions,
  KnowledgeBuildStatus,
  TaskRecord,
  WikiEntry,
} from '../types/knowledgeBase';

// ---------------------------------------------------------------------------
// 事件名常量（与后端 events.rs 保持一致）
// ---------------------------------------------------------------------------

const EVENT_KB_BUILD_STARTED = 'kb-build:started';
const EVENT_KB_BUILD_PROGRESS = 'kb-build:progress';
const EVENT_KB_BUILD_COMPLETED = 'kb-build:completed';
const EVENT_KB_BUILD_FAILED = 'kb-build:failed';
const EVENT_KB_BUILD_CANCELLED = 'kb-build:cancelled';

/**
 * 检测当前是否运行在 Tauri 环境中。
 */
function isTauriEnvironment(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

// ---------------------------------------------------------------------------
// 模块级单例状态（跨组件共享）
// ---------------------------------------------------------------------------

/** ref_id → 构建状态（响应式）。 */
const buildStatusMap = reactive<Record<string, KnowledgeBuildStatus>>({});

/** ref_id → 最近一次错误信息。 */
const buildErrorMap = reactive<Record<string, string>>({});

/** ref_id → 任务队列 ID（用于 update/complete/fail）。 */
const taskIdMap = new Map<string, string>();

/** 已注册的事件取消监听函数列表。 */
const unlistenFns: UnlistenFn[] = [];

/** 事件监听是否已注册（幂等保护）。 */
let listenersReady = false;

/** 是否正在构建（任意文献）。 */
const isBuilding = ref(false);

// ---------------------------------------------------------------------------
// 内部辅助
// ---------------------------------------------------------------------------

/**
 * 从 `WikiEntry.source`（如 `raw/ref-xxx.pdf`）解析出 ref_id。
 *
 * @returns ref_id 或 null（无法解析时）
 */
function extractRefIdFromSource(source: string | undefined | null): string | null {
  if (!source) return null;
  // 形如 raw/ref-xxxxxxxxxxxxxxxx.pdf
  const match = source.match(/raw\/(ref-[a-f0-9]+)\./i);
  return match ? match[1] : null;
}

// ---------------------------------------------------------------------------
// Composable
// ---------------------------------------------------------------------------

export function useKnowledgeBase() {
  const { t } = useI18n();
  const { register, update, complete, fail: failTask } = useTaskQueue();

  /* ── 事件监听 ─────────────────────────────────────────────────────────── */

  /** 注册所有 `kb-build:*` 事件监听（幂等，重复调用安全）。 */
  async function setupEventListeners(): Promise<void> {
    if (!isTauriEnvironment() || listenersReady) return;
    listenersReady = true;

    unlistenFns.push(
      await listen<KbBuildStartedPayload>(EVENT_KB_BUILD_STARTED, (e) => {
        const { ref_id, task_id } = e.payload;
        buildStatusMap[ref_id] = 'building';
        taskIdMap.set(ref_id, task_id);
        isBuilding.value = true;
      }),
    );

    unlistenFns.push(
      await listen<KbBuildProgressPayload>(EVENT_KB_BUILD_PROGRESS, (e) => {
        const { ref_id, stage, created_count, total_planned, detail } = e.payload;
        const taskId = taskIdMap.get(ref_id);
        if (!taskId) return;
        // 阶段标签走 i18n；缺失 key 时回退到 stage 原文（vue-i18n 默认行为）
        const stageLabel = t(
          `main.sidebar.references.kbStage.${stage}`,
          stage,
        );
        const progressText =
          total_planned != null
            ? `${stageLabel} ${created_count}/${total_planned}`
            : `${stageLabel} ${created_count}`;
        update(taskId, {
          detail: detail ?? progressText,
          progress:
            total_planned != null
              ? { current: created_count, total: total_planned }
              : undefined,
        });
      }),
    );

    unlistenFns.push(
      await listen<KbBuildCompletedPayload>(EVENT_KB_BUILD_COMPLETED, (e) => {
        const { ref_id, task_id } = e.payload;
        buildStatusMap[ref_id] = 'added';
        buildErrorMap[ref_id] = '';
        taskIdMap.delete(ref_id);
        complete(task_id);
        if (taskIdMap.size === 0) isBuilding.value = false;
      }),
    );

    unlistenFns.push(
      await listen<KbBuildFailedPayload>(EVENT_KB_BUILD_FAILED, (e) => {
        const { ref_id, task_id, error } = e.payload;
        buildStatusMap[ref_id] = 'failed';
        buildErrorMap[ref_id] = error;
        taskIdMap.delete(ref_id);
        failTask(task_id, error);
        if (taskIdMap.size === 0) isBuilding.value = false;
      }),
    );

    unlistenFns.push(
      await listen<KbBuildCancelledPayload>(EVENT_KB_BUILD_CANCELLED, (e) => {
        const { ref_id, task_id } = e.payload;
        // 取消后回退到 idle（保留 added 状态需重新查询，简化处理为 idle）
        if (buildStatusMap[ref_id] === 'building') {
          buildStatusMap[ref_id] = 'idle';
        }
        taskIdMap.delete(ref_id);
        complete(task_id);
        if (taskIdMap.size === 0) isBuilding.value = false;
      }),
    );
  }

  /** 清除所有事件监听（通常仅在应用卸载时调用）。 */
  function cleanupEventListeners(): void {
    for (const unlisten of unlistenFns) {
      unlisten();
    }
    unlistenFns.length = 0;
    listenersReady = false;
  }

  /* ── 状态查询 ─────────────────────────────────────────────────────────── */

  /** 读取指定文献的构建状态（响应式，未记录时返回 'idle'）。 */
  function buildStatusOf(refId: string): KnowledgeBuildStatus {
    return buildStatusMap[refId] ?? 'idle';
  }

  /** 读取指定文献最近一次构建错误（无错误时为空字符串）。 */
  function buildErrorOf(refId: string): string {
    return buildErrorMap[refId] ?? '';
  }

  /* ── 命令封装 ───────────────────────────────────────────────────────── */

  /**
   * 加载项目知识库条目，回填每个文献的 `added` 状态。
   *
   * 通过 `knowledge_list_entries` 拉取所有 summary 条目，从 `source` 字段
   * 解析出 ref_id，标记为 `added`。
   *
   * @param projectPath 项目根路径
   */
  /**
   * 加载知识库现状，回填每个文献的构建状态。
   *
   * 判定依据"知识库实际条目 + 最近一次构建任务状态"，避免把
   * "失败后遗留的部分条目"误读为"完整入库"：
   * - 有条目且最近构建成功 / 无任务记录 → `added`（已入库）
   * - 有条目但最近构建失败 / 被取消 → `partial`（部分入库）
   * - 有条目且构建任务排队 / 进行中 → `building`（入库中）
   *
   * @param projectPath 项目根路径
   */
  async function loadExistingStatus(projectPath: string): Promise<void> {
    if (!isTauriEnvironment()) return;
    try {
      const [entries, tasks] = await Promise.all([
        invoke<WikiEntry[]>('knowledge_list_entries', { projectPath }),
        // 任务队列查询失败不阻塞知识库扫描（无任务记录时按 added 处理）
        invoke<TaskRecord[]>('task_queue_list', { projectPath, statusFilter: null }).catch(
          () => [],
        ),
      ]);

      // ref_id → 最近一次知识库构建任务（created_at 最新）
      const latestKbTask = new Map<string, TaskRecord>();
      for (const task of tasks) {
        if (task.kind.kind !== 'knowledge_build') continue;
        const refId = task.kind.ref_id;
        const prev = latestKbTask.get(refId);
        if (!prev || task.created_at > prev.created_at) {
          latestKbTask.set(refId, task);
        }
      }

      for (const entry of entries) {
        if (entry.wiki_type !== 'summary') continue;
        const refId = extractRefIdFromSource(entry.source);
        if (!refId || buildStatusMap[refId] === 'building') continue;

        const task = latestKbTask.get(refId);
        if (task?.status === 'failed' || task?.status === 'cancelled') {
          // 有条目但最近构建未成功：部分入库（避免误读为完整入库）
          buildStatusMap[refId] = 'partial';
        } else if (task?.status === 'pending' || task?.status === 'running') {
          buildStatusMap[refId] = 'building';
        } else {
          buildStatusMap[refId] = 'added';
        }
      }
    } catch (err) {
      // 知识库未初始化或读取失败时静默处理（前端不强制要求初始化）
      console.warn('[useKnowledgeBase] 加载知识库状态失败:', err);
    }
  }

  /**
   * 启动知识库构建任务。
   *
   * 调用 `knowledge_build_start` 入队任务，进度和结果通过 `kb-build:*` 事件推送。
   * 同时注册到任务队列通知系统，在状态栏实时展示进度。
   *
   * @param projectPath 项目根路径
   * @param refId 文献 ID
   * @param options 构建选项（undefined 使用后端默认值）
   * @returns 入队的任务记录
   */
  async function startBuild(
    projectPath: string,
    refId: string,
    options?: KnowledgeBuildOptions,
  ): Promise<TaskRecord> {
    if (!isTauriEnvironment()) {
      throw new Error('知识库构建功能仅在 Tauri 环境下可用');
    }

    // 确保事件监听已注册
    await setupEventListeners();

    // 立即标记为 building（避免事件延迟期间 UI 误判为 idle）
    buildStatusMap[refId] = 'building';
    buildErrorMap[refId] = '';
    isBuilding.value = true;

    try {
      const record = await invoke<TaskRecord>('knowledge_build_start', {
        projectPath,
        refId,
        options: options ?? null,
      });

      // 提前注册任务到队列（若 started 事件尚未到达）
      const taskId = record.id;
      taskIdMap.set(refId, taskId);
      register(taskId, {
        title: t('main.sidebar.references.addToKnowledgeBase'),
        status: 'running',
        category: 'knowledge',
      });

      return record;
    } catch (err) {
      buildStatusMap[refId] = 'failed';
      buildErrorMap[refId] = typeof err === 'string' ? err : String(err);
      isBuilding.value = false;
      throw err;
    }
  }

  /* ── 生命周期清理 ───────────────────────────────────────────────────── */

  /**
   * 组件作用域清理。
   *
   * 注意：事件监听为模块级单例，不随组件卸载移除。
   * 此函数仅做占位，便于未来需要按组件作用域管理时扩展。
   */
  onUnmounted(() => {
    // 模块级监听不在组件卸载时清理
  });

  return {
    // 响应式状态
    isBuilding,
    buildStatusMap,
    // 查询
    buildStatusOf,
    buildErrorOf,
    // 命令
    startBuild,
    loadExistingStatus,
    // 事件管理
    setupEventListeners,
    cleanupEventListeners,
  };
}
