<script setup lang="ts">
/**
 * AIPanel — 右侧「学术助手」面板。
 *
 * 学术助手的职责：根据用户要求撰写**格式规范**的文章内容（fluen-markup）。
 * 面板自管理对话状态（useAIAssistant）：
 *
 *   - 消息列表：文本（Markdown 渲染）/ 思考指示器 / 工具调用 / 状态提示
 *   - 写操作审批弹窗：manuscript / project_write 等写工具调用前弹出确认，
 *     用户点击「应用」才真正写入
 *   - 输入区：发送 / 停止（生成中）
 */
import { ref, nextTick, watch, computed, inject } from 'vue';
import MarkdownIt from 'markdown-it';
import { useI18n } from '../../../i18n';
import { useAIAssistant } from '../composables/useAIAssistant';
import { useChatToolbar } from '../composables/useChatToolbar';
import { useMainLayout, MAIN_LAYOUT_KEY } from '../composables/useMainLayout';
import type { ChatMessage } from '../types';
import ApprovalDialog from './ApprovalDialog.vue';
import ChatInputToolbar from './ChatInputToolbar.vue';

const { t } = useI18n();
const ai = useAIAssistant();

const { messages, isGenerating, pendingApprovals, send, cancel, resolveApproval } = ai;

/* ── 布局与工具栏状态（模块级单例，与 MotisPanel 共享） ─────────────── */
const layout = inject(MAIN_LAYOUT_KEY, () => useMainLayout(), true);
const toolbar = useChatToolbar();

/** 工具栏模式切换时同步到布局状态（幂等展示，避免误触"点已激活项收起"）。 */
watch(
  () => toolbar.mode.value,
  (next) => {
    layout.showRightPanel(next);
  },
);

/** 外部切换面板时同步回工具栏。 */
watch(
  () => layout.activeRightPanel.value,
  (next) => {
    if (next && next !== toolbar.mode.value) {
      toolbar.mode.value = next;
    }
  },
  { immediate: true },
);

/* ── Markdown 渲染（助手消息） ───────────────────────────────────────── */
/**
 * markdown-it 实例（聊天专用）。
 *
 * - `html: false`：转义所有 HTML（含 `<f-cite>` 等 fluen 标签按原文文本显示），
 *   防止 LLM 输出注入任意 HTML
 * - `breaks: true`：单换行渲染为 `<br>`（聊天消息常用单换行分段）
 * - `linkify` / `typographer`：自动识别链接、优化排版
 */
const md = new MarkdownIt({
  html: false,
  linkify: true,
  breaks: true,
  typographer: true,
});

/** 将消息内容渲染为 HTML（渲染失败时回退原文）。 */
function renderMd(content: string): string {
  try {
    return md.render(content);
  } catch {
    return content;
  }
}

/* ── 输入框 ───────────────────────────────────────────────────────────── */
const inputText = ref('');
const chatBodyRef = ref<HTMLElement | null>(null);

/** 发送消息。 */
function handleSend(): void {
  const text = inputText.value.trim();
  if (!text || isGenerating.value) return;
  void send(text);
  inputText.value = '';
}

/** 停止生成。 */
function handleStop(): void {
  void cancel();
}

/** 按 Enter 发送，Shift+Enter 换行。 */
function handleKeydown(e: KeyboardEvent): void {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    handleSend();
  }
}

/* ── 自动滚动到底部 ───────────────────────────────────────────────────── */
watch(
  () => messages.value.length,
  async () => {
    await nextTick();
    if (chatBodyRef.value) {
      chatBodyRef.value.scrollTop = chatBodyRef.value.scrollHeight;
    }
  },
);

/* ── 消息渲染辅助 ─────────────────────────────────────────────────────── */

/** 工具调用展示名。 */
function toolDisplayName(msg: Pick<ChatMessage, 'toolName'>): string {
  switch (msg.toolName) {
    case 'paper_outline':
    case 'paper_section':
      return t('main.aiPanel.toolPaper');
    case 'manuscript':
      return t('main.aiPanel.toolManuscript');
    case 'project_read':
    case 'project_write':
    case 'project_edit':
      return t('main.aiPanel.toolProjectFile');
    default:
      return msg.toolName ?? '';
  }
}

