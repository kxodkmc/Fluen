/**
 * Motis 聊天 composable。
 *
 * 封装 `motis_chat_send` / `motis_chat_cancel` 命令调用，
 * 订阅 Tauri 事件流（thought / text / tool-call / finish / error），
 * 统一管理 Motis 对话消息列表、生成状态与宠物状态气泡。
 *
 * 依据 MascotConfig 的 `show_thinking_content` / `professional_expression`
 * 决定思考内容是否展示，以及状态文案的风格（拟人化 / 专业化）。
 *
 * 事件流：
 * ```text
 * motis:thought   →  思考增量（依配置决定是否展示）
 * motis:text       →  文本增量（追加到当前 assistant 文本消息）
 * motis:tool-call   →  工具调用（追加 tool_call 消息 + 更新气泡）
 * motis:tool-result →  工具执行结果（按 tool_call_id 关联到工具消息）
 * motis:agent-*     →  子智能体委派过程（挂到工具消息的 agentRun：
 *                      started / tool-call / tool-result / finished）
 * motis:finish      →  完成（结束流式状态 + 清空气泡）
 * motis:error      →  错误（追加状态消息 + 标记中断 + 清空气泡）
 * ```
 *
 * @example
 * ```ts
 * const { messages, isGenerating, statusBubble, draftMessage, send, cancel, setDraftMessage } = useMotisChat();
 * await send('帮我总结这段话');
 * ```
 */

import { ref, readonly, onScopeDispose, getCurrentScope } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useLogger } from '../../../composables/useLogger';
import { useMascotConfig } from '../../../composables/useMascotConfig';
import { useProject } from '../../../composables/useProject';
import { useI18n } from '../../../i18n';
import type { MascotConfig } from '../../../types/mascot';
import type { ChatMessage } from '../types';
import {
  PLAYFUL_TOOL_MESSAGE_KEYS,
  PLAYFUL_THINKING_MESSAGE_KEYS,
  PROFESSIONAL_TOOL_MESSAGE_KEY,
  PROFESSIONAL_THINKING_MESSAGE_KEY,
  getRandomMessageKey,
} from '../constants';

/** 传递给后端的历史消息结构。 */
interface HistoryEntry {
  role: string;
  content: string;
}

/* ── 事件 payload 类型（与后端 events.rs 对齐） ──────────────────────── */
interface ThoughtPayload {
  delta: string;
}
interface TextPayload {
  delta: string;
}
interface ToolCallPayload {
  id: string;
  name: string;
  input: unknown;
}
interface ToolResultPayload {
  tool_call_id: string;
  name: string;
  result: unknown;
}
interface FinishPayload {
  result: unknown;
  total_tokens: number;
}
interface ErrorPayload {
  message: string;
}

/* ── 子智能体事件 payload（与后端 events.rs 对齐） ──────────────────── */
interface AgentStartedPayload {
  /** 父级 delegate_agent 工具调用 ID（关联 tool_call 消息）。 */
  tool_call_id: string;
  agent_id: string;
  task: string;
  timeout_ms: number;
}
interface AgentToolCallPayload {
  tool_call_id: string;
  agent_id: string;
  /** 子智能体会话内的工具调用 ID。 */
  id: string;
  name: string;
  input: unknown;
}
interface AgentToolResultPayload {
  tool_call_id: string;
  agent_id: string;
  id: string;
  name: string;
  ok: boolean;
  duration_ms: number;
  result: unknown;
}
interface AgentFinishedPayload {
  tool_call_id: string;
  agent_id: string;
  ok: boolean;
  duration_ms: number;
  tokens_used?: number;
  error?: string;
}

/** 工具审批请求 payload（与后端 events.rs 对齐）。 */
interface ApprovalRequestPayload {
  id: string;
  tool_name: string;
  input: unknown;
}

/** 待审批的工具写操作（前端确认弹窗条目）。 */
export interface PendingApproval {
  id: string;
  toolName: string;
  /** 输入参数（含 path / action / content 等）。 */
  input: Record<string, unknown>;
}
/** 默认 MascotConfig（加载失败或非 Tauri 环境时兜底）。 */
const DEFAULT_CONFIG: MascotConfig = {
  version: '1.0.0',
  name: 'Motis',
  enabled: true,
  provider_id: null,
  model_id: null,
  mcp_enabled: false,
  skills_enabled: false,
  function_calling_enabled: false,
  enabled_agents: [],
  personality: 'cheerful',
  show_thinking_content: false,
  professional_expression: false,
};

