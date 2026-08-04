<script setup lang="ts">
/**
 * StepAppearance — 步骤 1：选择主题模式与界面语言。
 *
 * 上方为两张主题色块卡片（浅色 / 深色），点击切换。
 * 下方为居中的语言下拉列表。
 *
 * 点击主题卡片时立即调用 useTheme().setMode 实时预览主题效果，
 * 同时向上 emit 供 onboarding 状态记录用户选择。
 */
import ObDropdown from '../components/ObDropdown.vue';
import { THEME_OPTIONS } from '../constants';
import { SUPPORTED_LOCALES, useI18n } from '../../../i18n';
import { useTheme } from '../../../theme';
import type { ThemeMode } from '../types';
import type { Language } from '../../../i18n/types';

defineProps<{
  theme: ThemeMode;
  language: Language;
}>();

const emit = defineEmits<{
  (e: 'update:theme', value: ThemeMode): void;
  (e: 'update:language', value: Language): void;
}>();

const { t, setLocale } = useI18n();
const { setMode } = useTheme();

/** 选择主题：立即应用预览 + 通知父级记录选择 */
const handleThemeSelect = (mode: ThemeMode): void => {
  setMode(mode);
  emit('update:theme', mode);
};

/** 切换语言：即时预览（不持久化） + 通知父级记录选择 */
const handleLanguageChange = (lang: Language): void => {
  setLocale(lang, { persist: false });
  emit('update:language', lang);
};
</script>

<template>
  <div class="step">
    <h3 class="step__title">{{ t('onboarding.steps.appearance.title') }}</h3>
    <p class="step__desc">{{ t('onboarding.steps.appearance.description') }}</p>

    <!-- 主题色块卡片 -->
    <div class="theme-grid">
      <button
        v-for="opt in THEME_OPTIONS"
        :key="opt.key"
        class="theme-card"
        :class="[`theme-card--${opt.key}`, { 'theme-card--active': theme === opt.key }]"
        type="button"
        @click="handleThemeSelect(opt.key)"
      >
        <!-- 迷你预览窗口 -->
        <div class="theme-card__preview" :class="`theme-card__preview--${opt.key}`">
          <div class="preview-bar">
            <span class="preview-dot" />
            <span class="preview-dot" />
            <span class="preview-dot" />
          </div>
          <div class="preview-body">
            <div class="preview-line preview-line--title" />
            <div class="preview-line preview-line--text" />
            <div class="preview-line preview-line--text preview-line--short" />
            <div class="preview-btn" />
          </div>
        </div>

        <span class="theme-card__label">{{ t('settings.theme.' + opt.key + '.label') }}</span>
        <span class="theme-card__desc">{{ t('settings.theme.' + opt.key + '.description') }}</span>

        <!-- 选中标记 -->
        <span class="theme-card__check" :class="{ 'theme-card__check--visible': theme === opt.key }">
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
            <path d="M5 13l4 4L19 7" />
          </svg>
        </span>
      </button>
    </div>

    <!-- 语言选择 -->
    <div class="lang-section">
      <label class="lang-section__label">{{ t('onboarding.steps.appearance.languageLabel') }}</label>
      <div class="lang-section__dropdown">
        <ObDropdown
          :model-value="language"
          :options="SUPPORTED_LOCALES.map((l) => ({ value: l.key, label: l.label }))"
          @update:model-value="handleLanguageChange($event as Language)"
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

/* ── 主题色块卡片 ────────────────────────────────────────────────────── */
.theme-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0.75rem;
  margin-bottom: 1.5rem;
}

.theme-card {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.5rem;
  padding: 0.75rem;
  border: 1px solid var(--fluen-hairline);
  border-radius: 12px;
  background: var(--fluen-hover);
  cursor: pointer;
  transition: all 0.2s ease;
  font-family: var(--fluen-font-sans);
}

.theme-card:hover {
  border-color: var(--fluen-muted);
}

.theme-card--active {
  border-color: var(--fluen-accent);
  background: var(--fluen-info-bg);
}

/* ── 迷你预览 ────────────────────────────────────────────────────────── */
.theme-card__preview {
  width: 100%;
  border-radius: 8px;
  overflow: hidden;
  border: 1px solid transparent;
}

/* 浅色预览 */
.theme-card__preview--light {
  background: #ffffff;
  border-color: #e5e7eb;
}

/* 深色预览 */
.theme-card__preview--dark {
  background: #0a0a0a;
  border-color: #2a2a2a;
}

.preview-bar {
  display: flex;
  gap: 4px;
  padding: 8px 10px;
}

.theme-card__preview--light .preview-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #d1d5db;
}

.theme-card__preview--dark .preview-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #3a3a3a;
}

.preview-body {
  padding: 0 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 5px;
}

.preview-line {
  height: 5px;
  border-radius: 3px;
}

.preview-line--title {
  width: 60%;
  height: 7px;
}

.preview-line--text {
  width: 100%;
}

.preview-line--short {
  width: 40%;
}

/* 浅色预览内容 */
.theme-card__preview--light .preview-line--title {
  background: #0a0a0a;
}
.theme-card__preview--light .preview-line--text {
  background: #e5e7eb;
}
.theme-card__preview--light .preview-btn {
  width: 50px;
  height: 12px;
  margin-top: 4px;
  border-radius: 9999px;
  background: #0a0a0a;
}

/* 深色预览内容 */
.theme-card__preview--dark .preview-line--title {
  background: #ffffff;
}
.theme-card__preview--dark .preview-line--text {
  background: #2a2a2a;
}
.theme-card__preview--dark .preview-btn {
  width: 50px;
  height: 12px;
  margin-top: 4px;
  border-radius: 9999px;
  background: #3b82f6;
}

/* ── 卡片文字 ────────────────────────────────────────────────────────── */
.theme-card__label {
  font-size: 0.9rem;
  font-weight: 500;
  color: var(--fluen-charcoal);
}

.theme-card__desc {
  font-size: 0.72rem;
  color: var(--fluen-stone);
  text-align: center;
  line-height: 1.4;
}

.theme-card__check {
  position: absolute;
  top: 0.5rem;
  right: 0.5rem;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--fluen-accent);
  color: var(--fluen-on-accent);
  opacity: 0;
  transition: opacity 0.2s ease;
}

.theme-card__check--visible {
  opacity: 1;
}

/* ── 语言选择 ────────────────────────────────────────────────────────── */
.lang-section {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.5rem;
}

.lang-section__label {
  font-family: var(--fluen-font-sans);
  font-size: 0.8rem;
  color: var(--fluen-stone);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.lang-section__dropdown {
  width: 100%;
  max-width: 240px;
}
</style>
