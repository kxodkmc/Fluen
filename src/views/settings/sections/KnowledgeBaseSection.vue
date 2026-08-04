<script setup lang="ts">
/**
 * KnowledgeBaseSection — 知识库配置分区。
 *
 * 整合三项配置：
 * 1. Embedding 开关（默认开启，禁用时降级为纯关键词检索）
 * 2. Embedding 模型选择（内置免费 / 用户自定义）
 * 3. 知识库构建模型选择（默认跟随全局激活项）
 *
 * 状态由 `useLlmSettings` 统一管理，与 `LlmConfigSection` 共享同一份 LlmConfig。
 */
import { computed } from 'vue';
import { useI18n } from '../../../i18n';
import { useLlmSettings } from '../composables/useLlmSettings';
import type { SceneModelRef } from '../../../types/llm';

const { t } = useI18n();
const {
  providers,
  activeProviderId,
  activeModelId,
  sceneModels,
  embeddingConfig,
  setSceneModel,
  clearSceneModel,
  setEmbeddingEnabled,
  setEmbeddingModel,
} = useLlmSettings();

/* ── 特殊值 ── */

/** 表示使用全局激活项（不设置场景专用模型）。 */
const USE_GLOBAL = '__global__';
/** 表示使用内置免费 Embedding 提供商。 */
const USE_BUILTIN = '__builtin__';

/* ── 通用：可用模型选项 ── */

/** 单个可用 provider+model 组合。 */
interface ModelOption {
  value: string;
  label: string;
}

/** 所有可用 provider+model 组合（仅含 enabled 的 provider 与 model）。 */
const modelOptions = computed<ModelOption[]>(() => {
  const opts: ModelOption[] = [];
  for (const provider of providers.value) {
    if (!provider.enabled) continue;
    for (const model of provider.models) {
      if (!model.enabled) continue;
      opts.push({
        value: `${provider.id}::${model.id}`,
        label: `${provider.name} / ${model.name}`,
      });
    }
  }
  return opts;
});

/** 是否有任何可用 provider+model 组合。 */
const hasOptions = computed<boolean>(() => modelOptions.value.length > 0);

/* ── 知识库构建模型 ── */

/** 「使用全局激活项」选项的显示文本（附带当前全局激活项信息）。 */
const globalLabel = computed<string>(() => {
  const base = t('settings.llmConfig.knowledgeBase.useGlobal');
  const provider = providers.value.find((p) => p.id === activeProviderId.value);
  const model = provider?.models.find((m) => m.id === activeModelId.value);
  if (!provider || !model) return base;
  return `${base}（${provider.name} / ${model.name}）`;
});

/** 当前知识库构建场景下拉的值。 */
const knowledgeBuildValue = computed<string>(() => {
  const ref = sceneModels.value?.knowledge_build ?? null;
  if (!ref) return USE_GLOBAL;
  return `${ref.provider_id}::${ref.model_id}`;
});

/** 知识库构建模型下拉变更处理。 */
function onKnowledgeBuildChange(event: Event): Promise<void> {
  const target = event.target as HTMLSelectElement;
  const value = target.value;
  if (value === USE_GLOBAL) {
    return clearSceneModel('knowledge_build');
  }
  const sepIdx = value.indexOf('::');
  if (sepIdx < 0) return Promise.resolve();
  const ref: SceneModelRef = {
    provider_id: value.slice(0, sepIdx),
    model_id: value.slice(sepIdx + 2),
  };
  return setSceneModel('knowledge_build', ref);
}

/* ── Embedding 配置 ── */

/** Embedding 是否启用。 */
const embeddingEnabled = computed<boolean>(() => embeddingConfig.value.enabled);

/** 当前 Embedding 模型下拉的值。 */
const embeddingModelValue = computed<string>(() => {
  const ref = embeddingConfig.value.model_ref ?? null;
  if (!ref) return USE_BUILTIN;
  return `${ref.provider_id}::${ref.model_id}`;
});

/** 「使用内置免费模型」选项的显示文本。 */
const builtinLabel = computed<string>(() =>
  t('settings.llmConfig.knowledgeBase.builtinEmbedding'),
);

/** Embedding 启用开关变更处理。 */
function onEmbeddingToggleChange(event: Event): Promise<void> {
  const target = event.target as HTMLInputElement;
  return setEmbeddingEnabled(target.checked);
}

/** Embedding 模型下拉变更处理。 */
function onEmbeddingModelChange(event: Event): Promise<void> {
  const target = event.target as HTMLSelectElement;
  const value = target.value;
  if (value === USE_BUILTIN) {
    return setEmbeddingModel(null);
  }
  const sepIdx = value.indexOf('::');
  if (sepIdx < 0) return Promise.resolve();
  const ref: SceneModelRef = {
    provider_id: value.slice(0, sepIdx),
    model_id: value.slice(sepIdx + 2),
  };
  return setEmbeddingModel(ref);
}
</script>

