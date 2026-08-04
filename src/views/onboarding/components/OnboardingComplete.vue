<script setup lang="ts">
/**
 * OnboardingComplete — stepper 完成后的展示阶段。
 *
 * 展示成功图标、用户配置摘要（主题、语言、LLM 提供商、模型），
 * 以及进入主应用的按钮。
 */
import { computed } from 'vue';
import { Motion } from 'motion-v';
import { useI18n } from '../../../i18n';
import type { OnboardingData } from '../types';

const props = defineProps<{
  data: OnboardingData;
}>();

defineEmits<{
  (e: 'enter'): void;
}>();

const { t } = useI18n();

const themeLabel = computed(
  () => t('settings.theme.' + props.data.theme + '.label'),
);

const languageLabel = computed(
  () => t('settings.language.' + props.data.language),
);

const providerName = computed(
  () => props.data.llmProvider?.name ?? t('common.notConfigured'),
);

const modelName = computed(() => {
  const provider = props.data.llmProvider;
  if (!provider || !props.data.activeModelId) return '—';
  const model = provider.models.find((m) => m.id === props.data.activeModelId);
  return model?.name ?? props.data.activeModelId;
});
</script>

<template>
  <Motion
    as="div"
    class="complete"
    :initial="{ opacity: 0, scale: 0.92 }"
    :animate="{ opacity: 1, scale: 1 }"
    :transition="{ duration: 0.5, ease: 'easeOut' }"
  >
    <!-- Success icon -->
    <Motion
      as="div"
      class="complete__icon"
      :initial="{ scale: 0, rotate: -30 }"
      :animate="{ scale: 1, rotate: 0 }"
      :transition="{ delay: 0.15, type: 'spring', stiffness: 200, damping: 15 }"
    >
      <svg viewBox="0 0 24 24" width="40" height="40" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
        <path d="M22 4 12 14.01l-3-3" />
      </svg>
    </Motion>

    <h3 class="complete__title">{{ t('onboarding.complete.title') }}</h3>
    <p class="complete__desc">{{ t('onboarding.complete.description') }}</p>

    <!-- Summary -->
    <div class="complete__summary">
      <div class="summary-row">
        <span class="summary-label">{{ t('onboarding.complete.themeMode') }}</span>
        <span class="summary-value">{{ themeLabel }}</span>
      </div>
      <div class="summary-row">
        <span class="summary-label">{{ t('onboarding.complete.language') }}</span>
        <span class="summary-value">{{ languageLabel }}</span>
      </div>
      <div class="summary-row">
        <span class="summary-label">{{ t('onboarding.complete.llmProvider') }}</span>
        <span class="summary-value">{{ providerName }}</span>
      </div>
      <div class="summary-row">
        <span class="summary-label">{{ t('onboarding.complete.model') }}</span>
        <span class="summary-value">{{ modelName }}</span>
      </div>
    </div>

    <!-- Enter button -->
    <button class="complete__btn" @click="$emit('enter')">
      {{ t('onboarding.complete.enterButton') }}
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M5 12h14" />
        <path d="m12 5 7 7-7 7" />
      </svg>
    </button>
  </Motion>
</template>

<style scoped>
.complete {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  padding: 2rem;
  max-width: 420px;
}

.complete__icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 64px;
  height: 64px;
  border-radius: 9999px;
  background: var(--fluen-info-bg);
  color: var(--fluen-accent);
  margin-bottom: 1.5rem;
}

.complete__title {
  margin: 0 0 0.25rem;
  font-family: var(--fluen-font-sans);
  font-size: 1.4rem;
  font-weight: 600;
  color: var(--fluen-on-dark);
}

.complete__desc {
  margin: 0 0 1.5rem;
  font-family: var(--fluen-font-sans);
  font-size: 0.85rem;
  color: var(--fluen-stone);
}

.complete__summary {
  width: 100%;
  margin-bottom: 2rem;
}

.summary-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.6rem 0;
  border-bottom: 1px solid var(--fluen-hairline);
}

.summary-row:last-child {
  border-bottom: none;
}

.summary-label {
  font-family: var(--fluen-font-sans);
  font-size: 0.8rem;
  color: var(--fluen-stone);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.summary-value {
  font-family: var(--fluen-font-sans);
  font-size: 0.9rem;
  color: var(--fluen-on-dark);
  font-weight: 500;
  max-width: 60%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.complete__btn {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  padding: 12px 28px;
  border: none;
  border-radius: 9999px;
  background: var(--fluen-primary);
  color: var(--fluen-on-primary);
  font-family: var(--fluen-font-sans);
  font-size: 0.9rem;
  font-weight: 600;
  cursor: pointer;
  transition: background-color 0.2s ease, transform 0.2s ease;
}

.complete__btn:hover {
  background: var(--fluen-primary-soft);
  transform: translateY(-1px);
}

.complete__btn:active {
  transform: translateY(0);
}
</style>
