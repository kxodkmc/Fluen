<script setup lang="ts">
/**
 * AiServicesSection — AI 服务配置设置分区。
 *
 * 功能：
 *   - 列出已配置的 OCR 提供商（含激活标记）
 *   - 编辑提供商 API Key / Base URL
 *   - 选择 PaddleOCR API 模式（Job / Sync）
 *   - 切换识别选项（方向分类 / 去畸变 / 图表识别）
 *   - 设置激活的提供商与模型
 *
 * 状态管理委托给 useAiServicesSettings composable，组件仅负责 UI。
 */
import { ref, onMounted, computed } from 'vue';
import { useI18n } from '../../../i18n';
import { useAiServicesSettings } from '../composables/useAiServicesSettings';
import type { AiServiceProvider, PaddleOcrConfig } from '../../../types/aiServices';

const { t } = useI18n();
const {
  ocrProviders,
  selectedProvider,
  selectedProviderId,
  activeOcrProviderId,
  isLoading,
  isSaving,
  error,
  load,
  selectProvider,
  updateApiKey,
  updateBaseUrl,
  updateApiMode,
  updateOcrOptions,
  setActiveOcrProvider,
  setActiveModel,
  getPaddleOcrConfig,
} = useAiServicesSettings();

onMounted(() => {
  load();
});

/* ── 编辑表单状态 ───────────────────────────────────────────────────── */
const editApiKey = ref('');
const editBaseUrl = ref('');

/** 选中提供商时同步表单字段。 */
function onExpand(provider: AiServiceProvider): void {
  selectProvider(provider.id);
  editApiKey.value = provider.api_key ?? '';
  editBaseUrl.value = provider.api_base_url ?? '';
}

/** 保存 API Key。 */
async function handleSaveApiKey(): Promise<void> {
  if (!selectedProvider.value) return;
  await updateApiKey(selectedProvider.value.id, editApiKey.value);
}

/** 保存 Base URL。 */
async function handleSaveBaseUrl(): Promise<void> {
  if (!selectedProvider.value) return;
  await updateBaseUrl(selectedProvider.value.id, editBaseUrl.value);
}

/** 切换 API 模式。 */
async function handleApiModeChange(event: Event): Promise<void> {
  if (!selectedProvider.value) return;
  const target = event.target as HTMLSelectElement;
  const mode = target.value as 'job' | 'sync';
  await updateApiMode(selectedProvider.value.id, mode);
}

/** 切换识别选项。 */
async function handleOptionChange(
  key: keyof PaddleOcrConfig['options'],
  event: Event,
): Promise<void> {
  if (!selectedProvider.value) return;
  const target = event.target as HTMLInputElement;
  await updateOcrOptions(selectedProvider.value.id, { [key]: target.checked });
}

/** 设为激活提供商。 */
async function handleSetActive(provider: AiServiceProvider): Promise<void> {
  await setActiveOcrProvider(provider.id);
}

/** 设为激活模型。 */
async function handleSetActiveModel(provider: AiServiceProvider, modelId: string): Promise<void> {
  await setActiveModel(provider.id, modelId);
}

/** 判断提供商是否已配置 API Key。 */
function hasApiKey(provider: AiServiceProvider): boolean {
  return !!provider.api_key;
}

/** 获取提供商的 PaddleOCR 配置（响应式计算）。 */
const selectedPaddleConfig = computed<PaddleOcrConfig | null>(() => {
  if (!selectedProvider.value) return null;
  return getPaddleOcrConfig(selectedProvider.value);
});
</script>