/** 是否显示底部"生成中"指示器（生成期间常驻，位于消息列表末尾）。 */
const showGeneratingIndicator = computed(() => isGenerating.value && messages.value.length > 0);
</script>

<template>
  <div class="ai-panel">
    <!-- ── 头部 ─────────────────────────────────────────────────────── -->
    <div class="ai-panel__header">
      <div class="ai-panel__agent">
        <span class="ai-panel__agent-avatar">
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
            <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
          </svg>
        </span>
        <span class="ai-panel__agent-name">{{ t('main.aiPanel.agentName') }}</span>
      </div>
    </div>

    <!-- ── 对话区 ─────────────────────────────────────────────────────── -->
    <div ref="chatBodyRef" class="ai-panel__chat">
      <!-- 空状态 -->
      <div v-if="messages.length === 0" class="ai-panel__empty">
        <div class="ai-panel__empty-icon">
          <svg viewBox="0 0 24 24" width="36" height="36" fill="none" stroke="currentColor" stroke-width="1.5">
            <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
            <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
          </svg>
        </div>
        <p class="ai-panel__empty-title">{{ t('main.aiPanel.emptyTitle') }}</p>
        <p class="ai-panel__empty-hint">{{ t('main.aiPanel.emptyHint') }}</p>
      </div>

      <!-- 消息列表 -->
      <template v-else>
        <div
          v-for="msg in messages"
          :key="msg.id"
          class="ai-panel__msg"
          :class="`ai-panel__msg--${msg.role}`"
        >
          <!-- 用户文本 -->
          <div v-if="msg.kind === 'text' && msg.role === 'user'" class="ai-panel__bubble ai-panel__bubble--user">
            {{ msg.content }}
          </div>

          <!-- 助手文本（Markdown 渲染） -->
          <div v-else-if="msg.kind === 'text'" class="ai-panel__bubble ai-panel__bubble--assistant">
            <div class="ai-panel__markdown" v-html="renderMd(msg.content)" />
          </div>

          <!-- 思考指示器（无独立消息；由尾部生成中指示器呈现） -->

          <!-- 工具调用 -->
          <div v-else-if="msg.kind === 'tool_call'" class="ai-panel__tool-call">
            <span class="ai-panel__tool-call-icon">
              <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                <path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z" />
              </svg>
            </span>
            <span class="ai-panel__tool-call-name">{{ toolDisplayName(msg) }}</span>
          </div>

          <!-- 状态提示 -->
          <div v-else class="ai-panel__status" :class="{ 'ai-panel__status--error': msg.interrupted }">
            {{ msg.content }}
          </div>
        </div>

        <!-- 生成中指示器（生成期间常驻，位于消息列表末尾） -->
        <div v-if="showGeneratingIndicator" class="ai-panel__msg ai-panel__msg--assistant">
          <div class="ai-panel__bubble ai-panel__bubble--assistant">
            <div class="ai-panel__thinking">
              <span class="ai-panel__thinking-dot" />
              <span class="ai-panel__thinking-dot" />
              <span class="ai-panel__thinking-dot" />
            </div>
          </div>
        </div>
      </template>
    </div>

    <!-- ── 写操作确认弹窗（浮动层） ─────────────────────────────────── -->
    <ApprovalDialog
      :approvals="pendingApprovals"
      @resolve="(id, approved) => void resolveApproval(id, approved)"
    />

    <!-- ── 输入区 ─────────────────────────────────────────────────────── -->
    <div class="ai-panel__input-area">
      <div class="ai-panel__input-wrapper">
        <textarea
          v-model="inputText"
          class="ai-panel__input"
          :placeholder="t('main.aiPanel.inputPlaceholder')"
          rows="1"
          @keydown="handleKeydown"
        />
      </div>

      <!-- 底部工具栏 -->
      <ChatInputToolbar
        :mode="toolbar.mode.value"
        :thinking-intensity="toolbar.thinkingIntensity.value"
        :model-label="toolbar.activeModelLabel.value"
        :models="toolbar.modelOptions.value"
        :can-send="inputText.trim().length > 0 && !isGenerating"
        :is-generating="isGenerating"
        @update:mode="toolbar.mode.value = $event"
        @update:thinking-intensity="toolbar.thinkingIntensity.value = $event"
        @add-file="toolbar.addAttachment"
        @select-model="toolbar.selectModel"
        @send="handleSend"
        @stop="handleStop"
      />
    </div>
  </div>
