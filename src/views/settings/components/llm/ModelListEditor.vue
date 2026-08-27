<script setup lang="ts">
/**
 * ModelListEditor — 供应商模型列表编辑器。
 *
 * 展示模型行（名称 + 上下文/能力徽标 + 使用/编辑/删除操作），
 * 内嵌「添加 / 编辑模型」表单。数据的持久化由父级通过事件处理，
 * 便于在「详情面板」（直接写配置）与「添加面板」（暂存草稿）间复用。
 */
import { ref } from 'vue';
import { useI18n } from '../../../../i18n';
import type { ModelConfig } from '../../../../types/llm';
import { formatContext, draftFromModel, modelFromDraft, CONTEXT_OPTIONS, type ModelDraft } from './shared';

const props = defineProps<{
  /** 模型列表。 */
  models: ModelConfig[];
  /** 当前使用的模型 ID（仅详情面板传入；null 表示无激活项）。 */
  activeModelId?: string | null;
  /** 列表为空时的提示文案。 */
  emptyHint?: string;
  /** 是否展示「使用此模型」操作（添加面板中无意义）。 */
  showUseAction?: boolean;
}>();

const emit = defineEmits<{
  /** 使用某个模型（设为全局激活项）。 */
  use: [modelId: string];
  /** 删除某个模型。 */
  remove: [modelId: string];
  /** 提交模型（`originalId` 为 null 表示新增）。 */
  save: [model: ModelConfig, originalId: string | null];
}>();

const { t } = useI18n();

/* ── 编辑器状态 ─────────────────────────────────────────────────────── */

/** 当前打开的编辑表单（originalId 为 null 表示新增）。 */
const editing = ref<{ originalId: string | null; draft: ModelDraft } | null>(null);

/** 打开新增表单。 */
function startAdd(): void {
  editing.value = {
    originalId: null,
    draft: { id: '', name: '', contextWindow: null, thinking: true, vision: false },
  };
}

/** 打开编辑表单。 */
function startEdit(model: ModelConfig): void {
  editing.value = { originalId: model.id, draft: draftFromModel(model) };
}

function cancelEdit(): void {
  editing.value = null;
}

/** 提交表单（模型 ID 必填）。 */
function submit(): void {
  if (!editing.value || !editing.value.draft.id.trim()) return;
  emit('save', modelFromDraft(editing.value.draft), editing.value.originalId);
  editing.value = null;
}

/** 上下文档位变化时同步草稿值。 */
function onContextChange(event: Event): void {
  if (!editing.value) return;
  const raw = (event.target as HTMLSelectElement).value;
  editing.value.draft.contextWindow = raw ? Number(raw) : null;
}
</script>

<template>
  <div class="model-editor">
    <!-- 空状态 -->
    <div v-if="models.length === 0 && !editing" class="model-editor__empty">
      <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="10" />
        <path d="M12 8v4M12 16h.01" />
      </svg>
      {{ emptyHint ?? t('settings.llmConfig.emptyModels') }}
    </div>

    <!-- 模型行 -->
    <div v-else class="model-editor__list">
      <div v-for="model in models" :key="model.id" class="model-row">
        <span class="model-row__name" :title="`${model.name}（${model.id}）`">{{ model.name }}</span>
        <span class="model-row__tags">
          <span v-if="model.capabilities.vision" class="model-tag">{{ t('settings.llmConfig.tagVision') }}</span>
          <span v-if="model.capabilities.thinking" class="model-tag">{{ t('settings.llmConfig.tagThinking') }}</span>
          <span v-if="formatContext(model.context_window)" class="model-tag model-tag--mono">{{ formatContext(model.context_window) }}</span>
        </span>
        <span class="model-row__actions">
          <button
            v-if="showUseAction && !(activeModelId === model.id)"
            class="model-row__btn"
            :title="t('settings.llmConfig.useModel')"
            type="button"
            @click="emit('use', model.id)"
          >
            <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
              <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
              <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
            </svg>
          </button>
          <span v-else-if="showUseAction" class="model-row__active" :title="t('settings.llmConfig.active')">
            <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M5 13l4 4L19 7" />
            </svg>
          </span>
          <button class="model-row__btn" :title="t('settings.llmConfig.editModel')" type="button" @click="startEdit(model)">
            <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
              <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3z" />
            </svg>
          </button>
          <button class="model-row__btn model-row__btn--danger" :title="t('settings.llmConfig.remove')" type="button" @click="emit('remove', model.id)">
            <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
              <path d="M3 6h18M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2m3 0v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
            </svg>
          </button>
        </span>
      </div>
    </div>

    <!-- 内嵌添加 / 编辑表单 -->
    <div v-if="editing" class="model-form">
      <div class="model-form__grid">
        <div class="model-form__field">
          <label class="model-form__label">{{ t('settings.llmConfig.modelId') }}</label>
          <input v-model="editing.draft.id" class="form-input" type="text" :placeholder="t('settings.llmConfig.placeholders.modelId')" />
        </div>
        <div class="model-form__field">
          <label class="model-form__label">{{ t('settings.llmConfig.modelDisplayName') }}</label>
          <input v-model="editing.draft.name" class="form-input" type="text" :placeholder="t('settings.llmConfig.placeholders.modelDisplayName')" />
        </div>
        <div class="model-form__field">
          <label class="model-form__label">{{ t('settings.llmConfig.contextWindow') }}</label>
          <select
            class="form-input"
            :value="editing.draft.contextWindow ?? ''"
            @change="onContextChange"
          >
            <option
              v-for="opt in CONTEXT_OPTIONS"
              :key="opt.label"
              :value="opt.value ?? ''"
            >
              {{ opt.label }}
            </option>
          </select>
        </div>
      </div>
      <div class="model-form__checks">
        <label class="model-form__check">
          <input v-model="editing.draft.thinking" type="checkbox" />
          {{ t('settings.llmConfig.tagThinking') }}
        </label>
        <label class="model-form__check">
          <input v-model="editing.draft.vision" type="checkbox" />
          {{ t('settings.llmConfig.tagVision') }}
        </label>
      </div>
      <div class="model-form__actions">
        <button class="btn btn--ghost btn--sm" type="button" @click="cancelEdit">
          {{ t('settings.llmConfig.cancel') }}
        </button>
        <button
          class="btn btn--primary btn--sm"
          type="button"
          :disabled="!editing.draft.id.trim()"
          @click="submit"
        >
          {{ editing.originalId ? t('settings.llmConfig.save') : t('settings.llmConfig.addModel') }}
        </button>
      </div>
    </div>

    <!-- 添加按钮 -->
    <button v-if="!editing" class="model-editor__add" type="button" @click="startAdd">
      <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
        <path d="M12 5v14M5 12h14" />
      </svg>
      {{ t('settings.llmConfig.addModel') }}
    </button>
  </div>
