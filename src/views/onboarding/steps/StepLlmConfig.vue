<script setup lang="ts">
/**
 * StepLlmConfig — 步骤 2：配置 LLM 服务提供商。
 *
 * 提供下拉列表选择提供商（预设 / OpenAI 兼容自定义），
 * 选中后下方展示对应表单：
 * - 预设提供商（DeepSeek / Xiaomi MiMo 等）：选择模型 + 填写 API Key
 * - OpenAI 兼容：填写名称、Base URL、API Key、模型 ID 与名称
 *
 * 表单状态为组件内部 ref，从 props 初始化；
 * 任意字段变化时构建 ProviderConfig 并向上 emit。
 */
import { computed, ref, watch } from 'vue';
import ObDropdown from '../components/ObDropdown.vue';
import { PROVIDER_PRESETS } from '../constants';
import { useI18n } from '../../../i18n';
import type { ProviderPreset } from '../types';
import type {
  ModelCapabilities,
  ModelConfig,
  ProviderConfig,
} from '../../../types/llm';

const props = defineProps<{
  provider: ProviderConfig | null;
  activeModelId: string | null;
}>();

const emit = defineEmits<{
  (e: 'update:provider', value: ProviderConfig | null): void;
  (e: 'update:activeModelId', value: string | null): void;
}>();

const { t } = useI18n();

/* ── 下拉选项 ────────────────────────────────────────────────────────── */
const dropdownOptions = computed(() =>
  PROVIDER_PRESETS.map((p) => ({
    value: p.id,
    label: t(`onboarding.steps.llmConfig.providerPresets.${p.id === 'openai-compatible' ? 'openaiCompatible' : p.id}`),
    icon: p.icon,
  })),
);

/* ── 从 props 推断当前选中的预设 ─────────────────────────────────────── */
function inferPresetId(): string {
  if (!props.provider) return '';
  // 匹配预设提供商 ID
  const matched = PROVIDER_PRESETS.find(
    (p) => p.isPreset && p.id === props.provider!.id,
  );
  return matched ? matched.id : 'openai-compatible';
}

const selectedPresetId = ref<string>(inferPresetId());

/** 当前选中的预设对象。 */
const selectedPreset = computed<ProviderPreset | undefined>(() =>
  PROVIDER_PRESETS.find((p) => p.id === selectedPresetId.value),
);

/* ── 预设表单状态（通用：DeepSeek / MiMo 等） ─────────────────────────── */
const presetModelId = ref<string>(
  props.provider && selectedPresetId.value && selectedPresetId.value !== 'openai-compatible'
    ? (props.activeModelId ?? selectedPreset.value?.models[0]?.id ?? '')
    : '',
);
const presetApiKey = ref<string>(
  props.provider && selectedPresetId.value && selectedPresetId.value !== 'openai-compatible'
    ? (props.provider.api_key ?? '')
    : '',
);

/* ── OpenAI 兼容表单状态 ─────────────────────────────────────────────── */
/** 判断 provider 是否为非预设的自定义提供商。 */
function isCustomProvider(provider: ProviderConfig | null): boolean {
  if (!provider) return false;
  return !PROVIDER_PRESETS.some((p) => p.isPreset && p.id === provider.id);
}
const isCustom = isCustomProvider(props.provider);
const customName = ref<string>(isCustom ? props.provider!.name : '');
const customBaseUrl = ref<string>(isCustom ? (props.provider!.openai_base_url ?? '') : '');
const customApiKey = ref<string>(isCustom ? (props.provider!.api_key ?? '') : '');
const customModelId = ref<string>(
  isCustom && props.provider!.models.length > 0
    ? props.provider!.models[0].id
    : '',
);
const customModelName = ref<string>(
  isCustom && props.provider!.models.length > 0
    ? props.provider!.models[0].name
    : '',
);

/* ── 切换预设时自动选中第一个模型 ────────────────────────────────────── */
watch(selectedPresetId, (newId) => {
  if (newId === 'openai-compatible' || !newId) return;
  const preset = PROVIDER_PRESETS.find((p) => p.id === newId);
  if (preset && preset.models.length > 0) {
    presetModelId.value = preset.models[0].id;
  }
});

/* ── 工具函数 ────────────────────────────────────────────────────────── */

/** 将名称转换为 slug 作为 provider ID。 */
function slugify(text: string): string {
  return text
    .toLowerCase()
    .trim()
    .replace(/[^\w\s-]/g, '')
    .replace(/[\s_-]+/g, '-')
    .replace(/^-+|-+$/g, '');
}

/** 从预设模型构建能力标志。 */
function presetModelCapabilities(model: {
  thinking: boolean;
  vision: boolean;
  audio: boolean;
  video: boolean;
}): ModelCapabilities {
  return {
    thinking: model.thinking,
    vision: model.vision,
    audio: model.audio,
    video: model.video,
    tool_calling: true,
    streaming: true,
  };
}

