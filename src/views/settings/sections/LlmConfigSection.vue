<script setup lang="ts">
/**
 * LlmConfigSection — LLM 配置设置分区。
 *
 * 功能：
 *   - 列出已配置的提供商（含激活标记）
 *   - 添加新提供商（预设 / OpenAI 兼容自定义）
 *   - 编辑提供商 API Key
 *   - 设置激活的提供商与模型
 *   - 删除提供商
 *
 * 状态管理委托给 useLlmSettings composable，组件仅负责 UI。
 */
import { ref, onMounted } from 'vue';
import { useI18n } from '../../../i18n';
import { useLlmSettings } from '../composables/useLlmSettings';
import type { ProviderConfig } from '../../../types/llm';
import KnowledgeBaseSection from './KnowledgeBaseSection.vue';

const { t } = useI18n();
const {
  providers,
  selectedProvider,
  selectedProviderId,
  activeProviderId,
  activeModelId,
  isAdding,
  isLoading,
  selectedPresetId,
  selectedPreset,
  presets,
  load,
  startAdding,
  cancelAdding,
  addPresetProvider,
  addCustomProvider,
  updateApiKey,
  setActive,
  removeProvider,
  selectProvider,
  getPresetLabel,
} = useLlmSettings();

onMounted(() => {
  load();
});

/* ── 添加表单状态 ───────────────────────────────────────────────────── */
const presetApiKey = ref('');
const presetModelId = ref('');
const customName = ref('');
const customBaseUrl = ref('');
const customApiKey = ref('');
const customModelId = ref('');
const customModelName = ref('');

/** 重置表单。 */
function resetForm(): void {
  presetApiKey.value = '';
  presetModelId.value = '';
  customName.value = '';
  customBaseUrl.value = '';
  customApiKey.value = '';
  customModelId.value = '';
  customModelName.value = '';
}

/** 选择预设时自动选中第一个模型。 */
function onPresetSelect(presetId: string): void {
  selectedPresetId.value = presetId;
  const preset = presets.find((p) => p.id === presetId);
  if (preset && preset.models.length > 0) {
    presetModelId.value = preset.models[0].id;
  }
}

/** 确认添加。 */
async function handleAdd(): Promise<void> {
  const preset = selectedPreset.value;
  if (!preset) return;

  if (preset.isPreset) {
    await addPresetProvider(preset, presetApiKey.value, presetModelId.value);
  } else {
    await addCustomProvider({
      name: customName.value,
      baseUrl: customBaseUrl.value,
      apiKey: customApiKey.value,
      modelId: customModelId.value,
      modelName: customModelName.value,
    });
  }
  resetForm();
}

/** 取消添加。 */
function handleCancelAdd(): void {
  cancelAdding();
  resetForm();
}

/* ── 编辑面板状态 ───────────────────────────────────────────────────── */
const editApiKey = ref('');

/** 选中提供商时同步 API Key 到编辑框。 */
function onExpand(provider: ProviderConfig): void {
  selectProvider(provider.id);
  editApiKey.value = provider.api_key ?? '';
}

/** 保存 API Key。 */
async function handleSaveApiKey(): Promise<void> {
  if (!selectedProvider.value) return;
  await updateApiKey(selectedProvider.value.id, editApiKey.value);
}

/** 设为激活提供商。 */
async function handleSetActive(provider: ProviderConfig, modelId: string | null): Promise<void> {
  await setActive(provider.id, modelId);
}

/** 删除提供商。 */
async function handleRemove(provider: ProviderConfig): Promise<void> {
  await removeProvider(provider.id);
}

/** 获取提供商的已配置模型数量。 */
function getModelCount(provider: ProviderConfig): number {
  return provider.models.length;
}

/** 判断提供商是否已配置 API Key。 */
function hasApiKey(provider: ProviderConfig): boolean {
  return !!provider.api_key;
}
</script>

