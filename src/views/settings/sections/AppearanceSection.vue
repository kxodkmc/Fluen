<script setup lang="ts">
/**
 * AppearanceSection — 外观设置分区。
 *
 * 提供主题模式选择（浅色 / 深色），实时预览并持久化到 AppConfig。
 * 复用 useTheme composable 管理主题状态。
 */
import { useTheme } from '../../../theme';
import { useI18n } from '../../../i18n';
import type { ThemeMode } from '../../../types/app';

const { t } = useI18n();
const { currentMode, setMode } = useTheme();

/** 主题选项列表。 */
const themeOptions: { key: ThemeMode }[] = [
  { key: 'light' },
  { key: 'dark' },
];

/** 选择主题：立即应用 + 持久化（useTheme 内部处理）。 */
function handleThemeSelect(mode: ThemeMode): void {
  setMode(mode);
}
</script>

<template>
  <div class="section">
    <h2 class="section__title">{{ t('settings.sections.appearance') }}</h2>
    <p class="section__desc">{{ t('settings.appearance.description') }}</p>

    <!-- 主题色块卡片 -->
    <div class="theme-grid">
      <button
        v-for="opt in themeOptions"
        :key="opt.key"
        class="theme-card"
        :class="[`theme-card--${opt.key}`, { 'theme-card--active': currentMode === opt.key }]"
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
        <span class="theme-card__check" :class="{ 'theme-card__check--visible': currentMode === opt.key }">
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
            <path d="M5 13l4 4L19 7" />
          </svg>
        </span>
      </button>
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

/* ── 主题色块卡片 ────────────────────────────────────────────────────── */
.theme-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0.75rem;
  max-width: 480px;
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
  background: var(--fluen-surface);
  cursor: pointer;
  transition: all 0.2s ease;
  font-family: var(--fluen-font-sans);
}

.theme-card:hover {
  border-color: var(--fluen-stone);
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

.theme-card__preview--light {
  background: #ffffff;
  border-color: #e5e7eb;
}

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
</style>
