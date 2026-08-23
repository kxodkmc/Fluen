/**
 * 学术助手聊天 composable。
 *
 * 封装 `ai_assistant_send` / `ai_assistant_cancel` 命令调用，
 * 订阅 Tauri 事件流（ai-assistant:thought / text / tool-call /
 * approval-request / finish / error），统一管理学术助手对话消息列表、
 * 生成状态与待审批的写操作列表。
 *
 * 与 useMotisChat 的差异：学术助手不依赖宠物助手配置（无心情/好感度/
 * 人格文案），思考内容不展示（仅显示"思考中…"指示器）。
 *
 * 事件流：
 * ```text
 * ai-assistant:thought          →  思考增量（仅驱动指示器）
 * ai-assistant:text              →  文本增量（追加到当前 assistant 文本消息）
 * ai-assistant:tool-call         →  工具调用（追加 tool_call 消息）
 * ai-assistant:approval-request  →  写操作审批请求（加入待确认列表）
 * ai-assistant:finish            →  完成（结束流式状态）
 * ai-assistant:error             →  错误（追加状态消息 + 标记中断）
 * ```
 */
import { ref, readonly, onScopeDispose, getCurrentScope } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useProject } from '../../../composables/useProject';
import type { ChatMessage } from '../types';

/** 传递给后端的历史消息结构。 */
interface HistoryEntry {
  role: string;
  content: string;
}

/* ── 事件 payload 类型（与后端 ai_assistant/commands.rs 对齐） ─────────── */
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
interface ApprovalRequestPayload {
  id: string;
  tool_name: string;
  input: unknown;
}
interface FinishPayload {
  result: unknown;
  total_tokens: number;
}
interface ErrorPayload {
  message: string;
}

/** 待审批的工具写操作（前端确认弹窗条目）。 */
export interface PendingApproval {
  id: string;
  toolName: string;
  /** 输入参数（含 path / action / content 等）。 */
  input: Record<string, unknown>;
}

function isTauriEnvironment(): boolean {
  return '__TAURI_INTERNALS__' in window;
}