/** 从预设构建 ProviderConfig。 */
function buildPresetProvider(
  preset: ProviderPreset,
  apiKey: string,
): ProviderConfig {
  const models: ModelConfig[] = preset.models.map((m) => ({
    id: m.id,
    name: m.name,
    capabilities: presetModelCapabilities(m),
    max_output_tokens: m.maxOutputTokens,
    context_window: m.contextWindow,
    description: null,
    enabled: true,
  }));

  return {
    id: preset.id,
    name: t(`onboarding.steps.llmConfig.providerPresets.${preset.id === 'openai-compatible' ? 'openaiCompatible' : preset.id}`),
    provider_type: 'custom',
    openai_base_url: preset.openaiBaseUrl,
    anthropic_base_url: null,
    api_key: apiKey || null,
    default_style: preset.defaultStyle,
    extra_headers: {},
    enabled: true,
    models,
    created_at: null,
    updated_at: null,
  };
}

/** 构建 OpenAI 兼容自定义 ProviderConfig。 */
function buildCustomProvider(): ProviderConfig {
  const id = slugify(customName.value) || 'custom-provider';
  return {
    id,
    name: customName.value || 'Custom Provider',
    provider_type: 'custom',
    openai_base_url: customBaseUrl.value || null,
    anthropic_base_url: null,
    api_key: customApiKey.value || null,
    default_style: 'OpenAI',
    extra_headers: {},
    enabled: true,
    models: customModelId.value
      ? [{
          id: customModelId.value,
          name: customModelName.value || customModelId.value,
          capabilities: presetModelCapabilities({ thinking: false, vision: false, audio: false, video: false }),
          max_output_tokens: null,
          context_window: null,
          description: null,
          enabled: true,
        }]
      : [],
    created_at: null,
    updated_at: null,
  };
}

/* ── 状态同步：表单变化时 emit ───────────────────────────────────────── */
function syncEmit(): void {
  if (!selectedPresetId.value) {
    emit('update:provider', null);
    emit('update:activeModelId', null);
    return;
  }

  const preset = PROVIDER_PRESETS.find((p) => p.id === selectedPresetId.value);
  if (!preset) return;

  if (preset.isPreset) {
    emit('update:provider', buildPresetProvider(preset, presetApiKey.value));
    emit('update:activeModelId', presetModelId.value);
  } else {
    emit('update:provider', buildCustomProvider());
    emit('update:activeModelId', customModelId.value || null);
  }
}

// 监听所有表单字段变化
watch(
  [selectedPresetId, presetModelId, presetApiKey, customName, customBaseUrl, customApiKey, customModelId, customModelName],
  syncEmit,
  { immediate: true },
);
</script>

