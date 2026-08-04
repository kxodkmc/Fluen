<script setup lang="ts">
/**
 * StepTBD — 步骤 3：暂定占位。
 *
 * 此步骤为未来扩展预留，当前仅展示占位提示信息。
 */
import { useI18n } from '../../../i18n';

const { t } = useI18n();
</script>

<template>
  <div class="step">
    <h3 class="step__title">{{ t('onboarding.steps.tbd.title') }}</h3>
    <p class="step__desc">{{ t('onboarding.steps.tbd.description') }}</p>

    <div class="placeholder">
      <svg
        class="placeholder__icon"
        viewBox="0 0 24 24"
        width="48"
        height="48"
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <circle cx="12" cy="12" r="10" />
        <path d="M12 8v4" />
        <path d="M12 16h.01" />
      </svg>
      <p class="placeholder__text">{{ t('onboarding.steps.tbd.comingSoon') }}</p>
      <p class="placeholder__hint">{{ t('onboarding.steps.tbd.hint') }}</p>
    </div>
  </div>
</template>

<style scoped>
.step {
  padding: 0.5rem 0;
}

.step__title {
  margin: 0 0 0.5rem;
  font-family: var(--fluen-font-sans);
  font-size: 1.1rem;
  font-weight: 600;
  color: var(--fluen-on-dark);
}

.step__desc {
  margin: 0 0 1.5rem;
  font-family: var(--fluen-font-sans);
  font-size: 0.85rem;
  color: var(--fluen-stone);
}

.placeholder {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 2rem 1rem;
}

.placeholder__icon {
  color: var(--fluen-muted);
  margin-bottom: 1rem;
}

.placeholder__text {
  margin: 0 0 0.25rem;
  font-family: var(--fluen-font-sans);
  font-size: 1rem;
  font-weight: 500;
  color: var(--fluen-slate);
}

.placeholder__hint {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 0.8rem;
  color: var(--fluen-stone);
}
</style>