<template>
  <div class="section">
    <h2 class="section__title">{{ t('settings.sections.aiServices') }}</h2>
    <p class="section__desc">{{ t('settings.aiServices.description') }}</p>

    <!-- 加载中 -->
    <div v-if="isLoading" class="section__loading">
      {{ t('settings.aiServices.loading') }}
    </div>

    <!-- 错误提示 -->
    <div v-if="error" class="section__error">
      {{ error }}
    </div>

    <template v-else-if="!isLoading">
      <!-- ── OCR 提供商列表 ────────────────────────────────────────── -->
      <div v-if="ocrProviders.length > 0" class="provider-list">
        <div
          v-for="provider in ocrProviders"
          :key="provider.id"
          class="provider-card"
          :class="{ 'provider-card--active': activeOcrProviderId === provider.id }"
        >
          <!-- 卡片头部 -->
          <div class="provider-card__header" @click="onExpand(provider)">
            <div class="provider-card__info">
              <span class="provider-card__name">{{ provider.name }}</span>
              <span v-if="activeOcrProviderId === provider.id" class="provider-card__badge">
                {{ t('settings.aiServices.active') }}
              </span>
            </div>
            <div class="provider-card__meta">
              <span
                class="provider-card__key-status"
                :class="{ 'provider-card__key-status--ok': hasApiKey(provider) }"
              >
                {{ hasApiKey(provider) ? t('settings.aiServices.apiKeySet') : t('settings.aiServices.apiKeyMissing') }}
              </span>
            </div>
            <svg
              class="provider-card__chevron"
              :class="{ 'provider-card__chevron--open': selectedProviderId === provider.id }"
              viewBox="0 0 24 24"
              width="16"
              height="16"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="m6 9 6 6 6-6" />
            </svg>
          </div>

          <!-- 展开编辑区 -->
          <Transition name="expand">
            <div v-if="selectedProviderId === provider.id" class="provider-card__body">
              <!-- API Base URL -->
              <div class="form-group">
                <label class="form-label">{{ t('settings.aiServices.baseUrl') }}</label>
                <div class="input-row">
                  <input
                    v-model="editBaseUrl"
                    class="form-input"
                    type="text"
                    :placeholder="t('settings.aiServices.placeholders.baseUrl')"
                  />
                  <button class="btn btn--secondary" @click="handleSaveBaseUrl">
                    {{ t('settings.aiServices.save') }}
                  </button>
                </div>
              </div>

              <!-- API Key -->
              <div class="form-group">
                <label class="form-label">{{ t('settings.aiServices.apiKey') }}</label>
                <div class="input-row">
                  <input
                    v-model="editApiKey"
                    class="form-input"
                    type="password"
                    :placeholder="t('settings.aiServices.placeholders.apiKey')"
                    autocomplete="off"
                  />
                  <button class="btn btn--secondary" @click="handleSaveApiKey">
                    {{ t('settings.aiServices.save') }}
                  </button>
                </div>
                <p class="form-hint">{{ t('settings.aiServices.apiKeyHint') }}</p>
              </div>

              <!-- PaddleOCR 专属配置 -->
              <template v-if="selectedPaddleConfig">
                <!-- API 模式 -->
                <div class="form-group">
                  <label class="form-label">{{ t('settings.aiServices.apiMode') }}</label>
                  <select
                    class="form-select"
                    :value="selectedPaddleConfig.api_mode"
                    @change="handleApiModeChange"
                  >
                    <option value="job">{{ t('settings.aiServices.apiModes.job') }}</option>
                    <option value="sync">{{ t('settings.aiServices.apiModes.sync') }}</option>
                  </select>
                </div>

                <!-- 识别选项 -->
                <div class="form-group">
                  <label class="form-label">{{ t('settings.aiServices.recognitionOptions') }}</label>
                  <div class="checkbox-list">
                    <label class="checkbox-row">
                      <input
                        type="checkbox"
                        :checked="selectedPaddleConfig.options.use_doc_orientation_classify"
                        @change="handleOptionChange('use_doc_orientation_classify', $event)"
                      />
                      <span class="checkbox-label">{{ t('settings.aiServices.options.docOrientation') }}</span>
                    </label>
                    <label class="checkbox-row">
                      <input
                        type="checkbox"
                        :checked="selectedPaddleConfig.options.use_doc_unwarping"
                        @change="handleOptionChange('use_doc_unwarping', $event)"
                      />
                      <span class="checkbox-label">{{ t('settings.aiServices.options.docUnwarping') }}</span>
                    </label>
                    <label class="checkbox-row">
                      <input
                        type="checkbox"
                        :checked="selectedPaddleConfig.options.use_chart_recognition"
                        @change="handleOptionChange('use_chart_recognition', $event)"
                      />
                      <span class="checkbox-label">{{ t('settings.aiServices.options.chartRecognition') }}</span>
                    </label>
                  </div>
                </div>
              </template>

              <!-- 模型列表 -->
              <div v-if="provider.models.length > 0" class="form-group">
                <label class="form-label">{{ t('settings.aiServices.models') }}</label>
                <div class="model-list">
                  <div
                    v-for="model in provider.models"
                    :key="model.id"
                    class="model-row"
                    :class="{ 'model-row--active': activeOcrProviderId === provider.id && provider.active_model_id === model.id }"
                  >
                    <span class="model-row__name">{{ model.name }}</span>
                    <button
                      v-if="!(activeOcrProviderId === provider.id && provider.active_model_id === model.id)"
                      class="btn btn--ghost btn--sm"
                      @click="handleSetActiveModel(provider, model.id)"
                    >
                      {{ t('settings.aiServices.setActive') }}
                    </button>
                    <span v-else class="model-row__active-mark">
                      <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M5 13l4 4L19 7" />
                      </svg>
                    </span>
                  </div>
                </div>
              </div>

              <!-- 操作按钮 -->
              <div class="provider-card__actions">
                <button
                  v-if="activeOcrProviderId !== provider.id"
                  class="btn btn--primary btn--sm"
                  @click="handleSetActive(provider)"
                >
                  {{ t('settings.aiServices.setAsActive') }}
                </button>
                <span v-if="isSaving" class="saving-hint">{{ t('settings.aiServices.saving') }}</span>
              </div>
            </div>
          </Transition>
        </div>
      </div>

      <!-- 空状态 -->
      <div v-else class="empty-state">
        <p class="empty-state__text">{{ t('settings.aiServices.noProviders') }}</p>
      </div>
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

/* ── 提供商列表 ─────────────────────────────────────────────────────── */
.provider-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  max-width: 640px;
}

.provider-card {
  border: 1px solid var(--fluen-hairline);
  border-radius: 12px;
  background: var(--fluen-surface);
  overflow: hidden;
  transition: border-color 0.2s ease;
}

