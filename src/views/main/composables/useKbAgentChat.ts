/**
 * 知识库构建对话流 composable（Kb Agent 面板）。
 *
 * 订阅 `kbchat:*`（思考/文本增量、工具调用）与 `kb-build:*`（生命周期）
 * 事件，按 `taskId` 维护每个构建任务的对话会话，以 Motis 同款消息结构
 * （ChatMessage：text / thinking / tool_call / status）供面板渲染。
 *
 * 设计要点：
 *   - **模块级单例**：ReferencesPanel（触发构建）与 KbAgentPanel（展示）
 *     共享同一份会话状态，无需 provide/inject。
 *   - **管线语义不变**：仅可视化，不参与任务执行；阶段与终态复用
 *     `kb-build:progress|completed|failed|cancelled`（载荷已含 task_id）。
 *   - **事件监听懒加载单例**：首次调用注册全局监听，重复调用幂等。
 *
 * 事件流：
 * ```text
 * kb-build:started   →  创建会话 + 用户消息（《标题》加入知识库）
 * kbchat:thought     →  思考增量（thinking 消息追加）
 * kbchat:text        →  文本增量（assistant 文本消息追加）
 * kbchat:tool-call   →  工具调用（tool_call 消息）
 * kbchat:tool-result →  工具结果（按 tool_call_id 回填）
 * kb-build:progress  →  阶段迁移（status 消息，仅在阶段变化时）
 * kb-build:completed|failed|cancelled → 终态 status 消息
 * ```
 */

import { ref, readonly } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { useI18n } from '../../../i18n';
import type { ChatMessage } from '../types';
import type {
  KbBuildCancelledPayload,
  KbBuildCompletedPayload,
  KbBuildFailedPayload,
  KbBuildProgressPayload,
  KbBuildStartedPayload,
} from '../../../types/knowledgeBase';

/* ── 事件名常量（与后端 events.rs 保持一致） ─────────────────────────── */
const EVENT_KB_BUILD_STARTED = 'kb-build:started';
const EVENT_KB_BUILD_PROGRESS = 'kb-build:progress';
const EVENT_KB_BUILD_COMPLETED = 'kb-build:completed';
const EVENT_KB_BUILD_FAILED = 'kb-build:failed';
const EVENT_KB_BUILD_CANCELLED = 'kb-build:cancelled';
const EVENT_KBCHAT_THOUGHT = 'kbchat:thought';
const EVENT_KBCHAT_TEXT = 'kbchat:text';
const EVENT_KBCHAT_TOOL_CALL = 'kbchat:tool-call';
const EVENT_KBCHAT_TOOL_RESULT = 'kbchat:tool-result';

/* ── 事件 payload 类型（与后端 events.rs 对齐） ──────────────────────── */
/** 增量事件（思考 / 文本共用）。 */
interface KbChatDeltaPayload {
  task_id: string;
  ref_id: string;
  delta: string;
}
/** 工具调用事件。 */
interface KbChatToolCallPayload {
  task_id: string;
  ref_id: string;
  id: string;
  name: string;
  input: unknown;
}
/** 工具结果事件。 */
interface KbChatToolResultPayload {
  task_id: string;
  ref_id: string;
  tool_call_id: string;
  name: string;
  ok: boolean;
  duration_ms: number;
  result: unknown;
}

/** 单个构建任务的对话会话。 */
export interface KbAgentSession {
  /** 任务队列 ID（事件关联键）。 */
  taskId: string;
  /** 文献 ID。 */
  refId: string;
  /** 文献标题（started 事件未携带时回退 refId）。 */
  title: string;
  /** 会话状态（跟随 kb-build 终态事件）。 */
  status: 'running' | 'completed' | 'failed' | 'cancelled';
  /** 对话消息（Motis 同款结构，供 MotisChatMessageList 渲染）。 */
  messages: ChatMessage[];
  /** 会话创建时间戳（排序用）。 */
  startedAt: number;
}

/** 每会话消息上限（防超长构建膨胀，超出时丢弃最早的思考/文本消息）。 */
const MAX_MESSAGES_PER_SESSION = 800;

/**
 * 检测当前是否运行在 Tauri 环境中。
 */
