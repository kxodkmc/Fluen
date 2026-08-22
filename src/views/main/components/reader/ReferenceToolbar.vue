<template>
  <div class="reference-toolbar">
    <!-- 左侧：标题 -->
    <div class="toolbar-section toolbar-title-section">
      <span class="toolbar-title" :title="title">{{ title }}</span>
    </div>

    <!-- 右侧：控制 -->
    <div class="toolbar-section toolbar-controls">
      <!-- 字号 -->
      <ToolbarMenu
        :label="t('reader.toolbar.fontSize')"
        :options="fontSizeOptions"
        :model-value="options.fontSize"
        @update:model-value="emit('font-size-change', $event)"
      />

      <!-- 行距 -->
      <ToolbarMenu
        :label="t('reader.toolbar.lineHeight')"
        :options="lineHeightOptions"
        :model-value="options.lineHeight"
        @update:model-value="emit('line-height-change', $event)"
      />

      <!-- 标记 -->
      <button
        class="toolbar-btn"
        :class="{ active: marksActive }"
        :title="t('reader.toolbar.marks')"
        @click="emit('marks-toggle')"
      >
        <Pencil :size="14" />
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * ReferenceToolbar —— 文献阅读器工具栏。
 *
 * 左侧文献标题，右侧字号/行距下拉与标记开关。
 * 深浅色跟随应用主题，不提供切换；关闭由标签页负责。
 * 所有操作通过 emit 上抛，不直接修改状态。
 */

import { computed } from 'vue';
import { useI18n } from '../../../../i18n';
import type {
  ReaderFontSize,
  ReaderLineHeight,
  ReaderOptions,
} from '../../../../types/reader';
import { Pencil } from '../functionpanel/panels/icons';
import ToolbarMenu from './ToolbarMenu.vue';

defineProps<{
  /** 文献标题。 */
  title: string;
  /** 当前阅读器选项。 */
  options: ReaderOptions;
  /** 标记面板是否激活。 */
  marksActive?: boolean;
}>();

const emit = defineEmits<{
  'font-size-change': [size: ReaderFontSize];
  'line-height-change': [lh: ReaderLineHeight];
  'marks-toggle': [];
}>();

const { t } = useI18n();

const fontSizeOptions = computed(() => [
  { value: 14 as ReaderFontSize, label: t('reader.toolbar.fontSizeSmall') },
  { value: 16 as ReaderFontSize, label: t('reader.toolbar.fontSizeMedium') },
  { value: 18 as ReaderFontSize, label: t('reader.toolbar.fontSizeLarge') },
  { value: 20 as ReaderFontSize, label: t('reader.toolbar.fontSizeXLarge') },
]);

const lineHeightOptions = computed(() => [
  { value: 1.5 as ReaderLineHeight, label: t('reader.toolbar.lineHeightCompact') },
  { value: 1.7 as ReaderLineHeight, label: t('reader.toolbar.lineHeightComfortable') },
  { value: 1.9 as ReaderLineHeight, label: t('reader.toolbar.lineHeightLoose') },
]);
</script>

<style scoped>
.reference-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  height: 44px;
  padding: 0 12px;
  background: var(--fluen-canvas);
  border-bottom: 1px solid var(--fluen-hairline);
  flex-shrink: 0;
}

.toolbar-section {
  display: flex;
  align-items: center;
  gap: 6px;
}

.toolbar-title-section {
  flex: 1;
  min-width: 0;
}

.toolbar-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--fluen-ink);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.toolbar-controls {
  flex-shrink: 0;
}

/* 标记开关：幽灵按钮，激活时仅文字变强调色 */
.toolbar-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  padding: 0;
  border: none;
  background: transparent;
  border-radius: 6px;
  color: var(--fluen-stone);
  cursor: pointer;
  transition: color 0.14s ease;
}

.toolbar-btn:hover {
  color: var(--fluen-ink);
}

.toolbar-btn.active {
  color: var(--fluen-accent);
}
</style>
