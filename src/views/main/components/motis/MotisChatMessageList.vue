<script setup lang="ts">
/**
 * MotisChatMessageList — Motis 对话消息列表。
 *
 * 萌系圆润样式：
 *   - 用户消息：右对齐，brand-coral 背景
 *   - 助手消息：左对齐，canvas 背景 + hairline 边框
 *   - 思考消息：MotisThinkingIndicator
 *   - 工具调用：MotisToolCallBubble
 *   - 状态消息：居中提示
 *
 * 自动滚动到底部（watch messages 长度）。
 * 生成中且最后一条非流式文本时，追加思考指示器。
 */
import { ref, watch, nextTick, computed } from 'vue';
import MarkdownIt from 'markdown-it';
import type { ChatMessage } from '../../types';
import { useI18n } from '../../../../i18n';
import MotisThinkingIndicator from './MotisThinkingIndicator.vue';
import MotisToolCallBubble from './MotisToolCallBubble.vue';

const props = defineProps<{
  /** 消息列表。 */
  messages: readonly ChatMessage[];
  /** 是否正在生成。 */
  isGenerating: boolean;
}>();

const { t } = useI18n();

/* ── Markdown 渲染（助手消息） ───────────────────────────────────────── */
/**
 * markdown-it 实例（聊天专用）。
 *
 * - `html: false`：转义所有 HTML，防止 LLM 输出注入任意标签
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

const scrollRef = ref<HTMLElement | null>(null);

/* ── 自动滚动到底部 ───────────────────────────────────────────────────── */
watch(
  () => props.messages.length,
  async () => {
    await nextTick();
    if (scrollRef.value) {
      scrollRef.value.scrollTop = scrollRef.value.scrollHeight;
    }
  },
);

// 流式追加时也滚动（内容增长但消息数不变）
watch(
  () => props.messages.map((m) => m.content).join(''),
  async () => {
    await nextTick();
    if (scrollRef.value) {
      scrollRef.value.scrollTop = scrollRef.value.scrollHeight;
    }
  },
);

/* ── 尾部思考指示器 ───────────────────────────────────────────────────── */
/** 是否需要显示尾部思考指示器（生成中且最后一条非流式文本）。 */
const showTrailingThinking = computed(() => {
  if (!props.isGenerating) return false;
  const last = props.messages[props.messages.length - 1];
  if (!last) return true;
  // 最后一条是流式文本时不显示（文本正在输出）
  return !(last.kind === 'text' && last.isStreaming);
});
</script>

<template>
  <div ref="scrollRef" class="motis-msg-list">
    <!-- 空状态 -->
    <div v-if="messages.length === 0" class="motis-msg-list__empty">
      <div class="motis-msg-list__empty-icon">
        <svg viewBox="0 0 24 24" width="32" height="32" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
          <circle cx="12" cy="12" r="10" />
          <path d="M8 14s1.5 2 4 2 4-2 4-2" />
          <line x1="9" y1="9" x2="9.01" y2="9" />
          <line x1="15" y1="9" x2="15.01" y2="9" />
        </svg>
      </div>
      <p class="motis-msg-list__empty-hint">{{ t('main.motisPanel.searchMotisHint') }}</p>
    </div>

    <!-- 消息列表 -->
    <template v-else>
      <template v-for="msg in messages" :key="msg.id">
        <!-- 文本消息 -->
        <div
          v-if="msg.kind === 'text'"
          class="motis-msg"
          :class="`motis-msg--${msg.role}`"
        >
          <div class="motis-msg__bubble">
            <!-- 用户消息：纯文本；助手消息：Markdown 渲染 -->
            <div v-if="msg.role === 'assistant'" class="motis-msg__markdown" v-html="renderMd(msg.content)" />
            <template v-else>
              <span class="motis-msg__text">{{ msg.content }}</span><span v-if="msg.isStreaming" class="motis-msg__cursor" />
            </template>
            <span v-if="msg.role === 'assistant' && msg.isStreaming" class="motis-msg__cursor motis-msg__cursor--after" />
            <!-- 中断标记 -->
            <span v-if="msg.interrupted" class="motis-msg__interrupted">
              {{ t('main.motisPanel.interrupted') }}
            </span>
          </div>
        </div>

        <!-- 思考消息 -->
        <MotisThinkingIndicator
          v-else-if="msg.kind === 'thinking'"
          :content="msg.content"
          :active="msg.isStreaming !== false"
        />

        <!-- 工具调用消息 -->
        <MotisToolCallBubble
          v-else-if="msg.kind === 'tool_call'"
          :tool-name="msg.toolName"
          :input="msg.toolInput"
          :result="msg.toolResult"
        />

        <!-- 状态消息（错误/提示） -->
        <div v-else-if="msg.kind === 'status'" class="motis-msg-list__status">
          {{ msg.content }}
        </div>
      </template>

      <!-- 尾部思考指示器 -->
      <MotisThinkingIndicator v-if="showTrailingThinking" />
    </template>
  </div>