function generateId(): string {
  return `msg-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

export function useAIAssistant() {
  const { currentProject } = useProject();

  const messages = ref<ChatMessage[]>([]);
  const isGenerating = ref(false);
  const draftMessage = ref('');
  /** 待审批的工具写操作列表（用户确认后才会真正写入）。 */
  const pendingApprovals = ref<PendingApproval[]>([]);

  /** 当前流式段内部指针（text 段）。 */
  let currentTextId: string | null = null;

  const unlistenFns: UnlistenFn[] = [];

  /** 从消息列表构造历史上下文（仅文本消息，排除工具调用/思考/状态）。 */
  function buildHistory(): HistoryEntry[] {
    const history: HistoryEntry[] = [];
    for (const msg of messages.value) {
      if (msg.kind !== 'text' || !msg.content.trim()) continue;
      history.push({ role: msg.role, content: msg.content });
    }
    return history;
  }

  /** 关闭当前流式段，标记中断（如有）。 */
  function closeStreamingMessages(interrupted = false): void {
    if (currentTextId) {
      const msg = messages.value.find((m) => m.id === currentTextId);
      if (msg) {
        msg.isStreaming = false;
        if (interrupted) msg.interrupted = true;
      }
    }
    currentTextId = null;
  }

  /** 收到思考增量：生成中指示器由 isGenerating 驱动，思考事件仅忽略。 */
  function onThought(): void {
    // 思考内容不展示；指示器统一由列表尾部的生成中指示器呈现
  }

  /** 收到文本增量：追加到当前 assistant 文本消息。 */
  function onText(payload: TextPayload): void {
    // 空 delta 忽略；首帧纯空白也忽略（不新建消息，避免产生空气泡）
    if (payload.delta.length === 0) return;
    if (currentTextId === null && payload.delta.trim().length === 0) return;
    if (currentTextId === null) {
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
    } else {
      const target = messages.value.find((m) => m.id === currentTextId);
      if (target) target.content += payload.delta;
    }
  }

  /** 收到工具调用通知：追加 tool_call 消息。 */
  function onToolCall(payload: ToolCallPayload): void {
    messages.value.push({
      id: payload.id || generateId(),
      role: 'assistant',
      kind: 'tool_call',
      content: '',
      timestamp: Date.now(),
      toolName: payload.name,
    });
  }

  /** 收到审批请求：加入待确认列表。 */
  function onApprovalRequest(payload: ApprovalRequestPayload): void {
    pendingApprovals.value.push({
      id: payload.id,
      toolName: payload.tool_name,
      input: (payload.input ?? {}) as Record<string, unknown>,
    });
  }

  /** 收到完成事件：结束流式状态。 */
  function onFinish(): void {
    closeStreamingMessages();
    isGenerating.value = false;
  }

  /** 收到错误：追加状态消息并标记中断。 */
  function onError(payload: ErrorPayload): void {
    closeStreamingMessages(true);
    messages.value.push({
      id: generateId(),
      role: 'assistant',
      kind: 'status',
      content: payload.message,
      timestamp: Date.now(),
      interrupted: true,
    });
    isGenerating.value = false;
  }

  /** 订阅学术助手事件流。 */
  async function subscribeEvents(): Promise<void> {
    const handlers = await Promise.all([
      listen<ThoughtPayload>('ai-assistant:thought', () => onThought()),
      listen<TextPayload>('ai-assistant:text', (e) => onText(e.payload)),
      listen<ToolCallPayload>('ai-assistant:tool-call', (e) => onToolCall(e.payload)),
      listen<ApprovalRequestPayload>('ai-assistant:approval-request', (e) =>
        onApprovalRequest(e.payload),
      ),
      listen<FinishPayload>('ai-assistant:finish', () => onFinish()),
      listen<ErrorPayload>('ai-assistant:error', (e) => onError(e.payload)),
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
        console.error('[useAIAssistant] 注销事件监听失败:', err);
      }
    }
  }

  /**
   * 发送写作请求并启动流式监听。
   *
   * 流程：懒加载事件监听 → 构造历史 → 追加用户消息 → 调用 ai_assistant_send
   * （携带当前打开的论文项目路径，供论文写作工具使用）。
   */
  async function send(message: string): Promise<void> {
    const text = message.trim();
    if (!text || isGenerating.value) return;

    // 懒加载事件监听（非 Tauri 环境跳过）
    if (isTauriEnvironment() && unlistenFns.length === 0) {
      await subscribeEvents();
    }

    // 构造历史（在追加用户消息之前）
    const history = buildHistory();

    // 追加用户消息 + 新一轮开始时清空遗留待审批项
    pendingApprovals.value = [];
    messages.value.push({
      id: generateId(),
      role: 'user',
      kind: 'text',
      content: text,
      timestamp: Date.now(),
    });

    isGenerating.value = true;
    currentTextId = null;

    if (!isTauriEnvironment()) {
      console.warn('[useAIAssistant] 非 Tauri 环境，已跳过实际发送。');
      isGenerating.value = false;
      return;
    }

    try {
      await invoke('ai_assistant_send', {
        message: text,
        history,
        projectPath: currentProject.value?.project_path ?? null,
      });
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      onError({ message: msg });
    }
  }

  /** 取消当前生成。 */
  async function cancel(): Promise<void> {
    if (!isGenerating.value) return;
    if (isTauriEnvironment()) {
      try {
        await invoke('ai_assistant_cancel');
      } catch (err) {
        console.error('[useAIAssistant] 取消失败:', err);
      }
    }
    // 取消后清理遗留的待审批项（对应审批请求已被后端终止）
    pendingApprovals.value = [];
    closeStreamingMessages(true);
    isGenerating.value = false;
  }

  /**
   * 回传审批决策：批准（应用）或拒绝。
   *
   * 审批通道与 Motis 共享（后端 approval_id 全局唯一），
   * 复用 `motis_chat_resolve_approval` 命令回传。
   */
  async function resolveApproval(id: string, approved: boolean): Promise<void> {
    if (isTauriEnvironment()) {
      try {
        await invoke('motis_chat_resolve_approval', { approvalId: id, approved });
      } catch (err) {
        console.error('[useAIAssistant] 回传审批决策失败:', err);
      }
    }
    pendingApprovals.value = pendingApprovals.value.filter((a) => a.id !== id);
  }

  /** 设置草稿消息。 */
  function setDraftMessage(text: string): void {
    draftMessage.value = text;
  }

  // 组件作用域销毁时自动注销事件监听
  if (getCurrentScope()) {
    onScopeDispose(() => {
      unsubscribeAll();
    });
  }

  return {
    // 状态
    messages: readonly(messages),
    isGenerating: readonly(isGenerating),
    pendingApprovals: readonly(pendingApprovals),
    draftMessage,

    // 方法
    send,
    cancel,
    resolveApproval,
    setDraftMessage,
  };
}

/** useAIAssistant 返回值类型（供 provide/inject 推导）。 */
export type UseAIAssistantReturn = ReturnType<typeof useAIAssistant>;