<template>
  <div class="kb-config">
    <h3 class="sub-section__title">{{ t('settings.llmConfig.knowledgeBase.title') }}</h3>
    <p class="sub-section__desc">{{ t('settings.llmConfig.knowledgeBase.description') }}</p>

    <template v-if="hasOptions">
      <!-- Embedding 启用开关 -->
      <div class="form-group">
        <label class="toggle-row">
          <span class="toggle-row__label">{{ t('settings.llmConfig.knowledgeBase.embeddingEnabled') }}</span>
          <input
            type="checkbox"
            class="toggle-checkbox"
            :checked="embeddingEnabled"
            @change="onEmbeddingToggleChange"
          />
        </label>
        <p class="form-hint">{{ t('settings.llmConfig.knowledgeBase.embeddingEnabledHint') }}</p>
      </div>

      <!-- Embedding 模型选择 -->
      <div v-if="embeddingEnabled" class="form-group">
        <label class="form-label" for="kb-embedding-model">
          {{ t('settings.llmConfig.knowledgeBase.embeddingModel') }}
        </label>
        <select
          id="kb-embedding-model"
          class="form-select"
          :value="embeddingModelValue"
          @change="onEmbeddingModelChange"
        >
          <option :value="USE_BUILTIN">{{ builtinLabel }}</option>
          <option
            v-for="opt in modelOptions"
            :key="opt.value"
            :value="opt.value"
          >
            {{ opt.label }}
          </option>
        </select>
        <p class="form-hint">{{ t('settings.llmConfig.knowledgeBase.embeddingModelHint') }}</p>
      </div>

      <!-- 知识库构建模型选择 -->
      <div class="form-group">
        <label class="form-label" for="kb-build-model">
          {{ t('settings.llmConfig.knowledgeBase.buildModel') }}
        </label>
        <select
          id="kb-build-model"
          class="form-select"
          :value="knowledgeBuildValue"
          @change="onKnowledgeBuildChange"
        >
          <option :value="USE_GLOBAL">{{ globalLabel }}</option>
          <option
            v-for="opt in modelOptions"
            :key="opt.value"
            :value="opt.value"
          >
            {{ opt.label }}
          </option>
        </select>
        <p class="form-hint">{{ t('settings.llmConfig.knowledgeBase.buildModelHint') }}</p>
      </div>
    </template>

    <div v-else class="empty-state">
      <p class="empty-state__text">{{ t('settings.llmConfig.knowledgeBase.emptyHint') }}</p>
    </div>
  </div>
</template>

<style scoped>
.kb-config {
  border-top: 1px solid var(--fluen-hairline);
  padding-top: 1.25rem;
  margin-top: 1.5rem;
  max-width: 640px;
}

.sub-section__title {
  margin: 0 0 0.5rem;
  font-family: var(--fluen-font-sans);
  font-size: 0.95rem;
  font-weight: 600;
  color: var(--fluen-ink);
}

.sub-section__desc {
  margin: 0 0 0.75rem;
  font-family: var(--fluen-font-sans);
  font-size: 0.8rem;
  color: var(--fluen-stone);
  line-height: 1.4;
}

.form-group {
  margin-top: 0.5rem;
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

.toggle-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  cursor: pointer;
}

.toggle-row__label {
  font-family: var(--fluen-font-sans);
  font-size: 0.9rem;
  font-weight: 500;
  color: var(--fluen-ink);
}

.toggle-checkbox {
  width: 18px;
  height: 18px;
  cursor: pointer;
  accent-color: var(--fluen-accent);
}

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
  cursor: pointer;
  appearance: none;
  background-image: url("data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg' width='16' height='16' viewBox='0 0 24 24' fill='none' stroke='%238e8e93' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='m6 9 6 6 6-6'/%3E%3C/svg%3E");
  background-repeat: no-repeat;
  background-position: right 12px center;
  padding-right: 36px;
}

.form-select:focus {
  border-color: var(--fluen-accent);
}

.form-hint {
  margin: 0.4rem 0 0;
  font-size: 0.72rem;
  color: var(--fluen-stone);
  line-height: 1.4;
}

.empty-state {
  padding: 1.25rem;
  border: 1px solid var(--fluen-hairline);
  border-radius: 12px;
  background: var(--fluen-surface);
  text-align: center;
}

.empty-state__text {
  margin: 0;
  font-size: 0.85rem;
  color: var(--fluen-stone);
}
</style>