function isTauriEnvironment(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** 生成唯一消息 ID（优先使用 crypto.randomUUID）。 */
function generateId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `kbmsg-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

// ---------------------------------------------------------------------------
// 模块级单例状态（跨组件共享）
// ---------------------------------------------------------------------------

/** taskId → 会话（插入序，最新在后；展示时倒序）。 */
const sessions = ref<KbAgentSession[]>([]);

/** 当前聚焦的会话 taskId（面板展示对象）。 */
const activeTaskId = ref<string | null>(null);

/** 已注册的事件取消监听函数列表。 */
const unlistenFns: UnlistenFn[] = [];

/** 事件监听是否已注册（幂等保护）。 */
let listenersReady = false;

/** 每会话的流式消息 ID（taskId → 当前 thinking / text 消息 ID）。 */
const currentThinkingIds = new Map<string, string | null>();
const currentTextIds = new Map<string, string | null>();
/** 每会话最近一次进度阶段（仅在阶段变化时插入 status 消息）。 */
const lastStageIds = new Map<string, string>();

/**
 * 构建开始时的自动打开回调（由持有布局状态的组件注册，如 ReferencesPanel）。
 *
 * 自动打开不直接挂在「右键加入知识库」路径上，而是统一由
 * `kb-build:started` 事件驱动——覆盖右键触发、队列遗留任务恢复执行、
 * 失败重试等一切构建启动方式，保证「构建一旦运行即可见」。
 */
let autoOpenHandler: ((taskId: string) => void) | null = null;

/**
 * 注册构建开始时的自动打开回调（幂等覆盖，组件 setup 中调用一次即可）。
 *
 * 回调在 `kb-build:started` 事件到达时同步执行：应切换面板到该任务
 * 并展开右侧面板（如 `focusTask(taskId)` + `showRightPanel('kbagent')`）。
 */
export function registerKbAgentAutoOpen(handler: (taskId: string) => void): void {
  autoOpenHandler = handler;
}

// ---------------------------------------------------------------------------
// 内部辅助
// ---------------------------------------------------------------------------

/** 按 taskId 查找会话。 */
function findSession(taskId: string): KbAgentSession | null {
  return sessions.value.find((s) => s.taskId === taskId) ?? null;
}

/** 追加消息（超上限时丢弃最早的 thinking/text 消息，保留工具与状态时间线）。 */
function pushMessage(session: KbAgentSession, msg: ChatMessage): void {
  session.messages.push(msg);
  if (session.messages.length > MAX_MESSAGES_PER_SESSION) {
    const idx = session.messages.findIndex(
      (m) => m.kind === 'thinking' || (m.kind === 'text' && m.role === 'assistant'),
    );
    if (idx >= 0) session.messages.splice(idx, 1);
    else session.messages.shift();
  }
}

/** 关闭会话的流式消息（标记 isStreaming=false，可选标记中断）。 */
function closeStreaming(session: KbAgentSession, interrupted = false): void {
  for (const id of [currentThinkingIds.get(session.taskId), currentTextIds.get(session.taskId)]) {
    if (!id) continue;
    const msg = session.messages.find((m) => m.id === id);
    if (msg) {
      msg.isStreaming = false;
      if (interrupted) msg.interrupted = true;
    }
  }
  currentThinkingIds.set(session.taskId, null);
  currentTextIds.set(session.taskId, null);
}

// ---------------------------------------------------------------------------
// Composable
// ---------------------------------------------------------------------------

export function useKbAgentChat() {
  const { t } = useI18n();

  /* ── 事件处理 ─────────────────────────────────────────────────────────── */

  /** 构建开始 — 创建会话并插入用户消息（模拟对话发起）。 */
  function onStarted(payload: KbBuildStartedPayload): void {
    const session: KbAgentSession = {
      taskId: payload.task_id,
      refId: payload.ref_id,
      title: payload.title ?? payload.ref_id,
      status: 'running',
      messages: [],
      startedAt: Date.now(),
    };
    sessions.value.push(session);
    currentThinkingIds.set(session.taskId, null);
    currentTextIds.set(session.taskId, null);
    lastStageIds.delete(session.taskId);
    // 新任务自动获得焦点（多任务连发时跟随最新）
    activeTaskId.value = session.taskId;
    // 任何构建启动（右键 / 队列恢复 / 重试）都自动展开面板
    autoOpenHandler?.(session.taskId);
    pushMessage(session, {
      id: generateId(),
      role: 'user',
      kind: 'text',
      content: `${t('main.kbAgent.userRequest')}「${session.title}」`,
      timestamp: Date.now(),
    });
  }

  /** 思考增量 — 追加到当前 thinking 消息。 */
  function onThought(payload: KbChatDeltaPayload): void {
    const session = findSession(payload.task_id);
    if (!session || payload.delta.length === 0) return;
    const currentId = currentThinkingIds.get(session.taskId) ?? null;
    if (currentId === null && payload.delta.trim().length === 0) return;
    if (currentId === null) {
      const msg: ChatMessage = {
        id: generateId(),
        role: 'assistant',
        kind: 'thinking',
        content: payload.delta,
        timestamp: Date.now(),
        isStreaming: true,
      };
      pushMessage(session, msg);
      currentThinkingIds.set(session.taskId, msg.id);
    } else {
      const target = session.messages.find((m) => m.id === currentId);
      if (target) target.content += payload.delta;
    }
  }

  /** 文本增量 — 关闭思考段，追加到当前 assistant 文本消息。 */
  function onText(payload: KbChatDeltaPayload): void {
    const session = findSession(payload.task_id);
    if (!session || payload.delta.length === 0) return;
    const currentId = currentTextIds.get(session.taskId) ?? null;
    if (currentId === null && payload.delta.trim().length === 0) return;

    const thinkingId = currentThinkingIds.get(session.taskId);
    if (thinkingId) {
      const thinking = session.messages.find((m) => m.id === thinkingId);
      if (thinking) thinking.isStreaming = false;
      currentThinkingIds.set(session.taskId, null);
    }

    if (currentId === null) {
      const msg: ChatMessage = {
        id: generateId(),
        role: 'assistant',
        kind: 'text',
        content: payload.delta,
        timestamp: Date.now(),
        isStreaming: true,
      };
      pushMessage(session, msg);
      currentTextIds.set(session.taskId, msg.id);
    } else {
      const target = session.messages.find((m) => m.id === currentId);
      if (target) target.content += payload.delta;
    }
  }

  /** 工具调用 — 关闭流式段并追加 tool_call 消息。 */
  function onToolCall(payload: KbChatToolCallPayload): void {
    const session = findSession(payload.task_id);
    if (!session) return;
    closeStreaming(session);
    pushMessage(session, {
      id: payload.id || generateId(),
      role: 'assistant',
      kind: 'tool_call',
      content: '',
      timestamp: Date.now(),
      toolName: payload.name,
      toolInput: payload.input,
    });
  }

  /** 工具结果 — 按 tool_call_id 回填。 */
  function onToolResult(payload: KbChatToolResultPayload): void {
    const session = findSession(payload.task_id);
    if (!session) return;
    const msg = session.messages.find(
      (m) => m.kind === 'tool_call' && m.id === payload.tool_call_id,
    );
    if (msg) {
      msg.toolResult = { ok: payload.ok, duration_ms: payload.duration_ms, ...((payload.result as object) ?? {}) };
    }
  }

  /** 阶段进度 — 仅在阶段变化时插入 status 消息（避免逐条目刷屏）。 */
  function onProgress(payload: KbBuildProgressPayload): void {
    const session = findSession(payload.task_id);
    if (!session || payload.stage === lastStageIds.get(session.taskId)) return;
    lastStageIds.set(session.taskId, payload.stage);
    closeStreaming(session);
    const stageLabel = t(`main.sidebar.references.kbStage.${payload.stage}`, payload.stage);
    pushMessage(session, {
      id: generateId(),
      role: 'assistant',
      kind: 'status',
      content: stageLabel,
      timestamp: Date.now(),
    });
  }

  /** 构建 — 终态处理（completed / failed / cancelled 共用）。 */
  function finalize(
    taskId: string,
    status: KbAgentSession['status'],
    message: string,
  ): void {
    const session = findSession(taskId);
    if (!session) return;
    closeStreaming(session, status !== 'completed');
    session.status = status;
    pushMessage(session, {
      id: generateId(),
      role: 'assistant',
      kind: 'status',
      content: message,
      timestamp: Date.now(),
    });
  }

  /* ── 事件订阅（幂等单例） ─────────────────────────────────────────────── */

  /** 注册所有事件监听（幂等，重复调用安全）。 */
  async function setupEventListeners(): Promise<void> {
    if (!isTauriEnvironment() || listenersReady) return;
    listenersReady = true;

    unlistenFns.push(
      await listen<KbBuildStartedPayload>(EVENT_KB_BUILD_STARTED, (e) => onStarted(e.payload)),
      await listen<KbBuildProgressPayload>(EVENT_KB_BUILD_PROGRESS, (e) => onProgress(e.payload)),
      await listen<KbBuildCompletedPayload>(EVENT_KB_BUILD_COMPLETED, (e) => {
        const { task_id, concept_ids, entity_ids } = e.payload;
        const total = concept_ids.length + entity_ids.length + (e.payload.summary_id ? 1 : 0);
        finalize(
          task_id,
          'completed',
          t('main.kbAgent.doneSummary', { count: total, relations: e.payload.relations_established ? t('main.kbAgent.doneRelations') : '' }),
        );
      }),
      await listen<KbBuildFailedPayload>(EVENT_KB_BUILD_FAILED, (e) => {
        finalize(e.payload.task_id, 'failed', `${t('main.kbAgent.failedPrefix')}${e.payload.error}`);
      }),
      await listen<KbBuildCancelledPayload>(EVENT_KB_BUILD_CANCELLED, (e) => {
        finalize(e.payload.task_id, 'cancelled', t('main.kbAgent.cancelled'));
      }),
      await listen<KbChatDeltaPayload>(EVENT_KBCHAT_THOUGHT, (e) => onThought(e.payload)),
      await listen<KbChatDeltaPayload>(EVENT_KBCHAT_TEXT, (e) => onText(e.payload)),
      await listen<KbChatToolCallPayload>(EVENT_KBCHAT_TOOL_CALL, (e) => onToolCall(e.payload)),
      await listen<KbChatToolResultPayload>(EVENT_KBCHAT_TOOL_RESULT, (e) => onToolResult(e.payload)),
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

  /* ── 对外方法 ─────────────────────────────────────────────────────────── */

  /** 切换面板聚焦的任务会话。 */
  function focusTask(taskId: string): void {
    activeTaskId.value = taskId;
  }

  return {
    // 状态（只读）
    sessions: readonly(sessions),
    activeTaskId: readonly(activeTaskId),
    // 方法
    focusTask,
    setupEventListeners,
    cleanupEventListeners,
  };
}

/** useKbAgentChat 返回值类型。 */
export type UseKbAgentChatReturn = ReturnType<typeof useKbAgentChat>;
