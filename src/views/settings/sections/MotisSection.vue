<script setup lang="ts">
/**
 * MotisSection — Motis 设置分区（合并桌面宠物与智慧驱动）。
 *
 * 功能：
 *   - 启用 / 禁用 Motis
 *   - 查看 Motis 名称（可编辑）
 *   - 查看好感度与当前心情
 *   - 选择 Motis 人格
 *   - 对话风格开关（展示思考内容 / 专业化表述）
 *   - 高级能力开关（MCP / Skills / 函数调用）
 *   - 智慧驱动：为 Motis 选择独立的 LLM 服务商与模型
 *
 * 状态管理委托给 useMotisSettings 与 useMotisEngine composable，组件仅负责 UI。
 */
import { ref, onMounted, inject, watch } from 'vue';
import { useI18n } from '../../../i18n';
import { useMotisSettings } from '../composables/useMotisSettings';
import { useMotisEngine } from '../composables/useMotisEngine';
import { SETTINGS_NAVIGATE_KEY } from '../types';
import { Mascot } from '../../../components/mascot';
import type { Mood } from '../../../types/mascot';

const { t } = useI18n();

/* ── Motis 基本设置 ─────────────────────────────────────────────────── */
const {
  enabled,
  name,
  personality,
  mcpEnabled,
  skillsEnabled,
  functionCallingEnabled,
  showThinkingContent,
  professionalExpression,
  agents,
  affinity,
  mood,
  personalities,
  isLoading: isMotisLoading,
  load: loadMotis,
  toggleEnabled,
  updateName,
  updatePersonality,
  toggleMcp,
  toggleSkills,
  toggleFunctionCalling,
  toggleShowThinking,
  toggleProfessionalExpression,
  isAgentEnabled,
  toggleAgent,
} = useMotisSettings();

/* ── 智慧驱动 ───────────────────────────────────────────────────────── */
const {
  isLoading: isEngineLoading,
  availableProviders,
  availableModels,
  currentProviderId,
  currentModelId,
  isEmpty: isEngineEmpty,
  load: loadEngine,
  selectProvider,
  selectModel,
} = useMotisEngine();

/* ── 分区导航（用于空状态跳转） ─────────────────────────────────────── */
const navigate = inject(SETTINGS_NAVIGATE_KEY, null);

/** 跳转到 LLM 配置分区。 */
function goToLlmConfig(): void {
  navigate?.('llmConfig');
}

/* ── 加载 ───────────────────────────────────────────────────────────── */
onMounted(() => {
  loadMotis();
  loadEngine();
});

const isLoading = ref(false);
watch([isMotisLoading, isEngineLoading], ([a, b]) => {
  isLoading.value = a || b;
}, { immediate: true });

/* ── 名称编辑 ───────────────────────────────────────────────────────── */
const editName = ref('');
const isEditingName = ref(false);

function startEditName(): void {
  editName.value = name.value;
  isEditingName.value = true;
}

async function confirmEditName(): Promise<void> {
  const trimmed = editName.value.trim();
  if (trimmed && trimmed !== name.value) {
    await updateName(trimmed);
  }
  isEditingName.value = false;
}

function cancelEditName(): void {
  isEditingName.value = false;
}

/* ── 人格选择 ───────────────────────────────────────────────────────── */
function handlePersonalitySelect(id: string): void {
  if (id !== personality.value) {
    updatePersonality(id);
  }
}

/* ── 好感度进度条颜色 ───────────────────────────────────────────────── */
function affinityColor(value: number): string {
  if (value >= 70) return 'var(--fluen-success-text)';
  if (value >= 30) return 'var(--fluen-accent)';
  return 'var(--fluen-stone)';
}

/* ── 心情图标映射 ───────────────────────────────────────────────────── */
const moodIcons: Record<Mood, string> = {
  happy: '😊',
  neutral: '😐',
  sad: '😢',
};

/* ── 名称变化时同步编辑框 ───────────────────────────────────────────── */
watch(name, (val) => {
  if (!isEditingName.value) editName.value = val;
});

/* ── 智慧驱动选择处理 ───────────────────────────────────────────────── */

/** 服务商下拉变化时触发。 */
function onProviderChange(event: Event): void {
  const value = (event.target as HTMLSelectElement).value;
  void selectProvider(value || null);
}