<template>
  <div class="section">
    <h2 class="section__title">{{ t('settings.sections.llmConfig') }}</h2>
    <p class="section__desc">{{ t('settings.llmConfig.description') }}</p>

    <!-- 加载中 -->
    <div v-if="isLoading" class="section__loading">
      {{ t('settings.llmConfig.loading') }}
    </div>

    <template v-else>
      <!-- ── 已配置的提供商列表 ─────────────────────────────────────── -->
      <div v-if="providers.length > 0" class="provider-list">
        <div
          v-for="provider in providers"
          :key="provider.id"
          class="provider-card"
          :class="{ 'provider-card--active': activeProviderId === provider.id }"
        >
          <!-- 卡片头部 -->
          <div class="provider-card__header" @click="onExpand(provider)">
            <div class="provider-card__info">
              <span class="provider-card__name">{{ provider.name }}</span>
              <span v-if="activeProviderId === provider.id" class="provider-card__badge">
                {{ t('settings.llmConfig.active') }}
              </span>
            </div>
            <div class="provider-card__meta">
              <span class="provider-card__models">
                {{ t('settings.llmConfig.modelCount', { count: getModelCount(provider) }) }}
              </span>
              <span
                class="provider-card__key-status"
                :class="{ 'provider-card__key-status--ok': hasApiKey(provider) }"
              >
                {{ hasApiKey(provider) ? t('settings.llmConfig.apiKeySet') : t('settings.llmConfig.apiKeyMissing') }}
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
              <!-- API Key 编辑 -->
              <div class="form-group">
                <label class="form-label">{{ t('settings.llmConfig.apiKey') }}</label>
                <div class="api-key-row">
                  <input
                    v-model="editApiKey"
                    class="form-input"
                    type="password"
                    :placeholder="t('settings.llmConfig.placeholders.apiKey')"
                    autocomplete="off"
                  />
                  <button class="btn btn--secondary" @click="handleSaveApiKey">
                    {{ t('settings.llmConfig.save') }}
                  </button>
                </div>
              </div>

              <!-- 模型列表 -->
              <div v-if="provider.models.length > 0" class="form-group">
                <label class="form-label">{{ t('settings.llmConfig.models') }}</label>
                <div class="model-list">
                  <div
                    v-for="model in provider.models"
                    :key="model.id"
                    class="model-row"
                    :class="{ 'model-row--active': activeProviderId === provider.id && activeModelId === model.id }"
                  >
                    <span class="model-row__name">{{ model.name }}</span>
                    <div class="model-row__tags">
                      <span v-if="model.context_window && model.context_window >= 1_000_000" class="model-tag">{{ t('onboarding.steps.llmConfig.modelTags.context1M') }}</span>
                      <span v-if="model.capabilities.thinking" class="model-tag model-tag--accent">{{ t('onboarding.steps.llmConfig.modelTags.thinking') }}</span>
                    </div>
                    <button
                      v-if="!(activeProviderId === provider.id && activeModelId === model.id)"
                      class="btn btn--ghost btn--sm"
                      @click="handleSetActive(provider, model.id)"
                    >
                      {{ t('settings.llmConfig.setActive') }}
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
                  v-if="activeProviderId !== provider.id"
                  class="btn btn--primary btn--sm"
                  @click="handleSetActive(provider, provider.models[0]?.id ?? null)"
                >
                  {{ t('settings.llmConfig.setAsActive') }}
                </button>
                <button class="btn btn--danger btn--sm" @click="handleRemove(provider)">
                  {{ t('settings.llmConfig.remove') }}
                </button>
              </div>
            </div>
          </Transition>
        </div>
      </div>

      <!-- 空状态 -->
      <div v-else class="empty-state">
        <p class="empty-state__text">{{ t('settings.llmConfig.noProviders') }}</p>
      </div>

      <!-- ── 知识库配置 ─────────────────────────────────────────────── -->
