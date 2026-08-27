<script setup lang="ts">
/**
 * ProviderAddPanel — 添加供应商面板（右栏）。
 *
 * 支持从预设快速填充（名称 / Base URL / 内置模型），或完全自定义。
 * 表单内可直接增删模型；提交时统一交给 useLlmSettings.addProvider。
 */
import { ref, computed } from 'vue';
import { useI18n } from '../../../../i18n';
import { useLlmSettings } from '../../composables/useLlmSettings';
import type { ApiStyle, ModelConfig } from '../../../../types/llm';
import ModelListEditor from './ModelListEditor.vue';

const emit = defineEmits<{
  /** 取消添加。 */
  cancel: [];
  /** 添加成功（携带新供应商 ID）。 */
  added: [providerId: string];
}>();

const { t } = useI18n();
const { presets, getPresetLabel, addProvider, modelsFromPreset } = useLlmSettings();

/* ── 表单状态 ───────────────────────────────────────────────────────── */

/** 选中的预设 ID（'' 表示自定义）。 */
const presetId = ref('');
const name = ref('');
const style = ref<ApiStyle>('OpenAI');
const baseUrl = ref('');
const apiKey = ref('');
const draftModels = ref<ModelConfig[]>([]);

/** 是否选择了预设（非自定义）。 */
const hasPreset = computed(() => presetId.value !== '');

/** 至少需要一个模型才能提交。 */
const canSubmit = computed(
  () => name.value.trim().length > 0 && baseUrl.value.trim().length > 0 && draftModels.value.length > 0,
);

/** 选择预设时填充默认值；选择「自定义」时清空。 */
function onPresetSelect(id: string): void {
  presetId.value = id;
  if (id === '') {
    name.value = '';
    style.value = 'OpenAI';
    baseUrl.value = '';
    draftModels.value = [];
    return;
  }
  const preset = presets.find((p) => p.id === id);
  if (!preset) return;
  name.value = getPresetLabel(id);
  style.value = preset.defaultStyle;
  baseUrl.value = preset.openaiBaseUrl;
  draftModels.value = modelsFromPreset(preset);
}

/** 提交添加。 */
async function submit(): Promise<void> {
  if (!canSubmit.value) return;
  const id = await addProvider({
    presetId: hasPreset.value ? presetId.value : null,
    name: name.value.trim(),
    style: style.value,
    baseUrl: baseUrl.value.trim(),
    apiKey: apiKey.value.trim(),
    models: draftModels.value,
  });
  emit('added', id);
}
</script>