.provider-card--active {
  border-color: var(--fluen-accent);
}

.provider-card__header {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 12px 16px;
  cursor: pointer;
  transition: background 0.15s ease;
}

.provider-card__header:hover {
  background: var(--fluen-hover);
}

.provider-card__info {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex: 1;
}

.provider-card__name {
  font-family: var(--fluen-font-sans);
  font-size: 0.9rem;
  font-weight: 500;
  color: var(--fluen-ink);
}

.provider-card__badge {
  padding: 2px 8px;
  border-radius: 9999px;
  background: var(--fluen-info-bg);
  color: var(--fluen-accent);
  font-size: 0.68rem;
  font-weight: 600;
}

.provider-card__meta {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  font-size: 0.75rem;
  color: var(--fluen-stone);
}

.provider-card__key-status {
  color: var(--fluen-stone);
}

.provider-card__key-status--ok {
  color: var(--fluen-success-text);
}

.provider-card__chevron {
  flex-shrink: 0;
  color: var(--fluen-stone);
  transition: transform 0.2s ease;
}

.provider-card__chevron--open {
  transform: rotate(180deg);
}

/* ── 展开编辑区 ─────────────────────────────────────────────────────── */
.provider-card__body {
  padding: 0 16px 16px;
  border-top: 1px solid var(--fluen-hairline);
}

/* ── 表单通用 ───────────────────────────────────────────────────────── */
.form-group {
  margin-top: 1rem;
}

.form-label {
  display: block;
  margin-bottom: 0.4rem;
  font-family: var(--fluen-font-sans);
  font-size: 0.78rem;
  font-weight: 500;
  color: var(--fluen-steel);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.form-input,
.form-select {
  width: 100%;
  padding: 10px 14px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 0.9rem;
  outline: none;
  box-sizing: border-box;
  transition: border-color 0.2s ease;
}

.form-input:focus,
.form-select:focus {
  border-color: var(--fluen-accent);
}

.form-input::placeholder {
  color: var(--fluen-stone);
}

.form-hint {
  margin: 0.4rem 0 0;
  font-size: 0.72rem;
  color: var(--fluen-stone);
}

.input-row {
  display: flex;
  gap: 0.5rem;
}

.input-row .form-input {
  flex: 1;
}

/* ── 复选框列表 ─────────────────────────────────────────────────────── */
.checkbox-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.checkbox-row {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  cursor: pointer;
  padding: 6px 0;
}

.checkbox-row input[type="checkbox"] {
  width: 16px;
  height: 16px;
  accent-color: var(--fluen-accent);
  cursor: pointer;
}

.checkbox-label {
  font-family: var(--fluen-font-sans);
  font-size: 0.85rem;
  color: var(--fluen-charcoal);
}

/* ── 模型列表 ───────────────────────────────────────────────────────── */
.model-list {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.model-row {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 8px 12px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-canvas);
  transition: border-color 0.15s ease;
}

.model-row--active {
  border-color: var(--fluen-accent);
  background: var(--fluen-info-bg);
}

.model-row__name {
  flex: 1;
  font-size: 0.85rem;
  font-weight: 500;
  color: var(--fluen-charcoal);
}

.model-row__active-mark {
  display: flex;
  align-items: center;
  color: var(--fluen-accent);
}

/* ── 操作按钮行 ─────────────────────────────────────────────────────── */
.provider-card__actions {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  margin-top: 1rem;
}

.saving-hint {
  font-size: 0.75rem;
  color: var(--fluen-stone);
}

/* ── 按钮 ───────────────────────────────────────────────────────────── */
.btn {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  padding: 8px 16px;
  border: none;
  border-radius: 9999px;
  font-family: var(--fluen-font-sans);
  font-size: 0.82rem;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.15s ease, opacity 0.15s ease;
  white-space: nowrap;
}

.btn--sm {
  padding: 6px 12px;
  font-size: 0.78rem;
}

.btn--primary {
  background: var(--fluen-primary);
  color: var(--fluen-on-primary);
}

.btn--primary:hover {
  background: var(--fluen-primary-soft);
}

.btn--secondary {
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  border: 1px solid var(--fluen-hairline);
}

.btn--secondary:hover {
  border-color: var(--fluen-stone);
}

.btn--ghost {
  background: transparent;
  color: var(--fluen-steel);
  border: 1px solid var(--fluen-hairline);
}

.btn--ghost:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

/* ── 空状态 ─────────────────────────────────────────────────────────── */
.empty-state {
  padding: 2rem 0;
  text-align: center;
}

.empty-state__text {
  font-size: 0.85rem;
  color: var(--fluen-stone);
}

/* ── 展开动画 ───────────────────────────────────────────────────────── */
.expand-enter-active,
.expand-leave-active {
  transition: opacity 0.2s ease, max-height 0.2s ease;
  overflow: hidden;
}

.expand-enter-from,
.expand-leave-to {
  opacity: 0;
  max-height: 0;
}

.expand-enter-to,
.expand-leave-from {
  max-height: 600px;
}
</style>