<KnowledgeBaseSection v-if="providers.length > 0" />

      <!-- ── 添加提供商 ─────────────────────────────────────────────── -->
      <div v-if="isAdding" class="add-form">
        <h3 class="add-form__title">{{ t('settings.llmConfig.addProvider') }}</h3>

        <!-- 预设选择 -->
        <div class="form-group">
          <label class="form-label">{{ t('settings.llmConfig.provider') }}</label>
          <div class="preset-grid">
            <button
              v-for="preset in presets"
              :key="preset.id"
              class="preset-card"
              :class="{ 'preset-card--selected': selectedPresetId === preset.id }"
              type="button"
              @click="onPresetSelect(preset.id)"
            >
              <span class="preset-card__name">{{ getPresetLabel(preset.id) }}</span>
            </button>
          </div>
        </div>

        <!-- 预设提供商表单 -->
        <div v-if="selectedPreset?.isPreset" class="preset-form">
          <div class="form-group">
            <label class="form-label">{{ t('settings.llmConfig.apiAddress') }}</label>
            <div class="readonly-field">{{ selectedPreset.openaiBaseUrl }}</div>
          </div>
          <!-- 模型自填（预设无内置模型，如 OpenRouter 聚合网关） -->
          <div v-if="selectedPreset.models.length === 0" class="form-group">
            <label class="form-label">{{ t('settings.llmConfig.modelId') }}</label>
            <input v-model="presetModelId" class="form-input" type="text" :placeholder="t('settings.llmConfig.placeholders.modelId')" />
          </div>
          <div v-if="selectedPreset.models.length > 0" class="form-group">
            <label class="form-label">{{ t('settings.llmConfig.model') }}</label>
            <div class="model-select">
              <button
                v-for="model in selectedPreset.models"
                :key="model.id"
                class="model-select__item"
                :class="{ 'model-select__item--active': presetModelId === model.id }"
                type="button"
                @click="presetModelId = model.id"
              >
                {{ model.name }}
              </button>
            </div>
          </div>
          <div class="form-group">
            <label class="form-label">{{ t('settings.llmConfig.apiKey') }}</label>
            <input
              v-model="presetApiKey"
              class="form-input"
              type="password"
              :placeholder="t('settings.llmConfig.placeholders.apiKey')"
              autocomplete="off"
            />
          </div>
        </div>

        <!-- 自定义提供商表单 -->
        <div v-else-if="selectedPresetId === 'openai-compatible'" class="preset-form">
          <div class="form-group">
            <label class="form-label">{{ t('settings.llmConfig.providerName') }}</label>
            <input v-model="customName" class="form-input" type="text" :placeholder="t('settings.llmConfig.placeholders.providerName')" />
          </div>
          <div class="form-group">
            <label class="form-label">{{ t('settings.llmConfig.baseUrl') }}</label>
            <input v-model="customBaseUrl" class="form-input" type="text" :placeholder="t('settings.llmConfig.placeholders.baseUrl')" />
          </div>
          <div class="form-group">
            <label class="form-label">{{ t('settings.llmConfig.apiKey') }}</label>
            <input v-model="customApiKey" class="form-input" type="password" :placeholder="t('settings.llmConfig.placeholders.apiKey')" autocomplete="off" />
          </div>
          <div class="form-group">
            <label class="form-label">{{ t('settings.llmConfig.modelId') }}</label>
            <input v-model="customModelId" class="form-input" type="text" :placeholder="t('settings.llmConfig.placeholders.modelId')" />
          </div>
          <div class="form-group">
            <label class="form-label">{{ t('settings.llmConfig.modelDisplayName') }}</label>
            <input v-model="customModelName" class="form-input" type="text" :placeholder="t('settings.llmConfig.placeholders.modelDisplayName')" />
          </div>
        </div>

        <!-- 添加表单操作 -->
        <div class="add-form__actions">
          <button class="btn btn--secondary" @click="handleCancelAdd">
            {{ t('settings.llmConfig.cancel') }}
          </button>
          <button
            class="btn btn--primary"
            :disabled="!selectedPresetId"
            @click="handleAdd"
          >
            {{ t('settings.llmConfig.confirmAdd') }}
          </button>
        </div>
      </div>

      <!-- 添加按钮 -->
      <button v-else class="add-btn" @click="startAdding">
        <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M12 5v14M5 12h14" />
        </svg>
        {{ t('settings.llmConfig.addProvider') }}
      </button>
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

