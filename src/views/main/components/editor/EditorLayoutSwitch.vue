<script setup lang="ts">
/**
 * EditorLayoutSwitch — 编辑器视图模式切换控件（编辑区右上角浮动）。
 *
 * 提供四种视图模式（见 `EditorLayoutMode`）：
 *   - source   仅源码：只显示 MD 编辑器
 *   - live     半预览：同一编辑器开启实时渲染（WYSIWYG 所见即所得，光标处
 *              露出原始语法），由 livePreview 扩展实现
 *   - wysiwyg  预览编辑：TipTap 实验视图（独立 WYSIWYG 编辑面）
 *   - preview  仅渲染：只显示 HTML 预览（默认）
 *
 * 状态由 `useMainLayout` 单例统一管理（`editorLayout`），
 * 与快捷键（Mod+1/2/3/4，见 MainView 注册）共享同一数据源，
 * 保证 UI 点击与快捷键操作完全同步。
 *
 * 仅在编辑类标签页（ContentPanel 的 content-editor 视图）挂载，
 * 文献/知识库阅读器不显示。
 */
import { computed, inject } from 'vue';
import { MAIN_LAYOUT_KEY } from '../../composables/useMainLayout';
import { useI18n } from '../../../../i18n';
import type { EditorLayoutMode } from '../../types';

const layout = inject(MAIN_LAYOUT_KEY);
const { t } = useI18n();

/** 当前激活的视图模式（未注入布局实例时按预览处理，与 useMainLayout 默认一致）。 */
const active = computed<EditorLayoutMode>(() => layout?.editorLayout.value ?? 'preview');

/** 四个模式按钮：顺序为 源码 → 半预览 → 预览编辑 → 预览（编辑模式相邻）。 */
const modes = computed(() => [
  {
    id: 'source' as const,
    icon: 'M16 18l6-6-6-6M8 6l-6 6 6 6',
    label: t('main.content.view.source'),
  },
  {
    id: 'live' as const,
    // 铅笔（可编辑）+ 基线（排版渲染）：所见即所得的实时编辑
    icon: 'M17 3a2.83 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3zM14 21h8',
    label: t('main.content.view.live'),
  },
  {
    id: 'wysiwyg' as const,
    // 眼睛（渲染观感）+ 笔尖（可编辑）：独立 WYSIWYG 编辑面
    icon: 'M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM17.5 21.5l1-4 4.5-4.5 3 3-4.5 4.5z',
    label: t('main.content.view.wysiwyg'),
  },
  {
    id: 'preview' as const,
    icon: 'M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z',
    label: t('main.content.view.preview'),
  },
]);
</script>

<template>
  <div class="editor-layout-switch" role="group" :aria-label="t('main.content.view.label')">
    <button
      v-for="m in modes"
      :key="m.id"
      class="editor-layout-switch__btn"
      :class="{ 'editor-layout-switch__btn--active': active === m.id }"
      :title="m.label"
      :aria-pressed="active === m.id"
      @click="layout?.setEditorLayout(m.id)"
    >
      <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path :d="m.icon" />
      </svg>
    </button>
  </div>
</template>

<style scoped>
/* 编辑区右上角浮动小组件，半透明悬浮，不遮挡编辑内容交互 */
.editor-layout-switch {
  position: absolute;
  top: 8px;
  right: 12px;
  z-index: 20;
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 2px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-surface);
  box-shadow: var(--fluen-shadow-card, 0 1px 4px rgba(0, 0, 0, 0.12));
  opacity: 0.85;
  transition: opacity 0.15s ease;
}

.editor-layout-switch:hover {
  opacity: 1;
}

.editor-layout-switch__btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 24px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease;
}

.editor-layout-switch__btn:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.editor-layout-switch__btn--active {
  background: var(--fluen-hover);
  color: var(--fluen-accent);
}
</style>