</template>

<style scoped>
.model-editor__empty {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 14px 16px;
  border: 1px dashed var(--fluen-hairline);
  border-radius: 10px;
  color: var(--fluen-stone);
  font-size: 0.82rem;
}

.model-editor__list {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--fluen-hairline);
  border-radius: 10px;
  overflow: hidden;
}

.model-row {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 9px 14px;
  background: var(--fluen-canvas);
}

.model-row + .model-row {
  border-top: 1px solid var(--fluen-hairline);
}

.model-row__name {
  flex: 1;
  font-family: var(--fluen-font-mono, monospace);
  font-size: 0.84rem;
  color: var(--fluen-charcoal);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.model-row__tags {
  display: flex;
  gap: 0.3rem;
  flex-shrink: 0;
}

.model-tag {
  padding: 1px 7px;
  border-radius: 4px;
  background: var(--fluen-hover);
  color: var(--fluen-steel);
  font-size: 0.68rem;
  font-weight: 500;
}

.model-tag--mono {
  font-family: var(--fluen-font-mono, monospace);
}

.model-row__actions {
  display: flex;
  align-items: center;
  gap: 0.2rem;
  flex-shrink: 0;
}

.model-row__btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
  transition: all 0.15s ease;
}

.model-row__btn:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.model-row__btn--danger:hover {
  color: var(--fluen-error);
}

.model-row__active {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  color: var(--fluen-accent);
}

.model-editor__add {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  margin-top: 0.6rem;
  padding: 7px 14px;
  border: none;
  border-radius: 8px;
  background: var(--fluen-hover);
  color: var(--fluen-charcoal);
  font-family: var(--fluen-font-sans);
  font-size: 0.8rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
}

.model-editor__add:hover {
  background: var(--fluen-info-bg, var(--fluen-hover));
  color: var(--fluen-accent);
}

/* ── 内嵌表单 ───────────────────────────────────────────────────────── */
.model-form {
  margin-top: 0.6rem;
  padding: 0.9rem;
  border: 1px solid var(--fluen-hairline);
  border-radius: 10px;
  background: var(--fluen-canvas);
}

.model-form__grid {
  display: grid;
  grid-template-columns: 1.2fr 1fr 0.8fr;
  gap: 0.6rem;
}

.model-form__label {
  display: block;
  margin-bottom: 0.3rem;
  font-size: 0.72rem;
  font-weight: 500;
  color: var(--fluen-steel);
}

.model-form__checks {
  display: flex;
  gap: 1.2rem;
  margin-top: 0.6rem;
}

.model-form__check {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  font-size: 0.8rem;
  color: var(--fluen-charcoal);
  cursor: pointer;
}

.model-form__actions {
  display: flex;
  justify-content: flex-end;
  gap: 0.5rem;
  margin-top: 0.8rem;
}

/* ── 表单控件（与设置页通用风格一致） ───────────────────────────────── */
.form-input {
  width: 100%;
  padding: 8px 12px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-surface);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 0.85rem;
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
  padding: 5px 12px;
  font-size: 0.78rem;
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
