<script setup lang="ts">
/**
 * MotisChatInput — Motis 对话输入区。
 *
 * 文本输入框 + 发送/停止按钮：
 *   - 生成中显示停止按钮（触发 cancel）
 *   - 非生成中显示发送按钮（触发 send）
 *   - 回车发送，Shift+回车换行
 *   - 使用 v-model 双向绑定草稿消息
 */
import { ref, watch, nextTick, onMounted } from 'vue';
import { useI18n } from '../../../../i18n';

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

/** 是否可发送（有内容且未在生成中）。 */
function canSend(): boolean {
  return props.modelValue.trim().length > 0 && !props.isGenerating;
}
</script>

<template>
  <div class="motis-input">
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
      <!-- 发送 / 停止按钮 -->
      <button
        v-if="!isGenerating"
        class="motis-input__btn motis-input__btn--send"
        :disabled="!canSend()"
        :title="t('main.motisPanel.sendMessage')"
        @click="handleSend"
      >
        <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M12 19V5M5 12l7-7 7 7" />
        </svg>
      </button>
      <button
        v-else
        class="motis-input__btn motis-input__btn--stop"
        :title="t('main.motisPanel.stop')"
        @click="$emit('cancel')"
      >
        <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
          <rect x="6" y="6" width="12" height="12" rx="2" />
        </svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
.motis-input {
  flex-shrink: 0;
  padding: 12px;
  border-top: 1px solid var(--fluen-hairline);
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

.motis-input__btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  flex-shrink: 0;
  border: none;
  border-radius: 50%;
  cursor: pointer;
  transition: opacity 0.15s ease, background 0.15s ease;
}

.motis-input__btn--send {
  background: var(--fluen-brand-coral);
  color: var(--fluen-on-dark);
}

.motis-input__btn--send:hover:not(:disabled) {
  opacity: 0.85;
}

.motis-input__btn--send:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.motis-input__btn--stop {
  background: var(--fluen-surface);
  color: var(--fluen-slate);
  border: 1px solid var(--fluen-hairline);
}

.motis-input__btn--stop:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}
</style>
