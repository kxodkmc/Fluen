<script setup lang="ts">
/**
 * AiServicesSection — 旧版 AI 服务配置分区（OLD-AI 服务）。
 *
 * 保留历史形态（文献导入模式 + OCR 提供商配置），供过渡期使用。
 * OCR 提供商配置复用共享的 OcrProviderList 组件。
 */
import { onMounted } from 'vue';
import { useI18n } from '../../../i18n';
import { useAiServicesSettings } from '../composables/useAiServicesSettings';
import OcrProviderList from './OcrProviderList.vue';

const { t } = useI18n();
const { load, isLoading, error } = useAiServicesSettings();

onMounted(() => {
  load();
});
</script>

<template>
  <div class="section">
    <h2 class="section__title">{{ t('settings.oldAiServices.title') }}</h2>
    <p class="section__desc">{{ t('settings.oldAiServices.description') }}</p>

    <div v-if="isLoading" class="section__loading">{{ t('settings.aiServices.loading') }}</div>
    <div v-if="error" class="section__error">{{ error }}</div>

    <template v-else-if="!isLoading">
      <section class="legacy-mode">
        <header class="legacy-mode__header">
          <h3 class="legacy-mode__title">{{ t('settings.aiServices.importMode') }}</h3>
        </header>
        <p class="legacy-mode__hint">{{ t('settings.aiServices.importModeHint') }}</p>
        <p class="legacy-mode__note">{{ t('settings.oldAiServices.note') }}</p>
      </section>

      <OcrProviderList />
    </template>
  </div>
</template>

<style scoped>
.section {
  padding: 0;
}

.section__title {
  margin: 0 0 0.25rem;
  font-family: var(--fluen-font-sans);
  font-size: 1.4rem;
  font-weight: 600;
  color: var(--fluen-ink);
  letter-spacing: -0.5px;
}

.section__desc {
  margin: 0 0 1.5rem;
  font-family: var(--fluen-font-sans);
  font-size: 0.85rem;
  color: var(--fluen-stone);
}

.section__loading {
  padding: 2rem 0;
  text-align: center;
  font-size: 0.85rem;
  color: var(--fluen-stone);
}

.section__error {
  padding: 0.75rem 1rem;
  margin-bottom: 1rem;
  border-radius: 8px;
  background: var(--fluen-error-bg);
  color: var(--fluen-error);
  font-size: 0.82rem;
}

/* ── 旧版迁移提示 ───────────────────────────────────────────────────── */
.legacy-mode {
  max-width: 640px;
  margin-bottom: 1.5rem;
  padding: 1rem 1.25rem;
  border: 1px dashed var(--fluen-hairline);
  border-radius: 12px;
  background: var(--fluen-surface);
}

.legacy-mode__title {
  margin: 0 0 0.3rem;
  font-family: var(--fluen-font-sans);
  font-size: 0.9rem;
  font-weight: 600;
  color: var(--fluen-ink);
}

.legacy-mode__hint {
  margin: 0 0 0.4rem;
  font-size: 0.78rem;
  color: var(--fluen-stone);
}

.legacy-mode__note {
  margin: 0;
  font-size: 0.72rem;
  color: var(--fluen-accent);
}
</style>