/** 模型下拉变化时触发。 */
function onModelChange(event: Event): void {
  const value = (event.target as HTMLSelectElement).value;
  void selectModel(value || null);
}
</script>

<template>
  <div class="section">
    <h2 class="section__title">{{ t('settings.sections.motis') }}</h2>
    <p class="section__desc">{{ t('settings.motis.description') }}</p>

    <!-- 加载中 -->
    <div v-if="isLoading" class="section__loading">
      {{ t('settings.motis.loading') }}
    </div>

    <template v-else>
      <!-- ── 启用开关 + 预览 ───────────────────────────────────────── -->
      <div class="motis-card">
        <div class="motis-card__preview">
          <Mascot :height="48" :mood="mood" :interactive="enabled" />
        </div>
        <div class="motis-card__info">
          <div class="motis-card__name-row">
            <template v-if="!isEditingName">
              <span class="motis-card__name">{{ name }}</span>
              <button v-if="enabled" class="icon-btn" :title="t('settings.motis.editName')" @click="startEditName">
                <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
                  <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" />
                </svg>
              </button>
            </template>
            <template v-else>
              <input
                v-model="editName"
                class="name-input"
                type="text"
                maxlength="20"
                @keyup.enter="confirmEditName"
                @keyup.escape="cancelEditName"
              />
              <button class="icon-btn icon-btn--confirm" :title="t('settings.motis.confirmName')" @click="confirmEditName">
                <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M5 13l4 4L19 7" />
                </svg>
              </button>
              <button class="icon-btn" :title="t('settings.motis.cancelName')" @click="cancelEditName">
                <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M18 6 6 18M6 6l12 12" />
                </svg>
              </button>
            </template>
          </div>

          <!-- 心情 + 好感度 -->
          <div class="motis-card__stats">
            <div class="stat-item">
              <span class="stat-item__label">{{ t('settings.motis.moodLabel') }}</span>
              <span class="stat-item__value">{{ moodIcons[mood] }} {{ t('mascot.mood.' + mood) }}</span>
            </div>
            <div class="stat-item">
              <span class="stat-item__label">{{ t('settings.motis.affinityLabel') }}</span>
              <div class="affinity-bar">
                <div class="affinity-bar__fill" :style="{ width: affinity + '%', background: affinityColor(affinity) }" />
                <span class="affinity-bar__text">{{ affinity }} / 100</span>
              </div>
            </div>
          </div>
        </div>

        <!-- 启用开关 -->
        <label class="toggle">
          <input type="checkbox" :checked="enabled" class="toggle__input" @change="toggleEnabled(($event.target as HTMLInputElement).checked)" />
          <span class="toggle__track" :class="{ 'toggle__track--on': enabled }">
            <span class="toggle__thumb" :class="{ 'toggle__thumb--on': enabled }" />
          </span>
        </label>
      </div>

      <!-- ── 人格选择 ───────────────────────────────────────────────── -->
      <div v-if="enabled" class="form-group">
        <label class="form-label">{{ t('settings.motis.personality') }}</label>
        <div class="personality-grid">
          <button
            v-for="opt in personalities"
            :key="opt.id"
            class="personality-card"
            :class="{ 'personality-card--active': personality === opt.id }"
            type="button"
            @click="handlePersonalitySelect(opt.id)"
          >
            <span class="personality-card__label">{{ t('settings.motis.personalityOptions.' + opt.labelKey) }}</span>
          </button>
        </div>
      </div>

      <!-- ── 对话风格 ───────────────────────────────────────────────── -->
      <div v-if="enabled" class="dialog-style-section">
        <h3 class="sub-section__title">{{ t('settings.motis.dialogStyle.title') }}</h3>

        <div class="capability-row">
          <div class="capability-row__info">
            <span class="capability-row__label">{{ t('settings.motis.dialogStyle.showThinking') }}</span>
            <span class="capability-row__desc">{{ t('settings.motis.dialogStyle.showThinkingDesc') }}</span>
          </div>
          <label class="toggle">
            <input type="checkbox" :checked="showThinkingContent" class="toggle__input" @change="toggleShowThinking(($event.target as HTMLInputElement).checked)" />
            <span class="toggle__track" :class="{ 'toggle__track--on': showThinkingContent }">
              <span class="toggle__thumb" :class="{ 'toggle__thumb--on': showThinkingContent }" />
            </span>
          </label>
        </div>

        <div class="capability-row">
          <div class="capability-row__info">
            <span class="capability-row__label">{{ t('settings.motis.dialogStyle.professionalExpression') }}</span>
            <span class="capability-row__desc">{{ t('settings.motis.dialogStyle.professionalExpressionDesc') }}</span>
          </div>
          <label class="toggle">
            <input type="checkbox" :checked="professionalExpression" class="toggle__input" @change="toggleProfessionalExpression(($event.target as HTMLInputElement).checked)" />
            <span class="toggle__track" :class="{ 'toggle__track--on': professionalExpression }">
              <span class="toggle__thumb" :class="{ 'toggle__thumb--on': professionalExpression }" />
            </span>
          </label>
        </div>
      </div>

      <!-- ── 子智能体配置 ─────────────────────────────────────────── -->
      <div v-if="enabled" class="agents-section">
        <h3 class="sub-section__title">{{ t('settings.motis.agents.title') }}</h3>
        <p class="sub-section__desc">{{ t('settings.motis.agents.description') }}</p>

        <div
          v-for="agent in agents"
          :key="agent.id"
          class="capability-row"
        >
          <div class="capability-row__info">
            <span class="capability-row__label">{{ t('settings.motis.agents.' + agent.labelKey) }}</span>
            <span class="capability-row__desc">{{ t('settings.motis.agents.' + agent.descKey) }}</span>
          </div>
          <label class="toggle">
            <input
              type="checkbox"
              :checked="isAgentEnabled(agent.id)"
              class="toggle__input"
              @change="toggleAgent(agent.id, ($event.target as HTMLInputElement).checked)"
            />
            <span class="toggle__track" :class="{ 'toggle__track--on': isAgentEnabled(agent.id) }">
              <span class="toggle__thumb" :class="{ 'toggle__thumb--on': isAgentEnabled(agent.id) }" />
            </span>
          </label>
        </div>
      </div>

      <!-- ── 高级能力 ───────────────────────────────────────────────── -->
      <div v-if="enabled" class="advanced-section">
        <h3 class="sub-section__title">{{ t('settings.motis.advanced') }}</h3>

        <div class="capability-row">
          <div class="capability-row__info">
            <span class="capability-row__label">{{ t('settings.motis.capabilities.mcp') }}</span>
            <span class="capability-row__desc">{{ t('settings.motis.capabilities.mcpDesc') }}</span>
          </div>
          <label class="toggle">
            <input type="checkbox" :checked="mcpEnabled" class="toggle__input" @change="toggleMcp(($event.target as HTMLInputElement).checked)" />
            <span class="toggle__track" :class="{ 'toggle__track--on': mcpEnabled }">
              <span class="toggle__thumb" :class="{ 'toggle__thumb--on': mcpEnabled }" />
            </span>
          </label>
        </div>

        <div class="capability-row">
          <div class="capability-row__info">
            <span class="capability-row__label">{{ t('settings.motis.capabilities.skills') }}</span>
            <span class="capability-row__desc">{{ t('settings.motis.capabilities.skillsDesc') }}</span>
          </div>
          <label class="toggle">
            <input type="checkbox" :checked="skillsEnabled" class="toggle__input" @change="toggleSkills(($event.target as HTMLInputElement).checked)" />
            <span class="toggle__track" :class="{ 'toggle__track--on': skillsEnabled }">
              <span class="toggle__thumb" :class="{ 'toggle__thumb--on': skillsEnabled }" />
            </span>
          </label>
        </div>

        <div class="capability-row">
          <div class="capability-row__info">
            <span class="capability-row__label">{{ t('settings.motis.capabilities.functionCalling') }}</span>
            <span class="capability-row__desc">{{ t('settings.motis.capabilities.functionCallingDesc') }}</span>
          </div>
          <label class="toggle">
            <input type="checkbox" :checked="functionCallingEnabled" class="toggle__input" @change="toggleFunctionCalling(($event.target as HTMLInputElement).checked)" />
            <span class="toggle__track" :class="{ 'toggle__track--on': functionCallingEnabled }">
              <span class="toggle__thumb" :class="{ 'toggle__thumb--on': functionCallingEnabled }" />
            </span>
          </label>
        </div>
      </div>

      <!-- ── 智慧驱动 ───────────────────────────────────────────────── -->
      <div v-if="enabled" class="engine-section">
        <h3 class="sub-section__title">{{ t('settings.motis.engine.title') }}</h3>
        <p class="sub-section__desc">{{ t('settings.motis.engine.description') }}</p>

        <!-- 空状态：无 LLM 配置 -->
        <div v-if="isEngineEmpty" class="empty-state">
          <p class="empty-state__text">{{ t('settings.motis.engine.emptyHint') }}</p>
          <button class="btn btn--secondary btn--sm" @click="goToLlmConfig">
            {{ t('settings.sections.llmConfig') }}
          </button>
        </div>

        <!-- 选择表单 -->
        <div v-else class="engine-form">
          <!-- 服务商选择 -->
          <div class="form-group">
            <label class="form-label" for="motis-provider-select">
              {{ t('settings.motis.engine.provider') }}
            </label>
            <select
              id="motis-provider-select"
              class="form-select"
              :value="currentProviderId ?? ''"
              @change="onProviderChange"
            >
              <option value="">{{ t('common.placeholder.select') }}</option>
              <option v-for="provider in availableProviders" :key="provider.id" :value="provider.id">
                {{ provider.name }}
              </option>
            </select>
          </div>

          <!-- 模型选择 -->
          <div class="form-group">
            <label class="form-label" for="motis-model-select">
              {{ t('settings.motis.engine.model') }}
            </label>
            <select
              id="motis-model-select"
              class="form-select"
              :value="currentModelId ?? ''"
              :disabled="availableModels.length === 0"
              @change="onModelChange"
            >
              <option value="">{{ t('common.placeholder.select') }}</option>
              <option v-for="model in availableModels" :key="model.id" :value="model.id">
                {{ model.name }}
              </option>
            </select>
            <p v-if="availableModels.length === 0 && currentProviderId" class="form-hint">
              {{ t('settings.motis.engine.emptyHint') }}
            </p>
          </div>
        </div>
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

