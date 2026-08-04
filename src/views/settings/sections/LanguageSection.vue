<script setup lang="ts">
/**
 * LanguageSection — 语言设置分区。
 *
 * 提供界面语言切换，即时预览并持久化到 AppConfig。
 * 复用 useI18n composable 管理语言状态。
 */
import { useI18n, SUPPORTED_LOCALES } from '../../../i18n';
import type { Language } from '../../../i18n/types';

const { t, locale, setLocale } = useI18n();

/** 选择语言：即时切换 + 持久化（useI18n 内部处理）。 */
function handleLanguageSelect(lang: Language): void {
  setLocale(lang);
}
</script>

<template>
  <div class="section">
    <h2 class="section__title">{{ t('settings.sections.language') }}</h2>
    <p class="section__desc">{{ t('settings.language.description') }}</p>

    <!-- 语言列表 -->
    <div class="lang-list">
      <button
        v-for="l in SUPPORTED_LOCALES"
        :key="l.key"
        class="lang-item"
        :class="{ 'lang-item--active': locale === l.key }"
        type="button"
        @click="handleLanguageSelect(l.key)"
      >
        <span class="lang-item__label">{{ l.label }}</span>
        <span class="lang-item__key">{{ l.key }}</span>
        <span class="lang-item__check" :class="{ 'lang-item__check--visible': locale === l.key }">
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

/* ── 语言列表 ────────────────────────────────────────────────────────── */
.lang-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  max-width: 480px;
}

.lang-item {
  position: relative;
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 12px 16px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 12px;
  background: var(--fluen-surface);
  cursor: pointer;
  transition: all 0.2s ease;
  font-family: var(--fluen-font-sans);
  text-align: left;
}

.lang-item:hover {
  border-color: var(--fluen-stone);
}

.lang-item--active {
  border-color: var(--fluen-accent);
  background: var(--fluen-info-bg);
}

.lang-item__label {
  flex: 1;
  font-size: 0.9rem;
  font-weight: 500;
  color: var(--fluen-charcoal);
}

.lang-item__key {
  font-size: 0.75rem;
  color: var(--fluen-stone);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.lang-item__check {
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

.lang-item__check--visible {
  opacity: 1;
}
</style>
