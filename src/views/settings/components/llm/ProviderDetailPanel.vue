<script setup lang="ts">
/**
 * ProviderDetailPanel — 供应商配置详情面板（右栏）。
 *
 * 展示并编辑单个供应商：连接方式（API 风格）、Base URL、API Key、
 * 模型列表与删除入口。修改即时持久化（委托 useLlmSettings）。
 */
import { ref, computed, watch } from 'vue';
import { useI18n } from '../../../../i18n';
import { useLlmSettings } from '../../composables/useLlmSettings';
import type { ApiStyle, ProviderConfig } from '../../../../types/llm';
import ModelListEditor from './ModelListEditor.vue';
import { isPresetProvider, providerIcon, providerEnabled, API_KEY_URLS } from './shared';

const props = defineProps<{
  /** 展示的提供商。 */
  provider: ProviderConfig;
}>();

const { t } = useI18n();

/** 是否为内置预设供应商（API 风格跟随预设，不可切换）。 */
const isPreset = isPresetProvider(props.provider);

/** 内置预设的连接方式文案（静态展示）。 */
const presetStyleText = computed(() =>
  props.provider.default_style === 'Anthropic'
    ? t('settings.llmConfig.styleAnthropic')
    : t('settings.llmConfig.styleOpenAI'),
);
const {
  activeProviderId,
  activeModelId,
  updateProvider,
  updateApiKey,
  upsertModel,
  removeModel,
  setActive,
  removeProvider,
} = useLlmSettings();

/* ── 本地表单状态（切换提供商时从配置同步） ─────────────────────────── */
const style = ref<ApiStyle>('OpenAI');
const baseUrl = ref('');
const apiKey = ref('');
const showKey = ref(false);

watch(
  () => props.provider.id,
  () => {
    style.value = props.provider.default_style;
    baseUrl.value =
      (props.provider.default_style === 'Anthropic'
        ? props.provider.anthropic_base_url
        : props.provider.openai_base_url) ?? '';
    apiKey.value = props.provider.api_key ?? '';
    showKey.value = false;
  },
  { immediate: true },
);

/** 该风格是否已有可用的 Base URL（决定提示与提交合法性）。 */
const hasBaseUrl = (): boolean => baseUrl.value.trim().length > 0;

/** 提交连接方式 + Base URL（两者需同时持久化以满足后端校验）。 */
async function commitConnection(): Promise<void> {
  if (!hasBaseUrl()) return;
  const url = baseUrl.value.trim();
  await updateProvider(
    props.provider.id,
    style.value === 'OpenAI'
      ? { default_style: 'OpenAI', openai_base_url: url, anthropic_base_url: props.provider.anthropic_base_url }
      : { default_style: 'Anthropic', anthropic_base_url: url, openai_base_url: props.provider.openai_base_url },
  );
}

/** 切换 API 风格时，载入该风格已存的 URL（无则留空待填）。 */
function onStyleChange(): void {
  baseUrl.value =
    (style.value === 'Anthropic'
      ? props.provider.anthropic_base_url
      : props.provider.openai_base_url) ?? '';
  if (hasBaseUrl()) void commitConnection();
}

/** 提交 API Key（配置后自动启用，清空则停用）。 */
async function commitApiKey(): Promise<void> {
  await updateApiKey(props.provider.id, apiKey.value.trim());
}

/** 供应商品牌图标（预设才有）。 */
const icon = providerIcon(props.provider);

/** 「获取 API Key」链接（预设供应商才有）。 */
const keyUrl = API_KEY_URLS[props.provider.id] ?? null;

/** 当前供应商是否为全局激活供应商。 */
const isActiveProvider = (): boolean => activeProviderId.value === props.provider.id;

/** 删除当前供应商（带确认）。 */
async function handleRemove(): Promise<void> {
  if (!window.confirm(t('settings.llmConfig.deleteProviderConfirm'))) return;
  await removeProvider(props.provider.id);
}
</script>

