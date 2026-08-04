<script setup lang="ts">
/**
 * AIPanel — 右侧 AI 助手区。
 *
 * 三层结构：
 *   头部（Header）   — 智能体选择器 + 历史会话
 *   对话区（Chat）   — 消息列表（用户 / 助手）
 *   输入区（Input）  — 文本输入框 + 发送按钮
 *
 * 当前为骨架占位，消息列表和输入均为静态 UI，
 * 后续接入 agent_runtime 后实现实际交互。
 */
import { ref, nextTick, watch } from 'vue';
import type { ChatMessage } from '../types';
import { useI18n } from '../../../i18n';

const { t } = useI18n();

const props = defineProps<{
  /** 对话消息列表 */
  messages: readonly ChatMessage[];
  /** 是否正在生成回复 */
  isGenerating?: boolean;
}>();

const emit = defineEmits<{
  (e: 'send', content: string): void;
}>();

/* ── 输入框 ───────────────────────────────────────────────────────────── */
const inputText = ref('');
const chatBodyRef = ref<HTMLElement | null>(null);

/** 发送消息。 */
function handleSend(): void {
  const text = inputText.value.trim();
  if (!text) return;
  emit('send', text);
  inputText.value = '';
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
  () => props.messages.length,
  async () => {
    await nextTick();
    if (chatBodyRef.value) {
      chatBodyRef.value.scrollTop = chatBodyRef.value.scrollHeight;
    }
  },
);
</script>

<template>
  <div class="ai-panel">
    <!-- ── 头部 ─────────────────────────────────────────────────────── -->
    <div class="ai-panel__header">
      <button class="ai-panel__agent-selector">
        <span class="ai-panel__agent-avatar">
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M12 2a4 4 0 0 1 4 4v2a4 4 0 0 1-8 0V6a4 4 0 0 1 4-4z" />
            <path d="M4 20a8 8 0 0 1 16 0" />
          </svg>
        </span>
        <span class="ai-panel__agent-name">{{ t('main.aiPanel.agentName') }}</span>
        <svg class="ai-panel__agent-chevron" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <path d="m6 9 6 6 6-6" />
        </svg>
      </button>

      <button class="ai-panel__history" :title="t('main.aiPanel.history')">
        <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M3 12a9 9 0 1 0 3-6.7L3 8" />
          <path d="M3 3v5h5" />
          <path d="M12 7v5l3 2" />
        </svg>
      </button>
    </div>

    <!-- ── 对话区 ─────────────────────────────────────────────────────── -->
    <div ref="chatBodyRef" class="ai-panel__chat">
      <!-- 空状态 -->
      <div v-if="messages.length === 0" class="ai-panel__empty">
        <div class="ai-panel__empty-icon">
          <svg viewBox="0 0 24 24" width="36" height="36" fill="none" stroke="currentColor" stroke-width="1.5">
            <path d="M12 2a4 4 0 0 1 4 4v2a4 4 0 0 1-8 0V6a4 4 0 0 1 4-4z" />
            <path d="M4 20a8 8 0 0 1 16 0" />
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
          <div class="ai-panel__msg-content">{{ msg.content }}</div>
        </div>

        <!-- 生成中指示器 -->
        <div v-if="isGenerating" class="ai-panel__msg ai-panel__msg--assistant">
          <div class="ai-panel__typing">
            <span class="ai-panel__typing-dot" />
            <span class="ai-panel__typing-dot" />
            <span class="ai-panel__typing-dot" />
          </div>
        </div>
      </template>
    </div>

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
        <button
          class="ai-panel__send"
          :disabled="!inputText.trim()"
          @click="handleSend"
        >
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12 19V5M5 12l7-7 7 7" />
          </svg>
        </button>
      </div>
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
}

/* ── 头部 ─────────────────────────────────────────────────────────────── */
.ai-panel__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 8px 0 12px;
  height: 40px;
  flex-shrink: 0;
  border-bottom: 1px solid var(--fluen-hairline);
}

.ai-panel__agent-selector {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 8px;
  border: none;
  background: transparent;
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  border-radius: 6px;
  transition: background 0.15s ease;
}

.ai-panel__agent-selector:hover {
  background: var(--fluen-hover);
}

.ai-panel__agent-avatar {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border-radius: 50%;
  background: var(--fluen-accent);
  color: var(--fluen-on-accent);
  flex-shrink: 0;
}

.ai-panel__agent-chevron {
  color: var(--fluen-stone);
}

.ai-panel__history {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--fluen-slate);
  cursor: pointer;
  border-radius: 6px;
  transition: background 0.15s ease, color 0.15s ease;
}

.ai-panel__history:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

/* ── 对话区 ───────────────────────────────────────────────────────────── */
.ai-panel__chat {
  flex: 1;
  overflow-y: auto;
  padding: 16px 12px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

/* 空状态 */
.ai-panel__empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  text-align: center;
}

.ai-panel__empty-icon {
  color: var(--fluen-stone);
  margin-bottom: 4px;
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
  max-width: 240px;
  line-height: 1.5;
}

/* 消息气泡 */
.ai-panel__msg {
  display: flex;
  flex-direction: column;
  max-width: 90%;
}

.ai-panel__msg--user {
  align-self: flex-end;
  align-items: flex-end;
}

.ai-panel__msg--assistant {
  align-self: flex-start;
  align-items: flex-start;
}

.ai-panel__msg-content {
  padding: 8px 12px;
  border-radius: 12px;
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  line-height: 1.5;
  word-break: break-word;
}

.ai-panel__msg--user .ai-panel__msg-content {
  background: var(--fluen-accent);
  color: var(--fluen-on-accent);
  border-bottom-right-radius: 4px;
}

.ai-panel__msg--assistant .ai-panel__msg-content {
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  border: 1px solid var(--fluen-hairline);
  border-bottom-left-radius: 4px;
}

/* 打字指示器 */
.ai-panel__typing {
  display: flex;
  gap: 4px;
  padding: 10px 14px;
  background: var(--fluen-canvas);
  border: 1px solid var(--fluen-hairline);
  border-radius: 12px;
  border-bottom-left-radius: 4px;
}

.ai-panel__typing-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--fluen-stone);
  animation: ai-typing 1.4s infinite ease-in-out;
}

.ai-panel__typing-dot:nth-child(2) {
  animation-delay: 0.2s;
}

.ai-panel__typing-dot:nth-child(3) {
  animation-delay: 0.4s;
}

@keyframes ai-typing {
  0%, 60%, 100% { opacity: 0.3; transform: scale(0.8); }
  30% { opacity: 1; transform: scale(1); }
}

/* ── 输入区 ───────────────────────────────────────────────────────────── */
.ai-panel__input-area {
  flex-shrink: 0;
  padding: 12px;
  border-top: 1px solid var(--fluen-hairline);
}

.ai-panel__input-wrapper {
  display: flex;
  align-items: flex-end;
  gap: 8px;
  padding: 8px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 10px;
  background: var(--fluen-canvas);
  transition: border-color 0.15s ease;
}

.ai-panel__input-wrapper:focus-within {
  border-color: var(--fluen-accent);
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

.ai-panel__send {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  flex-shrink: 0;
  border: none;
  border-radius: 6px;
  background: var(--fluen-accent);
  color: var(--fluen-on-accent);
  cursor: pointer;
  transition: opacity 0.15s ease, background 0.15s ease;
}

.ai-panel__send:hover:not(:disabled) {
  background: var(--fluen-accent-hover);
}

.ai-panel__send:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