</template>

<style scoped>
.motis-msg-list {
  flex: 1;
  overflow-y: auto;
  padding: 16px 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

/* ── 空状态 ─────────────────────────────────────────────────────────── */
.motis-msg-list__empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  text-align: center;
}

.motis-msg-list__empty-icon {
  color: var(--fluen-brand-coral);
  opacity: 0.6;
  margin-bottom: 4px;
}

.motis-msg-list__empty-hint {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  color: var(--fluen-stone);
  max-width: 220px;
  line-height: 1.5;
}

/* ── 文本消息 ───────────────────────────────────────────────────────── */
.motis-msg {
  display: flex;
  flex-direction: column;
  max-width: 88%;
}

.motis-msg--user {
  align-self: flex-end;
  align-items: flex-end;
}

.motis-msg--assistant {
  align-self: flex-start;
  align-items: flex-start;
}

.motis-msg__bubble {
  padding: 8px 14px;
  border-radius: 14px;
  max-width: 100%;
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  line-height: 1.6;
  word-break: break-word;
}

.motis-msg--user .motis-msg__bubble {
  background: var(--fluen-brand-coral);
  color: var(--fluen-on-dark);
  border-bottom-right-radius: 4px;
}

.motis-msg--assistant .motis-msg__bubble {
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  border: 1px solid var(--fluen-hairline);
  border-bottom-left-radius: 4px;
}

.motis-msg__text {
  white-space: pre-wrap;
}

/* 助手消息 Markdown 渲染：继承气泡字体，重置块级默认边距，词长换行 */
.motis-msg__markdown {
  word-break: break-word;
  line-height: 1.6;
}

.motis-msg__markdown :deep(p),
.motis-msg__markdown :deep(ul),
.motis-msg__markdown :deep(ol),
.motis-msg__markdown :deep(pre),
.motis-msg__markdown :deep(blockquote) {
  margin: 0 0 6px;
}

.motis-msg__markdown :deep(:last-child) {
  margin-bottom: 0;
}

.motis-msg__markdown :deep(ul),
.motis-msg__markdown :deep(ol) {
  padding-left: 18px;
}

.motis-msg__markdown :deep(code) {
  background: var(--fluen-hover);
  border-radius: 4px;
  padding: 1px 4px;
  font-family: var(--fluen-font-mono);
  font-size: 0.92em;
}

.motis-msg__markdown :deep(pre) {
  background: var(--fluen-surface);
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  padding: 8px 12px;
  overflow-x: auto;
}

.motis-msg__markdown :deep(pre code) {
  background: transparent;
  padding: 0;
}

/* 流式光标（行内，跟随文字） */
.motis-msg__cursor {
  display: inline-block;
  width: 2px;
  height: 1em;
  vertical-align: text-bottom;
  background: var(--fluen-brand-coral);
  animation: motis-cursor-blink 1s infinite;
  margin-left: 1px;
}

/* 助手消息 markdown 之后的流式光标（独立行内块，跟随内容末尾） */
.motis-msg__cursor--after {
  margin-top: 2px;
}

@keyframes motis-cursor-blink {
  0%, 50% { opacity: 1; }
  51%, 100% { opacity: 0; }
}

/* 中断标记 */
.motis-msg__interrupted {
  font-size: 11px;
  color: var(--fluen-stone);
  font-style: italic;
  padding-left: 4px;
  border-left: 1px solid var(--fluen-hairline);
}

/* ── 状态消息 ───────────────────────────────────────────────────────── */
.motis-msg-list__status {
  align-self: center;
  padding: 4px 12px;
  border-radius: 9999px;
  background: var(--fluen-error-bg);
  color: var(--fluen-error);
  font-family: var(--fluen-font-sans);
  font-size: 11px;
  text-align: center;
  max-width: 90%;
  word-break: break-word;
}
</style>