<template>
  <div class="add-panel">
    <header class="add-panel__header">
      <h3 class="add-panel__title">{{ t('settings.llmConfig.addProviderTitle') }}</h3>
      <p class="add-panel__desc">{{ t('settings.llmConfig.addProviderDesc') }}</p>
    </header>

    <!-- 预设选择 -->
    <div class="add-panel__field">
      <label class="add-panel__label">{{ t('settings.llmConfig.choosePreset') }}</label>
      <div class="preset-grid">
        <button
          v-for="preset in presets"
          :key="preset.id"
          class="preset-chip"
          :class="{ 'preset-chip--selected': presetId === preset.id }"
          type="button"
          @click="onPresetSelect(preset.id)"
        >
          {{ getPresetLabel(preset.id) }}
        </button>
        <button
          class="preset-chip"
          :class="{ 'preset-chip--selected': presetId === '' }"
          type="button"
          @click="onPresetSelect('')"
        >
          {{ t('settings.llmConfig.custom') }}
        </button>
      </div>
    </div>

    <!-- 名称 -->
    <div class="add-panel__field">
      <label class="add-panel__label">{{ t('settings.llmConfig.name') }}</label>
      <input v-model="name" class="form-input" type="text" :placeholder="t('settings.llmConfig.placeholders.providerName')" />
    </div>

    <!-- Base URL -->
    <div class="add-panel__field">
      <label class="add-panel__label">Base URL</label>
      <input v-model="baseUrl" class="form-input" type="text" :placeholder="t('settings.llmConfig.placeholders.baseUrl')" />
    </div>

    <!-- API Key -->
    <div class="add-panel__field">
      <label class="add-panel__label">{{ t('settings.llmConfig.apiKey') }}</label>
      <input v-model="apiKey" class="form-input" type="password" :placeholder="t('settings.llmConfig.placeholders.apiKey')" autocomplete="off" />
    </div>

    <!-- API 格式 -->
    <div class="add-panel__field">
      <label class="add-panel__label">{{ t('settings.llmConfig.apiFormat') }}</label>
      <select v-model="style" class="form-input form-input--select">
        <option value="OpenAI">{{ t('settings.llmConfig.styleOpenAI') }}</option>
        <option value="Anthropic">{{ t('settings.llmConfig.styleAnthropic') }}</option>
      </select>
    </div>

    <!-- 模型列表（草稿） -->
    <div class="add-panel__field">
      <label class="add-panel__label">{{ t('settings.llmConfig.models') }}</label>
      <ModelListEditor
        :models="draftModels"
        @save="(model, originalId) => {
          const idx = draftModels.findIndex((m) => m.id === originalId);
          if (idx >= 0) draftModels.splice(idx, 1, model);
          else draftModels.push(model);
        }"
        @remove="draftModels = draftModels.filter((m) => m.id !== $event)"
      />
    </div>

    <!-- 底部：提示 + 提交 -->
    <footer class="add-panel__footer">
      <span class="add-panel__hint">
        <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="10" />
          <path d="M12 8v4M12 16h.01" />
        </svg>
        {{ t('settings.llmConfig.addProviderHint') }}
      </span>
      <div class="add-panel__actions">
        <button class="btn btn--ghost" type="button" @click="emit('cancel')">
          {{ t('settings.llmConfig.cancel') }}
        </button>
        <button class="btn btn--primary" type="button" :disabled="!canSubmit" @click="submit">
          {{ t('settings.llmConfig.addProvider') }}
        </button>
      </div>
    </footer>
  </div>
</template>

<style scoped>
.add-panel {
  display: flex;
  flex-direction: column;
  gap: 1.05rem;
  padding: 1.25rem 1.5rem;
}

.add-panel__title {
  margin: 0 0 0.25rem;
  font-family: var(--fluen-font-sans);
  font-size: 1.05rem;
  font-weight: 600;
  color: var(--fluen-ink);
}

.add-panel__desc {
  margin: 0;
  font-size: 0.82rem;
  color: var(--fluen-stone);
}

.add-panel__field {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.add-panel__label {
  font-size: 0.78rem;
  font-weight: 500;
  color: var(--fluen-steel);
}

/* ── 预设选择 ───────────────────────────────────────────────────────── */
.preset-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 0.45rem;
}

.preset-chip {
  padding: 6px 14px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 9999px;
  background: var(--fluen-canvas);
  color: var(--fluen-steel);
  font-family: var(--fluen-font-sans);
  font-size: 0.8rem;
  cursor: pointer;
  transition: all 0.15s ease;
}

.preset-chip:hover {
  border-color: var(--fluen-stone);
  color: var(--fluen-ink);
}

.preset-chip--selected {
  border-color: var(--fluen-accent);
  background: var(--fluen-info-bg, var(--fluen-hover));
  color: var(--fluen-accent);
}

/* ── 底部 ───────────────────────────────────────────────────────────── */
.add-panel__footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  margin-top: 0.4rem;
  padding-top: 0.9rem;
  border-top: 1px solid var(--fluen-hairline);
}

.add-panel__hint {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  font-size: 0.78rem;
  color: var(--fluen-stone);
}

.add-panel__actions {
  display: flex;
  gap: 0.5rem;
}

/* ── 表单控件 ───────────────────────────────────────────────────────── */
.form-input {
  width: 100%;
  padding: 10px 14px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 10px;
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 0.88rem;
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

.form-input--select {
  cursor: pointer;
}

/* ── 按钮 ───────────────────────────────────────────────────────────── */
.btn {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  padding: 8px 18px;
  border: none;
  border-radius: 9999px;
  font-family: var(--fluen-font-sans);
  font-size: 0.82rem;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.15s ease, opacity 0.15s ease;
  white-space: nowrap;
}

.btn--primary {
  background: var(--fluen-primary);
  color: var(--fluen-on-primary);
}

.btn--primary:hover {
  background: var(--fluen-primary-soft);
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

.btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
