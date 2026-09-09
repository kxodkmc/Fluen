<script setup lang="ts">
/**
 * MotisChatMessageList — Motis 对话消息列表。
 *
 * 布局参照「活动分组」风格：
 *   - 用户消息：右对齐气泡
 *   - 助手文本：左对齐 Markdown 气泡
 *   - 连续的思考/工具调用：合并为可折叠时间线块（MotisActivityGroup）
 *   - 状态消息：居中提示
 *
 * 自动滚动到底部（watch messages 长度）。
 * 生成中且末尾无流式文本、无活动组时，追加尾部思考指示器。
 */
import { ref, watch, nextTick, computed } from 'vue';
import MarkdownIt from 'markdown-it';
import type { ChatMessage } from '../../types';
import { useI18n } from '../../../../i18n';
import MotisThinkingIndicator from './MotisThinkingIndicator.vue';
import MotisActivityGroup from './MotisActivityGroup.vue';

/** 渲染块 — 单条普通消息或一个活动分组。 */
type RenderBlock =
  | { type: 'msg'; key: string; msg: ChatMessage }
  | { type: 'activity'; key: string; items: ChatMessage[] };

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
/** 是否需要显示尾部思考指示器（生成中且末尾无流式文本、无活动组）。 */
const showTrailingThinking = computed(() => {
  if (!props.isGenerating) return false;
  const last = props.messages[props.messages.length - 1];
  if (!last) return true;
  // 最后一条是流式文本时不显示（文本正在输出）
  if (last.kind === 'text' && last.isStreaming) return false;
  // 末尾是思考/工具调用时由活动组自身展示进行中状态
  if (last.kind === 'thinking' || last.kind === 'tool_call') return false;
  return true;
});

/* ── 活动分组 ─────────────────────────────────────────────────────────── */
/**
 * 将消息折叠为渲染块：连续的 thinking / tool_call 合并为一个活动分组，
 * 文本与状态消息保持单条渲染。分组的 key 取组内首条消息 id。
 *
 * 纯空白的 assistant 文本消息不生成渲染块（防御性兜底：流式首帧空白
 * 等场景产生的空消息既无内容又会隔断活动分组的连续性）。
 */
const blocks = computed<RenderBlock[]>(() => {
  const out: RenderBlock[] = [];
  let activity: ChatMessage[] | null = null;
  const flush = () => {
    if (activity && activity.length > 0) {
      out.push({ type: 'activity', key: activity[0].id, items: activity });
    }
    activity = null;
  };
  for (const msg of props.messages) {
    if (msg.kind === 'thinking' || msg.kind === 'tool_call') {
      if (activity === null) activity = [];
      activity.push(msg);
    } else {
      if (msg.kind === 'text' && msg.role === 'assistant' && !msg.content.trim()) {
        continue;
      }
      flush();
      out.push({ type: 'msg', key: msg.id, msg });
    }
  }
  flush();
  return out;
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
      <template v-for="(block, index) in blocks" :key="block.key">
        <!-- 活动分组（连续思考/工具调用的时间线块） -->
        <MotisActivityGroup
          v-if="block.type === 'activity'"
          :items="block.items"
          :active="isGenerating && index === blocks.length - 1"
        />

        <!-- 单条消息 -->
        <template v-else>
          <!-- 文本消息 -->
          <div
            v-if="block.msg.kind === 'text'"
            class="motis-msg"
            :class="`motis-msg--${block.msg.role}`"
          >
            <div class="motis-msg__bubble">
              <!-- 用户消息：纯文本；助手消息：Markdown 渲染 -->
              <div
                v-if="block.msg.role === 'assistant'"
                class="motis-msg__markdown"
                v-html="renderMd(block.msg.content)"
              />
              <template v-else>
                <!-- 附带引用文段（论文编辑器划选添加） -->
                <div v-if="block.msg.quotes?.length" class="motis-msg__quotes">
                  <div
                    v-for="(quote, qi) in block.msg.quotes"
                    :key="qi"
                    class="motis-msg__quote"
                  >{{ quote }}</div>
                </div>
                <span class="motis-msg__text">{{ block.msg.content }}</span><span
                  v-if="block.msg.isStreaming"
                  class="motis-msg__cursor"
                />
              </template>
              <span
                v-if="block.msg.role === 'assistant' && block.msg.isStreaming"
                class="motis-msg__cursor motis-msg__cursor--after"
              />
              <!-- 中断标记 -->
              <span v-if="block.msg.interrupted" class="motis-msg__interrupted">
                {{ t('main.motisPanel.interrupted') }}
              </span>
            </div>
          </div>

          <!-- 状态消息（错误/提示） -->
          <div v-else-if="block.msg.kind === 'status'" class="motis-msg-list__status">
            {{ block.msg.content }}
          </div>
        </template>
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

/* 用户消息附带的引用文段（论文划选） */
.motis-msg__quotes {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 6px;
  padding-left: 8px;
  border-left: 2px solid rgba(255, 255, 255, 0.55);
}

.motis-msg__quote {
  font-size: 12px;
  line-height: 1.5;
  opacity: 0.85;
  white-space: pre-wrap;
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
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
