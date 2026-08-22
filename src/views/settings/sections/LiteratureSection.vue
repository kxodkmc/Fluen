<script setup lang="ts">
/**
 * LiteratureSection — 文献配置分区（AI 服务分组下的子项）。
 *
 * 配置：
 *   - 文献导入默认模式（纯 OCR / OCR+AI 校正 / 纯 AI 识别）
 *   - AI 最大响应时间（秒），即导入文献时 AI 校正的请求超时
 *
 * 状态管理委托给 useAiServicesSettings composable，组件仅负责 UI。
 */
import { ref, onMounted, onBeforeUnmount } from 'vue';
import { useI18n } from '../../../i18n';
import { useAiServicesSettings } from '../composables/useAiServicesSettings';
import type { ReferenceImportMode } from '../../../types/aiServices';

const { t } = useI18n();
const {
  importMode,
  referenceImportTimeoutSecs,
  isSaving,
  error,
  load,
  setImportMode,
  setReferenceImportTimeoutSecs,
} = useAiServicesSettings();

onMounted(async () => {
  await load();
  timeoutDraft.value = referenceImportTimeoutSecs.value;
});
onBeforeUnmount(() => {
  timeoutDraft.value = null;
});

/* ── 导入模式选项 ───────────────────────────────────────────────────── */
const IMPORT_MODES: {
  value: ReferenceImportMode;
  labelKey: string;
  descKey: string;
}[] = [
  {
    value: 'ocr',
    labelKey: 'settings.aiServices.importModes.ocr',
    descKey: 'settings.aiServices.importModes.ocrDesc',
  },
  {
    value: 'ocr_with_ai_correction',
    labelKey: 'settings.aiServices.importModes.ocrWithAi',
    descKey: 'settings.aiServices.importModes.ocrWithAiDesc',
  },
  {
    value: 'ai_only',
    labelKey: 'settings.aiServices.importModes.aiOnly',
    descKey: 'settings.aiServices.importModes.aiOnlyDesc',
  },
];

/** 切换文献导入默认模式。 */
async function handleImportModeChange(mode: ReferenceImportMode): Promise<void> {
  await setImportMode(mode);
}

/* ── AI 最大响应时间 ────────────────────────────────────────────────── */
/** 超时输入草稿（仅在打开本分区时同步一次）。 */
const timeoutDraft = ref<number | null>(null);

/** 保存 AI 最大响应时间。 */
async function handleSaveTimeout(): Promise<void> {
  if (timeoutDraft.value === null) return;
  await setReferenceImportTimeoutSecs(Number(timeoutDraft.value));
}
</script>

<template>
  <div class="section">
    <h2 class="section__title">{{ t('settings.literature.title') }}</h2>
    <p class="section__desc">{{ t('settings.literature.description') }}</p>

    <div v-if="error" class="section__error">{{ error }}</div>

    <!-- ── 文献导入默认模式 ─────────────────────────────────────────── -->
    <div class="import-mode">
      <div class="import-mode__header">
        <h3 class="import-mode__title">{{ t('settings.aiServices.importMode') }}</h3>
        <span v-if="isSaving" class="saving-hint">{{ t('settings.aiServices.saving') }}</span>
      </div>
      <p class="import-mode__hint">{{ t('settings.aiServices.importModeHint') }}</p>
      <div class="import-mode__options">
        <label
          v-for="mode in IMPORT_MODES"
          :key="mode.value"
          class="import-mode__option"
          :class="{ 'import-mode__option--active': importMode === mode.value }"
        >
          <input
            type="radio"
            name="import-mode"
            :value="mode.value"
            :checked="importMode === mode.value"
            @change="handleImportModeChange(mode.value)"
          />
          <span class="import-mode__option-body">
            <span class="import-mode__option-label">{{ t(mode.labelKey) }}</span>
            <span class="import-mode__option-desc">{{ t(mode.descKey) }}</span>
          </span>
        </label>
      </div>
    </div>

    <!-- ── AI 最大响应时间 ──────────────────────────────────────────── -->
    <div class="timeout">
      <div class="timeout__header">
        <h3 class="timeout__title">{{ t('settings.literature.timeout') }}</h3>
        <span v-if="isSaving" class="saving-hint">{{ t('settings.aiServices.saving') }}</span>
      </div>
      <p class="timeout__hint">{{ t('settings.literature.timeoutHint') }}</p>
      <div class="input-row">
        <input
          v-model.number="timeoutDraft"
          class="form-input"
          type="number"
          min="1"
          step="1"
          :placeholder="String(referenceImportTimeoutSecs)"
        />
        <span class="timeout__unit">{{ t('settings.literature.timeoutUnit') }}</span>
        <button class="btn btn--secondary" :disabled="timeoutDraft === null" @click="handleSaveTimeout">
          {{ t('settings.aiServices.save') }}
        </button>
      </div>
    </div>
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

.section__error {
  padding: 0.75rem 1rem;
  margin-bottom: 1rem;
  border-radius: 8px;
  background: var(--fluen-error-bg);
  color: var(--fluen-error);
  font-size: 0.82rem;
}

/* ── 文献导入默认模式 ─────────────────────────────────────────────── */
.import-mode {
  max-width: 640px;
  margin-bottom: 1.5rem;
  padding: 1rem 1.25rem;
  border: 1px solid var(--fluen-hairline);
  border-radius: 12px;
  background: var(--fluen-surface);
}

.import-mode__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.import-mode__title {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 0.9rem;
  font-weight: 600;
  color: var(--fluen-ink);
}

.import-mode__hint {
  margin: 0.3rem 0 0.9rem;
  font-size: 0.78rem;
  color: var(--fluen-stone);
}

.import-mode__options {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.import-mode__option {
  display: flex;
  align-items: flex-start;
  gap: 0.6rem;
  padding: 0.6rem 0.75rem;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-canvas);
  cursor: pointer;
  transition: border-color 0.15s ease;
}

.import-mode__option:hover {
  border-color: var(--fluen-stone);
}

.import-mode__option--active {
  border-color: var(--fluen-accent);
  background: var(--fluen-info-bg);
}

.import-mode__option input[type="radio"] {
  margin-top: 2px;
  width: 15px;
  height: 15px;
  accent-color: var(--fluen-accent);
  cursor: pointer;
  flex-shrink: 0;
}

.import-mode__option-body {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.import-mode__option-label {
  font-family: var(--fluen-font-sans);
  font-size: 0.85rem;
  font-weight: 500;
  color: var(--fluen-charcoal);
}

.import-mode__option-desc {
  font-size: 0.75rem;
  color: var(--fluen-stone);
}

/* ── AI 最大响应时间 ──────────────────────────────────────────────── */
.timeout {
  max-width: 640px;
  padding: 1rem 1.25rem;
  border: 1px solid var(--fluen-hairline);
  border-radius: 12px;
  background: var(--fluen-surface);
}

.timeout__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.timeout__title {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 0.9rem;
  font-weight: 600;
  color: var(--fluen-ink);
}

.timeout__hint {
  margin: 0.3rem 0 0.9rem;
  font-size: 0.78rem;
  color: var(--fluen-stone);
}

.input-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.input-row .form-input {
  flex: 1;
  max-width: 220px;
}

.timeout__unit {
  font-size: 0.82rem;
  color: var(--fluen-stone);
  font-family: var(--fluen-font-sans);
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

.btn--secondary {
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  border: 1px solid var(--fluen-hairline);
}

.btn--secondary:hover {
  border-color: var(--fluen-stone);
}
</style>