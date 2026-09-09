<script setup lang="ts">
/**
 * MotisChatInput — Motis 对话输入区。
 *
 * 仅保留多行文本输入框：
 *   - 回车发送，Shift+回车换行
 *   - 使用 v-model 双向绑定草稿消息
 *   - 发送/停止等操作由外层 ChatInputToolbar 提供
 */
import { ref, watch, nextTick, onMounted } from 'vue';
import { useI18n } from '../../../../i18n';
import { useChatQuotes } from '../../composables/useChatQuotes';

const props = defineProps<{
  /** 草稿文本（v-model）。 */
  modelValue: string;
  /** 是否正在生成。 */
  isGenerating: boolean;
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
  (e: 'send'): void;
  (e: 'cancel'): void;
}>();

const { t } = useI18n();

/** 论文编辑器划选加入的引用文段（发送时自动附带）。 */
const { quotes, removeQuote } = useChatQuotes();

/** 截断引用预览（超长以省略号收尾）。 */
function excerpt(text: string): string {
  const single = text.replace(/\s+/g, ' ');
  return single.length > 120 ? `${single.slice(0, 120)}…` : single;
}

const textareaRef = ref<HTMLTextAreaElement | null>(null);

/** 输入框内容（同步到 modelValue）。 */
function onInput(e: Event): void {
  emit('update:modelValue', (e.target as HTMLTextAreaElement).value);
}

/** 回车发送，Shift+回车换行。 */
function onKeydown(e: KeyboardEvent): void {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    handleSend();
  }
}

/** 发送消息。 */
function handleSend(): void {
  const text = props.modelValue.trim();
  if (!text || props.isGenerating) return;
  emit('send');
}

/**
 * 自动调整高度 + 草稿非空时自动聚焦。
 *
 * 当 draftMessage 从空变为非空时（如搜索栏 @Motis 预填），
 * 自动聚焦输入框以完成焦点转移。
 */
watch(
  () => props.modelValue,
  async (newVal, oldVal) => {
    await nextTick();
    if (textareaRef.value) {
      textareaRef.value.style.height = 'auto';
      textareaRef.value.style.height = `${Math.min(textareaRef.value.scrollHeight, 120)}px`;
    }
    // 草稿从空变为非空时自动聚焦（来自搜索栏 @Motis 预填）
    if (newVal && !oldVal) {
      textareaRef.value?.focus();
    }
  },
);

/**
 * 组件挂载时若已有草稿内容，自动聚焦。
 *
 * 场景：搜索栏 @Motis 触发时面板可能尚未渲染，setDraftMessage 先于面板挂载执行，
 * 此时 watch 不会触发（初始值即非空），需在 onMounted 补充聚焦。
 */
onMounted(() => {
  if (props.modelValue) {
    nextTick(() => textareaRef.value?.focus());
  }
});
</script>

<template>
  <div class="motis-input">
    <!-- 引用文段卡片（论文编辑器划选添加，发送时自动附带） -->
    <div v-if="quotes.length > 0" class="motis-quotes">
      <div v-for="quote in quotes" :key="quote.id" class="motis-quotes__card">
        <svg class="motis-quotes__icon" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M3 21c3-1 5-3.5 5-7V6H3v8h4c0 3-1.5 5-4 6v1z" />
          <path d="M14 21c3-1 5-3.5 5-7V6h-5v8h4c0 3-1.5 5-4 6v1z" />
        </svg>
        <span class="motis-quotes__text" :title="quote.text">{{ excerpt(quote.text) }}</span>
        <button
          type="button"
          class="motis-quotes__remove"
          :title="t('main.motisPanel.removeQuote')"
          @click="removeQuote(quote.id)"
        >
          <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            <path d="M18 6 6 18M6 6l12 12" />
          </svg>
        </button>
      </div>
    </div>
    <div class="motis-input__wrapper">
      <textarea
        ref="textareaRef"
        :value="modelValue"
        class="motis-input__textarea"
        :placeholder="t('main.motisPanel.inputPlaceholder')"
        rows="1"
        @input="onInput"
        @keydown="onKeydown"
      />
    </div>
  </div>
</template>

<style scoped>
.motis-input {
  flex-shrink: 0;
  padding: 12px 12px 0;
  background: var(--fluen-surface);
}

.motis-input__wrapper {
  display: flex;
  align-items: flex-end;
  gap: 8px;
  padding: 8px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 14px;
  background: var(--fluen-canvas);
  transition: border-color 0.15s ease;
}

.motis-input__wrapper:focus-within {
  border-color: var(--fluen-brand-coral);
}

.motis-input__textarea {
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

.motis-input__textarea::placeholder {
  color: var(--fluen-stone);
}

/* ── 引用文段卡片 ────────────────────────────────────────────────────── */
.motis-quotes {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: 8px;
}

.motis-quotes__card {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  padding: 8px 10px;
  border: 1px solid var(--fluen-hairline);
  border-left: 3px solid var(--fluen-brand-coral);
  border-radius: 10px;
  background: var(--fluen-canvas);
}

.motis-quotes__icon {
  flex-shrink: 0;
  margin-top: 2px;
  color: var(--fluen-stone);
}

.motis-quotes__text {
  flex: 1;
  min-width: 0;
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  line-height: 1.5;
  color: var(--fluen-slate);
  word-break: break-word;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.motis-quotes__remove {
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  width: 18px;
  height: 18px;
  margin-top: 1px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease;
}

.motis-quotes__remove:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

</style>