/**
 * 检测当前是否运行在 Tauri 环境中。
 * 与 useMascotConfig 保持一致的判定方式。
 */
function isTauriEnvironment(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** 生成唯一消息 ID（优先使用 crypto.randomUUID）。 */
function generateId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `msg-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

export function useMotisChat() {
  const { loadConfig } = useMascotConfig();
  const { t } = useI18n();
  /** 统一前端日志（桥接到后端同一日志文件）。 */
  const log = useLogger('motis-chat');
  /** 当前打开的论文项目（发送消息时读取 project_path，供论文内容工具使用）。 */
  const { currentProject } = useProject();

  /* ── 对外状态 ─────────────────────────────────────────────────────── */
  const messages = ref<ChatMessage[]>([]);
  const isGenerating = ref(false);
  const statusBubble = ref<string | null>(null);
  const currentRunId = ref<string | null>(null);
  /** 待审批的工具写操作列表（用户确认后才会真正写入）。 */
  const pendingApprovals = ref<PendingApproval[]>([]);
  /** 草稿消息（供搜索栏 @Motis 预填，可双向绑定）。 */
  const draftMessage = ref('');

  /* ── 内部状态 ─────────────────────────────────────────────────────── */
  /** 当前 MascotConfig（每次 send 前刷新）。 */
  let config: MascotConfig = { ...DEFAULT_CONFIG };

  /** 当前流式 assistant 文本消息 ID（用于增量追加）。 */
  let currentTextId: string | null = null;
  /** 当前流式 thinking 消息 ID（用于增量追加）。 */
  let currentThinkingId: string | null = null;

  /** 已注册的事件取消监听函数集合。 */
  const unlistenFns: UnlistenFn[] = [];

  /* ── 配置加载 ─────────────────────────────────────────────────────── */

  /** 刷新 MascotConfig（失败时回退到默认配置）。 */
  async function refreshConfig(): Promise<void> {
    try {
      config = await loadConfig();
    } catch (err) {
      log.error('加载配置失败', err);
      config = { ...DEFAULT_CONFIG };
    }
  }

  /* ── 文案策略 ─────────────────────────────────────────────────────── */

  /** 选取工具调用状态文案（依配置决定拟人化 / 专业化，通过 t() 翻译 i18n key）。 */
  function pickToolMessage(): string {
    return config.professional_expression
      ? t(PROFESSIONAL_TOOL_MESSAGE_KEY)
      : t(getRandomMessageKey(PLAYFUL_TOOL_MESSAGE_KEYS));
  }

  /** 选取思考状态文案（依配置决定拟人化 / 专业化，通过 t() 翻译 i18n key）。 */
  function pickThinkingMessage(): string {
    return config.professional_expression
      ? t(PROFESSIONAL_THINKING_MESSAGE_KEY)
      : t(getRandomMessageKey(PLAYFUL_THINKING_MESSAGE_KEYS));
  }

  /* ── 消息工具 ─────────────────────────────────────────────────────── */

  /** 关闭当前流式消息（标记 isStreaming=false，可选标记中断）。 */
  function closeStreamingMessages(interrupted = false): void {
    for (const id of [currentTextId, currentThinkingId]) {
      if (id === null) continue;
      const msg = messages.value.find((m) => m.id === id);
      if (msg) {
        msg.isStreaming = false;
        if (interrupted) msg.interrupted = true;
      }
    }
    currentTextId = null;
    currentThinkingId = null;
  }

  /** 构造传递给后端的历史消息（仅文本类，排除流式中与被中断的）。 */
  function buildHistory(): HistoryEntry[] {
    return messages.value
      .filter((m) => m.kind === 'text' && !m.isStreaming && !m.interrupted)
      .map((m) => ({ role: m.role, content: m.content }));
  }

  /* ── 事件处理 ─────────────────────────────────────────────────────── */

  /** 处理思考增量 — 依 show_thinking_content 决定是否追加为 thinking 消息。 */
  /** 收到审批请求：加入待确认列表，由用户点击「应用 / 拒绝」决定是否生效。 */
  function onApprovalRequest(payload: ApprovalRequestPayload): void {
    pendingApprovals.value.push({
      id: payload.id,
      toolName: payload.tool_name,
      input: (payload.input ?? {}) as Record<string, unknown>,
    });
  }

  /**
   * 回传审批决策：批准（应用）或拒绝。
   *
   * 成功后该条从待确认列表移除；后端据此放行或拦截对应工具调用。
   */
  async function resolveApproval(id: string, approved: boolean): Promise<void> {
    if (isTauriEnvironment()) {
      try {
        await invoke('motis_chat_resolve_approval', { approvalId: id, approved });
      } catch (err) {
        log.error('回传审批决策失败', err);
      }
    }
    pendingApprovals.value = pendingApprovals.value.filter((a) => a.id !== id);
  }

  function onThought(payload: ThoughtPayload): void {
    // 空 delta 忽略；首帧纯空白（如 "\n"）也忽略——避免空思考块隔断活动分组
    if (payload.delta.length === 0) return;
    if (currentThinkingId === null && payload.delta.trim().length === 0) return;
    // 始终维护思考状态气泡（若当前无气泡则填入思考文案）
    if (statusBubble.value === null) {
      statusBubble.value = pickThinkingMessage();
    }
    // 不展示思考内容时，仅保留状态气泡，不写入消息列表
    if (!config.show_thinking_content) return;

    // 追加到当前 thinking 消息（无则新建）
    if (currentThinkingId === null) {
      const msg: ChatMessage = {
        id: generateId(),
        role: 'assistant',
        kind: 'thinking',
        content: payload.delta,
        timestamp: Date.now(),
        isStreaming: true,
      };
      messages.value.push(msg);
      currentThinkingId = msg.id;
    } else {
      const target = messages.value.find((m) => m.id === currentThinkingId);
      if (target) target.content += payload.delta;
    }
  }

  /** 处理文本增量 — 追加到当前 assistant 文本消息，同时更新气泡显示实时回复内容。 */
  function onText(payload: TextPayload): void {
    // 空 delta 忽略；首帧纯空白（如 "\n"）也忽略——空气泡会隔断活动时间线分组，
    // 后续帧的空白正常拼接（保留词间空格与换行）
    if (payload.delta.length === 0) return;
    if (currentTextId === null && payload.delta.trim().length === 0) return;
    // 文本开始 → 关闭当前 thinking 段，切换为"作答"阶段
    if (currentThinkingId !== null) {
      const thinking = messages.value.find((m) => m.id === currentThinkingId);
      if (thinking) thinking.isStreaming = false;
      currentThinkingId = null;
    }

    if (currentTextId === null) {
      // 新建 assistant 文本消息，气泡同步显示回复内容（气泡即输出框）
      const msg: ChatMessage = {
        id: generateId(),
        role: 'assistant',
        kind: 'text',
        content: payload.delta,
        timestamp: Date.now(),
        isStreaming: true,
      };
      messages.value.push(msg);
      currentTextId = msg.id;
      statusBubble.value = msg.content;
    } else {
      const target = messages.value.find((m) => m.id === currentTextId);
      if (target) {
        target.content += payload.delta;
        // 气泡实时显示最新回复内容（截断过长文本，避免气泡撑爆）
        statusBubble.value = target.content;
      }
    }
  }

  /** 处理工具调用 — 追加 tool_call 消息并更新气泡为拟人化文案。 */
  function onToolCall(payload: ToolCallPayload): void {
    log.debug('工具调用', { name: payload.name, id: payload.id });
    // 关闭当前流式段（工具调用穿插在文本之间）
    closeStreamingMessages();

    messages.value.push({
      id: payload.id || generateId(),
      role: 'assistant',
      kind: 'tool_call',
      content: '',
      timestamp: Date.now(),
      toolName: payload.name,
      toolInput: payload.input,
    });
    statusBubble.value = pickToolMessage();
  }

  /** 处理工具结果 — 按 tool_call_id 关联到对应工具调用消息，写入执行结果。 */
  function onToolResult(payload: ToolResultPayload): void {
    const msg = messages.value.find((m) => m.kind === 'tool_call' && m.id === payload.tool_call_id);
    if (msg) {
      msg.toolResult = payload.result;
    }
  }

  /* ── 子智能体事件处理（按父级 tool_call_id 挂到工具消息的 agentRun） ── */

  /** 查找委派目标工具消息（不存在时返回 null）。 */
  function findDelegationMessage(toolCallId: string) {
    return messages.value.find((m) => m.kind === 'tool_call' && m.id === toolCallId) ?? null;
  }

  /** 委派发起 — 初始化工具消息的 agentRun 运行记录。 */
  function onAgentStarted(payload: AgentStartedPayload): void {
    const msg = findDelegationMessage(payload.tool_call_id);
    if (!msg) {
      log.warn('委派发起事件未找到对应工具消息', { toolCallId: payload.tool_call_id });
      return;
    }
    msg.agentRun = {
      agentId: payload.agent_id,
      task: payload.task,
      status: 'running',
      startedAt: Date.now(),
      activities: [],
    };
  }

  /** 子智能体内部工具调用 — 追加活动条目（数组引用整体替换）。 */
  function onAgentToolCall(payload: AgentToolCallPayload): void {
    const msg = findDelegationMessage(payload.tool_call_id);
    if (!msg?.agentRun) return;
    msg.agentRun.activities = [
      ...msg.agentRun.activities,
      {
        id: payload.id,
        name: payload.name,
        input: payload.input,
        running: true,
      },
    ];
  }

  /** 子智能体内部工具调用结束 — 按内部调用 ID 回填结果。 */
  function onAgentToolResult(payload: AgentToolResultPayload): void {
    const msg = findDelegationMessage(payload.tool_call_id);
    const activity = msg?.agentRun?.activities.find((a) => a.id === payload.id);
    if (!activity) return;
    activity.running = false;
    activity.ok = payload.ok;
    activity.result = payload.result;
    activity.durationMs = payload.duration_ms;
  }

  /** 委派结束 — 更新运行状态与统计。 */
  function onAgentFinished(payload: AgentFinishedPayload): void {
    const msg = findDelegationMessage(payload.tool_call_id);
    if (!msg?.agentRun) return;
    msg.agentRun.status = payload.ok ? 'done' : 'failed';
    msg.agentRun.durationMs = payload.duration_ms;
    if (payload.tokens_used !== undefined && payload.tokens_used !== null) {
      msg.agentRun.tokensUsed = payload.tokens_used;
    }
    if (payload.error) {
      msg.agentRun.error = payload.error;
    }
  }

  /** 气泡清除延迟（ms）——完成后保留气泡一段时间让用户读完最后回复。 */
  const BUBBLE_CLEAR_DELAY = 5000;
  /** 气泡清除计时器。 */
  let bubbleClearTimer: ReturnType<typeof setTimeout> | null = null;

  /** 延迟清空气泡（若已有计时器则先清除）。 */
  function scheduleBubbleClear(): void {
    if (bubbleClearTimer) clearTimeout(bubbleClearTimer);
    bubbleClearTimer = setTimeout(() => {
      bubbleClearTimer = null;
      // 仅在没有新的生成/思考时才清空
      if (!isGenerating.value) {
        statusBubble.value = null;
      }
    }, BUBBLE_CLEAR_DELAY);
  }

  /** 处理完成事件 — 结束流式状态，气泡延迟清除。 */
  function onFinish(_payload: FinishPayload): void {
    closeStreamingMessages();
    // 不立即清空气泡，延迟让用户读完最后回复
    scheduleBubbleClear();
    isGenerating.value = false;
    currentRunId.value = null;
  }

  /** 处理错误事件 — 追加状态消息、标记中断并清空气泡。 */
  function onError(payload: ErrorPayload): void {
    log.error('收到错误事件', { message: payload.message });
    closeStreamingMessages(true);
    messages.value.push({
      id: generateId(),
      role: 'assistant',
      kind: 'status',
      content: payload.message,
      timestamp: Date.now(),
    });
    statusBubble.value = null;
    isGenerating.value = false;
    currentRunId.value = null;
  }

  /* ── 事件订阅 ─────────────────────────────────────────────────────── */

  /** 注册所有 Tauri 事件监听（并发注册以提升效率）。 */
  async function subscribeEvents(): Promise<void> {
    const handlers = await Promise.all([
      listen<ThoughtPayload>('motis:thought', (e) => onThought(e.payload)),
      listen<TextPayload>('motis:text', (e) => onText(e.payload)),
      listen<ToolCallPayload>('motis:tool-call', (e) => onToolCall(e.payload)),
      listen<ToolResultPayload>('motis:tool-result', (e) => onToolResult(e.payload)),
      listen<ApprovalRequestPayload>('motis:approval-request', (e) => onApprovalRequest(e.payload)),
      listen<AgentStartedPayload>('motis:agent-started', (e) => onAgentStarted(e.payload)),
      listen<AgentToolCallPayload>('motis:agent-tool-call', (e) => onAgentToolCall(e.payload)),
      listen<AgentToolResultPayload>('motis:agent-tool-result', (e) => onAgentToolResult(e.payload)),
      listen<AgentFinishedPayload>('motis:agent-finished', (e) => onAgentFinished(e.payload)),
      listen<FinishPayload>('motis:finish', (e) => onFinish(e.payload)),
      listen<ErrorPayload>('motis:error', (e) => onError(e.payload)),
    ]);
    unlistenFns.push(...handlers);
  }

  /** 注销所有事件监听。 */
  function unsubscribeAll(): void {
    while (unlistenFns.length > 0) {
      const unlisten = unlistenFns.pop();
      try {
        unlisten?.();
      } catch (err) {
        console.error('[useMotisChat] 注销事件监听失败:', err);
      }
    }
  }

  /* ── 对外方法 ─────────────────────────────────────────────────────── */

  /**
   * 发送消息并启动流式监听。
   *
   * 流程：
   * 1. 刷新 MascotConfig（决定文案风格与思考内容展示）
   * 2. 首次调用时懒加载事件监听
   * 3. 基于已有消息构造历史上下文
   * 4. 追加用户消息、设置生成状态与思考气泡
   * 5. 调用 motis_chat_send，事件回调实时驱动消息更新
   */
  async function send(message: string): Promise<void> {
    const text = message.trim();
    if (!text || isGenerating.value) return;

    // 1. 刷新配置
    await refreshConfig();

    // 2. 懒加载事件监听（非 Tauri 环境跳过）
    if (isTauriEnvironment() && unlistenFns.length === 0) {
      await subscribeEvents();
    }

    // 3. 构造历史（在追加用户消息之前）
    const history = buildHistory();
    log.info('发送消息', {
      message: text,
      historyCount: history.length,
      projectPath: currentProject.value?.project_path ?? null,
    });

    // 4. 追加用户消息
    messages.value.push({
      id: generateId(),
      role: 'user',
      kind: 'text',
      content: text,
      timestamp: Date.now(),
    });

    // 5. 设置生成状态与思考气泡（新一轮开始时清空遗留的待审批项）
    pendingApprovals.value = [];
    isGenerating.value = true;
    currentRunId.value = generateId();
    currentTextId = null;
    currentThinkingId = null;
    statusBubble.value = pickThinkingMessage();

    // 6. 调用后端命令（非 Tauri 环境直接结束生成状态）
    if (!isTauriEnvironment()) {
      log.warn('非 Tauri 环境，已跳过实际发送');
      statusBubble.value = null;
      isGenerating.value = false;
      currentRunId.value = null;
      return;
    }

    try {
      // 携带当前打开的论文项目路径（供智能体读取论文内容的工具使用）
      await invoke('motis_chat_send', {
        message: text,
        history,
        projectPath: currentProject.value?.project_path ?? null,
      });
    } catch (err) {
      // invoke 抛错时（命令层异常），补一条错误消息
      const msg = err instanceof Error ? err.message : String(err);
      log.error('motis_chat_send 失败', { message: msg });
      onError({ message: msg });
    }
  }

  /** 取消当前生成。 */
  async function cancel(): Promise<void> {
    if (!isGenerating.value) return;
    if (isTauriEnvironment()) {
      try {
        await invoke('motis_chat_cancel');
      } catch (err) {
        console.error('[useMotisChat] 取消失败:', err);
      }
    }
    // 取消后清理遗留的待审批项（对应审批请求已被后端终止）
    pendingApprovals.value = [];
    // 标记当前消息被中断并重置状态
    closeStreamingMessages(true);
    statusBubble.value = null;
    isGenerating.value = false;
    currentRunId.value = null;
  }

  /**
   * 主动聊天（预留空实现）。
   *
   * 后续接入主动对话能力时填充，当前不产生任何副作用。
   */
  function proactiveMessage(_message: string): void {
    // TODO: 接入主动聊天能力
  }

  /** 设置草稿消息（供搜索栏 @Motis 预填调用）。 */
  function setDraftMessage(text: string): void {
    draftMessage.value = text;
  }

  /* ── 生命周期清理 ─────────────────────────────────────────────────── */

  // 组件作用域销毁时自动注销事件监听，避免内存泄漏
  if (getCurrentScope()) {
    onScopeDispose(() => {
      unsubscribeAll();
      if (bubbleClearTimer) {
        clearTimeout(bubbleClearTimer);
        bubbleClearTimer = null;
      }
    });
  }

  return {
    // 状态（只读，仅内部可变）
    messages: readonly(messages),
    isGenerating: readonly(isGenerating),
    statusBubble: readonly(statusBubble),
    currentRunId: readonly(currentRunId),
    pendingApprovals: readonly(pendingApprovals),
    // 草稿消息（可双向绑定）
    draftMessage,

    // 方法
    send,
    cancel,
    proactiveMessage,
    setDraftMessage,
    resolveApproval,
  };
}

/** useMotisChat 返回值类型（供 provide/inject 推导）。 */
export type UseMotisChatReturn = ReturnType<typeof useMotisChat>;
