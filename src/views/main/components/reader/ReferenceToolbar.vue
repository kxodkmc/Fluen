<template>
  <div class="reference-toolbar">
    <!-- 左侧：标题 -->
    <div class="toolbar-section toolbar-title-section">
      <span class="toolbar-title" :title="title">{{ title }}</span>
    </div>

    <!-- 右侧：控制按钮 -->
    <div class="toolbar-section toolbar-controls">
      <!-- 主题切换 -->
      <button
        class="toolbar-btn"
        :class="{ active: options.themeMode === 'dark' }"
        :title="t('reader.toolbar.themeToggle')"
        @click="emit('theme-mode-change', options.themeMode === 'dark' ? 'light' : 'dark')"
      >
        <span class="btn-icon">{{ options.themeMode === 'dark' ? '☀' : '☾' }}</span>
      </button>

      <!-- 字号 -->
      <div class="control-group">
        <span class="control-label">{{ t('reader.toolbar.fontSize') }}</span>
        <div class="btn-group">
          <button
            v-for="opt in fontSizeOptions"
            :key="opt.value"
            class="toolbar-btn toolbar-btn-sm"
            :class="{ active: options.fontSize === opt.value }"
            :title="opt.label"
            @click="emit('font-size-change', opt.value)"
          >
            {{ opt.icon }}
          </button>
        </div>
      </div>

      <!-- 行高 -->
      <div class="control-group">
        <span class="control-label">{{ t('reader.toolbar.lineHeight') }}</span>
        <div class="btn-group">
          <button
            v-for="opt in lineHeightOptions"
            :key="opt.value"
            class="toolbar-btn toolbar-btn-sm"
            :class="{ active: options.lineHeight === opt.value }"
            :title="opt.label"
            @click="emit('line-height-change', opt.value)"
          >
            {{ opt.icon }}
          </button>
        </div>
      </div>

      <!-- 标记 -->
      <button
        class="toolbar-btn"
        :class="{ active: marksActive }"
        :title="t('reader.toolbar.marks')"
        @click="emit('marks-toggle')"
      >
        <span class="btn-icon">✎</span>
      </button>

      <!-- 关闭 -->
      <button
        class="toolbar-btn"
        :title="t('reader.toolbar.close')"
        @click="emit('close')"
      >
        <span class="btn-icon">×</span>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * ReferenceToolbar —— 文献阅读器工具栏。
 *
 * 提供主题切换、字号/行高调整、关闭等控制。
 * 所有操作通过 emit 上抛，不直接修改状态。
 */

import { computed } from 'vue';
import { useI18n } from '../../../../i18n';
import type {
  ReaderFontSize,
  ReaderLineHeight,
  ReaderOptions,
  ReaderThemeMode,
} from '../../../../types/reader';

defineProps<{
  /** 文献标题。 */
  title: string;
  /** 当前阅读器选项。 */
  options: ReaderOptions;
  /** 标记面板是否激活。 */
  marksActive?: boolean;
}>();

const emit = defineEmits<{
  'theme-mode-change': [mode: ReaderThemeMode];
  'font-size-change': [size: ReaderFontSize];
  'line-height-change': [lh: ReaderLineHeight];
  'marks-toggle': [];
  close: [];
}>();

const { t } = useI18n();

const fontSizeOptions = computed(() => [
  { value: 14 as ReaderFontSize, icon: 'S', label: t('reader.toolbar.fontSizeSmall') },
  { value: 16 as ReaderFontSize, icon: 'M', label: t('reader.toolbar.fontSizeMedium') },
  { value: 18 as ReaderFontSize, icon: 'L', label: t('reader.toolbar.fontSizeLarge') },
  { value: 20 as ReaderFontSize, icon: 'XL', label: t('reader.toolbar.fontSizeXLarge') },
]);

const lineHeightOptions = computed(() => [
  { value: 1.5 as ReaderLineHeight, icon: '☱', label: t('reader.toolbar.lineHeightCompact') },
  { value: 1.7 as ReaderLineHeight, icon: '☲', label: t('reader.toolbar.lineHeightComfortable') },
  { value: 1.9 as ReaderLineHeight, icon: '☳', label: t('reader.toolbar.lineHeightLoose') },
]);
</script>

<style scoped>
.reference-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  height: 48px;
  padding: 0 16px;
  background: var(--app-bg, #ffffff);
  border-bottom: 1px solid var(--app-border, #e5e7eb);
  flex-shrink: 0;
}

.toolbar-section {
  display: flex;
  align-items: center;
  gap: 8px;
}

.toolbar-title-section {
  flex: 1;
  min-width: 0;
}

.toolbar-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--app-text, #1a1a2e);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.toolbar-controls {
  flex-shrink: 0;
}

.control-group {
  display: flex;
  align-items: center;
  gap: 6px;
}

.control-label {
  font-size: 11px;
  color: var(--app-text-muted, #6b7280);
  white-space: nowrap;
}

.btn-group {
  display: flex;
  gap: 2px;
}

.toolbar-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 32px;
  height: 32px;
  padding: 0 8px;
  border: 1px solid var(--app-border, #e5e7eb);
  border-radius: 6px;
  background: var(--app-bg, #ffffff);
  color: var(--app-text-muted, #6b7280);
  font-size: 13px;
  cursor: pointer;
  transition: all 0.15s;
}

.toolbar-btn:hover {
  border-color: var(--app-primary, #2563eb);
  color: var(--app-primary, #2563eb);
}

.toolbar-btn.active {
  background: var(--app-primary, #2563eb);
  border-color: var(--app-primary, #2563eb);
  color: #ffffff;
}

.toolbar-btn-sm {
  min-width: 28px;
  height: 28px;
  font-size: 12px;
}

.btn-icon {
  font-size: 16px;
  line-height: 1;
}
</style>