/* ── Motis 主卡片 ────────────────────────────────────────────────────── */
.motis-card {
  display: flex;
  align-items: center;
  gap: 1rem;
  padding: 1rem 1.25rem;
  border: 1px solid var(--fluen-hairline);
  border-radius: 12px;
  background: var(--fluen-surface);
  max-width: 640px;
  margin-bottom: 1.5rem;
}

.motis-card__preview {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 72px;
  height: 56px;
  flex-shrink: 0;
  border-radius: 10px;
  background: var(--fluen-canvas);
  border: 1px solid var(--fluen-hairline);
}

.motis-card__info {
  flex: 1;
  min-width: 0;
}

.motis-card__name-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 0.5rem;
}

.motis-card__name {
  font-family: var(--fluen-font-sans);
  font-size: 1.1rem;
  font-weight: 600;
  color: var(--fluen-ink);
}

.name-input {
  width: 160px;
  padding: 4px 10px;
  border: 1px solid var(--fluen-accent);
  border-radius: 6px;
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 1rem;
  font-weight: 500;
  outline: none;
}

.icon-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
  border-radius: 6px;
  transition: background 0.15s ease, color 0.15s ease;
}

.icon-btn:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.icon-btn--confirm {
  color: var(--fluen-accent);
}

.icon-btn--confirm:hover {
  color: var(--fluen-accent);
  background: var(--fluen-info-bg);
}