</template>

<style scoped>
.ai-panel {
  display: flex;
  flex-shrink: 0;
  flex-direction: column;
  height: 100%;
  background: var(--fluen-surface);
  overflow: hidden;
  position: relative;
}

/* ── 头部 ─────────────────────────────────────────────────────────────── */
.ai-panel__header {
  display: flex;
  align-items: center;
  padding: 0 12px;
  height: 44px;
  flex-shrink: 0;
  border-bottom: 1px solid var(--fluen-hairline);
}

.ai-panel__agent {
  display: flex;
  align-items: center;
  gap: 8px;
}

.ai-panel__agent-avatar {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border-radius: 8px;
  background: linear-gradient(135deg, var(--fluen-accent), var(--fluen-accent-2, var(--fluen-accent)));
  color: var(--fluen-on-accent);
  flex-shrink: 0;
}

.ai-panel__agent-name {
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  font-weight: 600;
  color: var(--fluen-ink);
}

/* ── 对话区 ───────────────────────────────────────────────────────────── */
.ai-panel__chat {
  flex: 1;
  overflow-y: auto;
  padding: 16px 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

/* 空状态 */
.ai-panel__empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  text-align: center;
}

.ai-panel__empty-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 64px;
  height: 64px;
  border-radius: 50%;
  background: var(--fluen-canvas);
  border: 1px solid var(--fluen-hairline);
  color: var(--fluen-stone);
}

.ai-panel__empty-title {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 14px;
  font-weight: 600;
  color: var(--fluen-slate);
}

.ai-panel__empty-hint {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  color: var(--fluen-stone);
  max-width: 280px;
  line-height: 1.6;
}

/* 消息容器 */
.ai-panel__msg {
  display: flex;
  flex-direction: column;
  max-width: 100%;
}

.ai-panel__msg--user {
  align-self: flex-end;
  align-items: flex-end;
}

.ai-panel__msg--assistant {
  align-self: flex-start;
  align-items: flex-start;
}

/* 气泡 */
.ai-panel__bubble {
  padding: 8px 12px;
  border-radius: 12px;
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  line-height: 1.6;
  max-width: 100%;
}

.ai-panel__bubble--user {
  max-width: 88%;
  background: linear-gradient(135deg, var(--fluen-accent), var(--fluen-accent-hover));
  color: var(--fluen-on-accent);
  border-bottom-right-radius: 4px;
  white-space: pre-wrap;
  word-break: break-word;
}

.ai-panel__bubble--assistant {
  max-width: 100%;
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  border: 1px solid var(--fluen-hairline);
  border-bottom-left-radius: 4px;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.03);
}

/* ── Markdown 内容排版 ───────────────────────────────────────────────── */
.ai-panel__markdown {
  word-break: break-word;
  overflow-wrap: break-word;
}

.ai-panel__markdown > :first-child {
  margin-top: 0;
}

.ai-panel__markdown > :last-child {
  margin-bottom: 0;
}

.ai-panel__markdown p {
  margin: 0.4em 0;
}

.ai-panel__markdown h1,
.ai-panel__markdown h2,
.ai-panel__markdown h3,
.ai-panel__markdown h4,
.ai-panel__markdown h5,
.ai-panel__markdown h6 {
  margin: 0.9em 0 0.4em;
  font-weight: 600;
  line-height: 1.4;
  color: var(--fluen-ink);
}

.ai-panel__markdown h1 { font-size: 17px; }
.ai-panel__markdown h2 { font-size: 15px; }
.ai-panel__markdown h3 { font-size: 14px; }
.ai-panel__markdown h4,
.ai-panel__markdown h5,
.ai-panel__markdown h6 { font-size: 13px; }

.ai-panel__markdown ul,
.ai-panel__markdown ol {
  margin: 0.4em 0;
  padding-left: 1.4em;
}

.ai-panel__markdown li {
  margin: 0.2em 0;
}