<template>
  <div class="detail">
    <!-- 头部：图标 + 名称 + 状态徽标 + 连接方式 -->
    <header class="detail__header">
      <div class="detail__title">
        <svg v-if="icon" class="detail__icon" viewBox="0 0 24 24" width="22" height="22" v-html="icon" />
        <h3 class="detail__name">{{ provider.name }}</h3>
        <span class="detail__badge" :class="providerEnabled(provider) ? 'detail__badge--on' : 'detail__badge--off'">
          {{ providerEnabled(provider) ? t('settings.llmConfig.enabled') : t('settings.llmConfig.disabled') }}
        </span>
      </div>
      <!-- 内置预设：API 风格跟随预设固定，仅展示；自定义供应商：可切换 -->
      <div v-if="isPreset" class="detail__method">
        <span class="detail__method-label">{{ t('settings.llmConfig.connectionMethod') }}</span>
        <span class="detail__method-static">{{ presetStyleText }}</span>
      </div>
      <label v-else class="detail__method">
        <span class="detail__method-label">{{ t('settings.llmConfig.connectionMethod') }}</span>
        <select v-model="style" class="detail__method-select" @change="onStyleChange">
          <option value="OpenAI">{{ t('settings.llmConfig.styleOpenAI') }}</option>
          <option value="Anthropic">{{ t('settings.llmConfig.styleAnthropic') }}</option>
        </select>
      </label>
    </header>

    <!-- 未配置 Key 提示 -->
    <div v-if="!provider.api_key" class="detail__hint">
      <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="10" />
        <path d="M12 8v4M12 16h.01" />
      </svg>
      {{ t('settings.llmConfig.apiKeyHint') }}
    </div>

    <!-- Base URL -->
    <div class="detail__field">
      <label class="detail__label">Base URL</label>
      <input
        v-model="baseUrl"
        class="form-input"
        type="text"
        :placeholder="t('settings.llmConfig.placeholders.baseUrl')"
        @change="commitConnection"
        @blur="commitConnection"
      />
    </div>

    <!-- API Key -->
    <div class="detail__field">
      <div class="detail__label-row">
        <label class="detail__label">{{ t('settings.llmConfig.apiKey') }}</label>
        <a
          v-if="keyUrl"
          class="detail__link"
          :href="keyUrl"
          target="_blank"
          rel="noopener noreferrer"
        >
          {{ t('settings.llmConfig.getApiKey') }}
        </a>
      </div>
      <div class="detail__key">
        <input
          v-model="apiKey"
          class="form-input"
          :type="showKey ? 'text' : 'password'"
          :placeholder="t('settings.llmConfig.placeholders.apiKey')"
          autocomplete="off"
          @change="commitApiKey"
          @blur="commitApiKey"
        />
        <button class="detail__eye" type="button" @click="showKey = !showKey">
          <svg v-if="!showKey" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7-10-7-10-7z" />
            <circle cx="12" cy="12" r="3" />
          </svg>
          <svg v-else viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24" />
            <path d="M1 1l22 22" />
          </svg>
        </button>
      </div>
    </div>

    <!-- 模型列表 -->
    <div class="detail__field">
      <label class="detail__label">{{ t('settings.llmConfig.models') }}</label>
      <ModelListEditor
        :models="provider.models"
        :active-model-id="isActiveProvider() ? activeModelId : null"
        show-use-action
        @use="setActive(provider.id, $event)"
        @save="(model, originalId) => upsertModel(provider.id, model, originalId ?? undefined)"
        @remove="removeModel(provider.id, $event)"
      />
    </div>

    <!-- 底部：删除供应商 -->
    <footer class="detail__footer">
      <button class="detail__delete" type="button" @click="handleRemove">
        <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M3 6h18M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2m3 0v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
        </svg>
        {{ t('settings.llmConfig.deleteProvider') }}
      </button>
    </footer>
  </div>
</template>

<style scoped>
.detail {
  display: flex;
  flex-direction: column;
  gap: 1.1rem;
  padding: 1.25rem 1.5rem;
}

/* ── 头部 ───────────────────────────────────────────────────────────── */
.detail__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  flex-wrap: wrap;
}

.detail__title {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  min-width: 0;
}

.detail__icon {
  flex-shrink: 0;
  color: var(--fluen-accent);
}

.detail__name {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 1.1rem;
  font-weight: 600;
  color: var(--fluen-ink);
}

.detail__badge {
  padding: 2px 9px;
  border-radius: 9999px;
  font-size: 0.7rem;
  font-weight: 600;
  white-space: nowrap;
}

.detail__badge--on {
  background: var(--fluen-success-bg, var(--fluen-hover));
  color: var(--fluen-success-text, var(--fluen-accent));
}

.detail__badge--off {
  background: var(--fluen-hover);
  color: var(--fluen-stone);
}

.detail__method {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.detail__method-label {
  font-size: 0.78rem;
  color: var(--fluen-stone);
}

.detail__method-select {
  padding: 6px 10px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-surface);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 0.8rem;
  outline: none;
  cursor: pointer;
}

.detail__method-static {
  padding: 6px 10px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-hover);
  color: var(--fluen-steel);
  font-family: var(--fluen-font-sans);
  font-size: 0.8rem;
}

/* ── 提示条 ─────────────────────────────────────────────────────────── */
.detail__hint {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 10px 14px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 10px;
  background: var(--fluen-hover);
  color: var(--fluen-stone);
  font-size: 0.82rem;
}

/* ── 字段 ───────────────────────────────────────────────────────────── */
.detail__field {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.detail__label {
  font-size: 0.78rem;
  font-weight: 500;
  color: var(--fluen-steel);
}

.detail__label-row {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
}

.detail__link {
  font-size: 0.78rem;
  color: var(--fluen-accent);
  text-decoration: none;
}

.detail__link:hover {
  text-decoration: underline;
}

.detail__key {
  position: relative;
  display: flex;
}

.detail__key .form-input {
  padding-right: 40px;
}

.detail__eye {
  position: absolute;
  right: 6px;
  top: 50%;
  transform: translateY(-50%);
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
}

.detail__eye:hover {
  color: var(--fluen-ink);
}

/* ── 底部 ───────────────────────────────────────────────────────────── */
.detail__footer {
  display: flex;
  justify-content: flex-end;
  padding-top: 0.4rem;
  border-top: 1px solid var(--fluen-hairline);
}

.detail__delete {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  padding: 6px 12px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: var(--fluen-error);
  font-family: var(--fluen-font-sans);
  font-size: 0.78rem;
  font-weight: 500;
  cursor: pointer;
  transition: background 0.15s ease;
}

.detail__delete:hover {
  background: var(--fluen-error-bg, var(--fluen-hover));
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
</style>
