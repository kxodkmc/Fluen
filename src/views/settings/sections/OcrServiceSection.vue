<script setup lang="ts">
/**
 * OcrServiceSection — OCR 服务配置分区（AI 服务分组下的子项）。
 *
 * 展示并编辑 OCR 提供商（PaddleOCR 等）的 API Key、地址、调用模式、
 * 识别选项与模型。复用共享的 OcrProviderList 组件。
 */
import { onMounted } from 'vue';
import { useI18n } from '../../../i18n';
import { useAiServicesSettings } from '../composables/useAiServicesSettings';
import OcrProviderList from './OcrProviderList.vue';

const { t } = useI18n();
const { load, error, isLoading } = useAiServicesSettings();

onMounted(() => {
  load();
});
</script>

<template>
  <div class="section">
    <h2 class="section__title">{{ t('settings.ocrService.title') }}</h2>
    <p class="section__desc">{{ t('settings.ocrService.description') }}</p>

    <div v-if="isLoading" class="section__loading">{{ t('settings.aiServices.loading') }}</div>
    <div v-if="error" class="section__error">{{ error }}</div>
    <template v-else-if="!isLoading">
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
</style>