/* ── 提供商列表 ─────────────────────────────────────────────────────── */
.provider-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  margin-bottom: 1rem;
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

.form-input {
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

.form-input:focus {
  border-color: var(--fluen-accent);
}

.form-input::placeholder {
  color: var(--fluen-stone);
}

.readonly-field {
  padding: 10px 14px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-hover);
  color: var(--fluen-steel);
  font-family: var(--fluen-font-sans);
  font-size: 0.85rem;
  user-select: text;
}

.api-key-row {
  display: flex;
  gap: 0.5rem;
}

.api-key-row .form-input {
  flex: 1;
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

.model-row__tags {
  display: flex;
  gap: 0.3rem;
}

.model-tag {
  padding: 2px 7px;
  border-radius: 4px;
  background: var(--fluen-hover);
  color: var(--fluen-steel);
  font-size: 0.68rem;
  font-weight: 500;
}

.model-tag--accent {
  background: var(--fluen-info-bg);
  color: var(--fluen-accent);
}

.model-row__active-mark {
  display: flex;
  align-items: center;
  color: var(--fluen-accent);
}

/* ── 操作按钮行 ─────────────────────────────────────────────────────── */
.provider-card__actions {
  display: flex;
  gap: 0.5rem;
  margin-top: 1rem;
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

.btn--danger {
  background: transparent;
  color: var(--fluen-error);
  border: 1px solid var(--fluen-error);
}

.btn--danger:hover {
  background: var(--fluen-error-bg);
}

.btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
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

/* ── 添加表单 ───────────────────────────────────────────────────────── */
.add-form {
  padding: 1.25rem;
  border: 1px solid var(--fluen-hairline);
  border-radius: 12px;
  background: var(--fluen-surface);
  max-width: 640px;
  margin-bottom: 1rem;
}

.add-form__title {
  margin: 0 0 1rem;
  font-family: var(--fluen-font-sans);
  font-size: 1rem;
  font-weight: 600;
  color: var(--fluen-ink);
}

.preset-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: 0.5rem;
}

.preset-card {
  padding: 10px 12px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-canvas);
  cursor: pointer;
  transition: all 0.15s ease;
  text-align: center;
}

.preset-card:hover {
  border-color: var(--fluen-stone);
}

.preset-card--selected {
  border-color: var(--fluen-accent);
  background: var(--fluen-info-bg);
}

.preset-card__name {
  font-family: var(--fluen-font-sans);
  font-size: 0.82rem;
  font-weight: 500;
  color: var(--fluen-charcoal);
}

.preset-form {
  margin-top: 0.5rem;
}

.model-select {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
}

.model-select__item {
  padding: 6px 12px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 9999px;
  background: var(--fluen-canvas);
  color: var(--fluen-steel);
  font-size: 0.8rem;
  cursor: pointer;
  transition: all 0.15s ease;
}

.model-select__item:hover {
  border-color: var(--fluen-stone);
}

.model-select__item--active {
  background: var(--fluen-primary);
  color: var(--fluen-on-primary);
  border-color: var(--fluen-primary);
}

.add-form__actions {
  display: flex;
  justify-content: flex-end;
  gap: 0.5rem;
  margin-top: 1.25rem;
}

/* ── 添加按钮 ───────────────────────────────────────────────────────── */
.add-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  padding: 8px 16px;
  border: 1px dashed var(--fluen-hairline);
  border-radius: 9999px;
  background: transparent;
  color: var(--fluen-steel);
  font-family: var(--fluen-font-sans);
  font-size: 0.82rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
}

.add-btn:hover {
  border-color: var(--fluen-accent);
  color: var(--fluen-accent);
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
  max-height: 500px;
}
</style>