/* ── 状态信息 ────────────────────────────────────────────────────────── */
.motis-card__stats {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.stat-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.stat-item__label {
  font-size: 0.75rem;
  color: var(--fluen-stone);
  min-width: 56px;
}

.stat-item__value {
  font-size: 0.82rem;
  color: var(--fluen-charcoal);
  font-weight: 500;
}

.affinity-bar {
  position: relative;
  flex: 1;
  height: 18px;
  border-radius: 9999px;
  background: var(--fluen-hover);
  overflow: hidden;
  min-width: 120px;
}

.affinity-bar__fill {
  height: 100%;
  border-radius: 9999px;
  transition: width 0.4s ease, background 0.3s ease;
}

.affinity-bar__text {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  font-size: 0.68rem;
  font-weight: 600;
  color: var(--fluen-ink);
  text-shadow: 0 0 4px var(--fluen-canvas);
}

/* ── 开关组件 ────────────────────────────────────────────────────────── */
.toggle {
  display: inline-flex;
  cursor: pointer;
  flex-shrink: 0;
}

.toggle__input {
  position: absolute;
  opacity: 0;
  width: 0;
  height: 0;
}

.toggle__track {
  display: inline-flex;
  align-items: center;
  width: 38px;
  height: 22px;
  border-radius: 9999px;
  background: var(--fluen-hairline);
  transition: background 0.2s ease;
  padding: 2px;
}

.toggle__track--on {
  background: var(--fluen-accent);
}

.toggle__thumb {
  display: block;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: #ffffff;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
  transition: transform 0.2s ease;
}

.toggle__thumb--on {
  transform: translateX(16px);
}

/* ── 人格选择 ────────────────────────────────────────────────────────── */
.form-group {
  margin-bottom: 1.5rem;
  max-width: 640px;
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

.personality-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
  gap: 0.5rem;
}

.personality-card {
  padding: 10px 12px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-canvas);
  cursor: pointer;
  transition: all 0.15s ease;
  text-align: center;
}

