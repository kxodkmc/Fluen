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

</style>