.ai-panel__markdown blockquote {
  margin: 0.5em 0;
  padding: 2px 12px;
  border-left: 3px solid var(--fluen-accent);
  background: var(--fluen-surface-2, rgba(0, 0, 0, 0.03));
  border-radius: 0 6px 6px 0;
  color: var(--fluen-slate);
}

.ai-panel__markdown code {
  padding: 1px 5px;
  border-radius: 4px;
  background: rgba(0, 0, 0, 0.07);
  font-family: var(--fluen-font-mono);
  font-size: 12px;
  color: var(--fluen-ink);
}

.ai-panel__markdown pre {
  margin: 0.5em 0;
  padding: 10px 12px;
  border-radius: 8px;
  background: #1e1e2e;
  color: #e6e6e6;
  overflow-x: auto;
  font-size: 12px;
}

.ai-panel__markdown pre code {
  padding: 0;
  background: transparent;
  color: inherit;
}

.ai-panel__markdown table {
  margin: 0.5em 0;
  border-collapse: collapse;
  width: 100%;
  font-size: 12px;
}

.ai-panel__markdown th,
.ai-panel__markdown td {
  padding: 5px 10px;
  border: 1px solid var(--fluen-hairline);
  text-align: left;
  word-break: break-word;
}

.ai-panel__markdown th {
  background: var(--fluen-surface-2, rgba(0, 0, 0, 0.04));
  font-weight: 600;
}

.ai-panel__markdown a {
  color: var(--fluen-accent);
  text-decoration: underline;
}

.ai-panel__markdown hr {
  margin: 0.8em 0;
  border: none;
  border-top: 1px solid var(--fluen-hairline);
}

/* ── 思考指示器 ──────────────────────────────────────────────────────── */
.ai-panel__thinking {
  display: flex;
  gap: 5px;
  padding: 2px 0;
}

.ai-panel__thinking-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--fluen-stone);
  animation: ai-typing 1.4s infinite ease-in-out;
}

.ai-panel__thinking-dot:nth-child(2) {
  animation-delay: 0.2s;
}

.ai-panel__thinking-dot:nth-child(3) {
  animation-delay: 0.4s;
}

@keyframes ai-typing {
  0%, 60%, 100% { opacity: 0.3; transform: scale(0.8); }
  30% { opacity: 1; transform: scale(1); }
}

/* ── 工具调用 ────────────────────────────────────────────────────────── */
.ai-panel__tool-call {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 10px;
  border-radius: 8px;
  background: var(--fluen-canvas);
  border: 1px dashed var(--fluen-hairline);
  color: var(--fluen-slate);
  font-family: var(--fluen-font-sans);
  font-size: 12px;
}

.ai-panel__tool-call-icon {
  display: flex;
  color: var(--fluen-stone);
}

/* ── 状态提示 ────────────────────────────────────────────────────────── */
.ai-panel__status {
  align-self: center;
  padding: 4px 12px;
  border-radius: 8px;
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  color: var(--fluen-slate);
  background: var(--fluen-canvas);
  border: 1px solid var(--fluen-hairline);
  text-align: center;
}

.ai-panel__status--error {
  color: var(--fluen-danger, #d64545);
  border-color: color-mix(in srgb, var(--fluen-danger, #d64545) 35%, transparent);
  background: color-mix(in srgb, var(--fluen-danger, #d64545) 8%, transparent);
}

/* ── 输入区 ───────────────────────────────────────────────────────────── */
.ai-panel__input-wrapper {
  display: flex;
  align-items: flex-end;
  gap: 8px;
  padding: 8px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 10px;
  background: var(--fluen-canvas);
  transition: border-color 0.15s ease, box-shadow 0.15s ease;
}

.ai-panel__input-wrapper:focus-within {
  border-color: var(--fluen-accent);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--fluen-accent) 15%, transparent);
}

.ai-panel__input {
  flex: 1;
  border: none;
  outline: none;
  resize: none;
  background: transparent;
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  line-height: 1.5;
  max-height: 120px;
}

.ai-panel__input::placeholder {
  color: var(--fluen-stone);
}

.ai-panel__input-area {
  flex-shrink: 0;
  padding: 12px 12px 0;
  border-top: 1px solid var(--fluen-hairline);
  display: flex;
  flex-direction: column;
  gap: 8px;
}
</style>