<template>
  <div class="step">
    <h3 class="step__title">{{ t('onboarding.steps.llmConfig.title') }}</h3>
    <p class="step__desc">{{ t('onboarding.steps.llmConfig.description') }}</p>

    <!-- 提供商下拉选择 -->
    <div class="form-group">
      <label class="form-label">{{ t('onboarding.steps.llmConfig.provider') }}</label>
      <ObDropdown
        :model-value="selectedPresetId"
        :options="dropdownOptions"
        :placeholder="t('onboarding.steps.llmConfig.providerPlaceholder')"
        @update:model-value="selectedPresetId = $event"
      />
    </div>

    <!-- ── 预设提供商表单（通用） ───────────────────────────────────── -->
    <div v-if="selectedPreset?.isPreset" class="provider-form">
      <!-- URL 信息展示（只读，带提供商图标） -->
      <div class="form-group">
        <label class="form-label">{{ t('onboarding.steps.llmConfig.apiAddress') }}</label>
        <div class="readonly-field">
          <svg
            v-if="selectedPreset.icon"
            class="readonly-field__icon"
            viewBox="0 0 24 24"
            width="16"
            height="16"
            v-html="selectedPreset.icon"
          />
          <span>{{ selectedPreset.openaiBaseUrl }}</span>
        </div>
      </div>

      <!-- 模型选择 -->
      <div class="form-group">
        <label class="form-label">{{ t('onboarding.steps.llmConfig.model') }}</label>
        <div class="model-grid">
          <button
            v-for="model in selectedPreset.models"
            :key="model.id"
            class="model-card"
            :class="{ 'model-card--active': presetModelId === model.id }"
            type="button"
            @click="presetModelId = model.id"
          >
            <div class="model-card__header">
              <span class="model-card__name">{{ model.name }}</span>
              <span class="model-card__check" :class="{ 'model-card__check--visible': presetModelId === model.id }">
                <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M5 13l4 4L19 7" />
                </svg>
              </span>
            </div>
            <div class="model-card__meta">
              <span v-if="model.contextWindow >= 1_000_000" class="model-tag">{{ t('onboarding.steps.llmConfig.modelTags.context1M') }}</span>
              <span v-if="model.thinking" class="model-tag model-tag--accent">{{ t('onboarding.steps.llmConfig.modelTags.thinking') }}</span>
              <span v-if="model.vision || model.audio || model.video" class="model-tag model-tag--accent">{{ t('onboarding.steps.llmConfig.modelTags.multimodal') }}</span>
              <span v-if="model.maxOutputTokens >= 131_072" class="model-tag">{{ t('onboarding.steps.llmConfig.modelTags.maxOutput128K') }}</span>
            </div>
          </button>
        </div>
      </div>

      <!-- API Key -->
      <div class="form-group">
        <label class="form-label">{{ t('onboarding.steps.llmConfig.apiKey') }}</label>
        <input
          v-model="presetApiKey"
          class="form-input"
          type="password"
          :placeholder="t('onboarding.steps.llmConfig.placeholders.apiKey')"
          autocomplete="off"
        />
      </div>
    </div>

    <!-- ── OpenAI 兼容自定义表单 ─────────────────────────────────────── -->
    <div v-else-if="selectedPresetId === 'openai-compatible'" class="provider-form">
      <div class="form-group">
        <label class="form-label">{{ t('onboarding.steps.llmConfig.providerName') }}</label>
        <input
          v-model="customName"
          class="form-input"
          type="text"
          :placeholder="t('onboarding.steps.llmConfig.placeholders.providerName')"
        />
      </div>

      <div class="form-group">
        <label class="form-label">{{ t('onboarding.steps.llmConfig.baseUrl') }}</label>
        <input
          v-model="customBaseUrl"
          class="form-input"
          type="text"
          :placeholder="t('onboarding.steps.llmConfig.placeholders.baseUrl')"
        />
      </div>

      <div class="form-group">
        <label class="form-label">{{ t('onboarding.steps.llmConfig.apiKey') }}</label>
        <input
          v-model="customApiKey"
          class="form-input"
          type="password"
          :placeholder="t('onboarding.steps.llmConfig.placeholders.apiKey')"
          autocomplete="off"
        />
      </div>

      <div class="form-group">
        <label class="form-label">{{ t('onboarding.steps.llmConfig.modelId') }}</label>
        <input
          v-model="customModelId"
          class="form-input"
          type="text"
          :placeholder="t('onboarding.steps.llmConfig.placeholders.modelId')"
        />
      </div>

      <div class="form-group">
        <label class="form-label">{{ t('onboarding.steps.llmConfig.modelDisplayName') }}</label>
        <input
          v-model="customModelName"
          class="form-input"
          type="text"
          :placeholder="t('onboarding.steps.llmConfig.placeholders.modelDisplayName')"
        />
      </div>
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
  margin: 0 0 1.25rem;
  font-family: var(--fluen-font-sans);
  font-size: 0.85rem;
  color: var(--fluen-stone);
}

/* ── 表单通用 ────────────────────────────────────────────────────────── */
.form-group {
  margin-bottom: 1rem;
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
  background: var(--fluen-hover);
  color: var(--fluen-on-dark);
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
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 10px 14px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-hover);
  color: var(--fluen-steel);
  font-family: var(--fluen-font-sans);
  font-size: 0.85rem;
  user-select: text;
}

.readonly-field__icon {
  flex-shrink: 0;
  color: var(--fluen-slate);
}

/* ── 提供商表单容器 ──────────────────────────────────────────────────── */
.provider-form {
  margin-top: 1.25rem;
  padding-top: 1.25rem;
  border-top: 1px solid var(--fluen-hairline);
}

/* ── 模型选择卡片 ────────────────────────────────────────────────────── */
.model-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0.6rem;
}

.model-card {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  padding: 0.75rem;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-hover);
  cursor: pointer;
  transition: all 0.2s ease;
  text-align: left;
  font-family: var(--fluen-font-sans);
}

.model-card:hover {
  border-color: var(--fluen-muted);
}

.model-card--active {
  border-color: var(--fluen-accent);
  background: var(--fluen-info-bg);
}

.model-card__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.model-card__name {
  font-size: 0.82rem;
  font-weight: 500;
  color: var(--fluen-charcoal);
}

.model-card__check {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: var(--fluen-accent);
  color: var(--fluen-on-accent);
  opacity: 0;
  transition: opacity 0.2s ease;
}

.model-card__check--visible {
  opacity: 1;
}

.model-card__meta {
  display: flex;
  flex-wrap: wrap;
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
</style>