.personality-card:hover {
  border-color: var(--fluen-stone);
}

.personality-card--active {
  border-color: var(--fluen-accent);
  background: var(--fluen-info-bg);
}

.personality-card__label {
  font-family: var(--fluen-font-sans);
  font-size: 0.82rem;
  font-weight: 500;
  color: var(--fluen-charcoal);
}

/* ── 子分区标题 ──────────────────────────────────────────────────────── */
.sub-section__title {
  margin: 0 0 0.75rem;
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

/* ── 对话风格 ────────────────────────────────────────────────────────── */
.dialog-style-section {
  border-top: 1px solid var(--fluen-hairline);
  padding-top: 1.25rem;
  margin-bottom: 1.5rem;
  max-width: 640px;
}

/* ── 子智能体配置 ────────────────────────────────────────────────────── */
.agents-section {
  border-top: 1px solid var(--fluen-hairline);
  padding-top: 1.25rem;
  margin-bottom: 1.5rem;
  max-width: 640px;
}

/* ── 高级能力 ────────────────────────────────────────────────────────── */
.advanced-section {
  border-top: 1px solid var(--fluen-hairline);
  padding-top: 1.25rem;
  margin-bottom: 1.5rem;
  max-width: 640px;
}

.capability-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding: 10px 0;
  border-bottom: 1px solid var(--fluen-hairline);
}

.capability-row:last-child {
  border-bottom: none;
}

.capability-row__info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.capability-row__label {
  font-size: 0.85rem;
  font-weight: 500;
  color: var(--fluen-charcoal);
}

.capability-row__desc {
  font-size: 0.72rem;
  color: var(--fluen-stone);
  line-height: 1.4;
}

/* ── 智慧驱动 ────────────────────────────────────────────────────────── */
.engine-section {
  border-top: 1px solid var(--fluen-hairline);
  padding-top: 1.25rem;
  max-width: 640px;
}

.engine-form {
  max-width: 100%;
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

.form-select:disabled {
  background-color: var(--fluen-hover);
  color: var(--fluen-stone);
  cursor: not-allowed;
}

.form-hint {
  margin: 0.4rem 0 0;
  font-size: 0.72rem;
  color: var(--fluen-stone);
  line-height: 1.4;
}

/* ── 空状态 ─────────────────────────────────────────────────────────── */
.empty-state {
  padding: 1.5rem;
  border: 1px solid var(--fluen-hairline);
  border-radius: 12px;
  background: var(--fluen-surface);
  text-align: center;
}

.empty-state__text {
  margin: 0 0 0.75rem;
  font-size: 0.85rem;
  color: var(--fluen-stone);
  line-height: 1.5;
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
  transition: background 0.15s ease, border-color 0.15s ease;
  white-space: nowrap;
}

.btn--sm {
  padding: 6px 12px;
  font-size: 0.78rem;
}

.btn--secondary {
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  border: 1px solid var(--fluen-hairline);
}

.btn--secondary:hover {
  border-color: var(--fluen-accent);
  color: var(--fluen-accent);
}

/* ── 响应式 ──────────────────────────────────────────────────────────── */
@media (max-width: 600px) {
  .motis-card {
    flex-direction: column;
    text-align: center;
  }

  .motis-card__stats {
    width: 100%;
  }

  .stat-item {
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }

  .stat-item__label {
    min-width: 0;
  }
}
</style>
