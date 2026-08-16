<script setup lang="ts">
/**
 * OutlinePanel — 文章大纲面板（Word 风格）。
 *
 * 功能：
 *   - 展示 `main.md` 的 1-6 级标题树
 *   - 按层级差异化字号/字重（H1 最大最粗，H6 最小最细）
 *   - 节点支持折叠/展开（仅含子节点的节点显示折叠按钮）
 *   - 标题栏提供「全部折叠」「全部展开」「新建章节」操作
 *   - 点击标题跳转到编辑器对应位置
 *   - 节点 hover 显示 ✎（重命名）和 +（新建子标题）
 *   - 无项目时展示空状态占位
 *
 * 数据来源：`useOutline` composable（单例），自动监听项目变化。
 * 树形渲染通过 `OutlineNodeItem` 递归组件实现，支持无限层级。
 */
import { ref, nextTick } from 'vue';
import { useOutline } from '../../../composables/useOutline';
import { useProject } from '../../../../../composables/useProject';
import { useI18n } from '../../../../../i18n';
import OutlineNodeItem from './OutlineNodeItem.vue';
import type { OutlineNode } from '../../../composables/outlineParser';

const { t } = useI18n();
const {
  outline,
  hasOutline,
  hasProject,
  isSaving,
  jumpTo,
  renameNode,
  insertChildHeading,
  expandAll,
  collapseAll,
} = useOutline();
const { createSection } = useProject();

// ── 新建章节状态 ────────────────────────────────────────────────────

/** 是否显示新建章节输入框。 */
const _showAddSection = ref(false);
/** 新建章节输入值。 */
const _addSectionValue = ref('');
/** 新建章节输入框引用。 */
const _addSectionInputRef = ref<HTMLInputElement | null>(null);

// ── 新建章节 ────────────────────────────────────────────────────────

/** 显示新建章节输入框。 */
async function showAddSection(): Promise<void> {
  _showAddSection.value = true;
  _addSectionValue.value = '';
  await nextTick();
  _addSectionInputRef.value?.focus();
}

/** 确认新建章节。 */
async function confirmAddSection(): Promise<void> {
  const trimmed = _addSectionValue.value.trim();
  if (trimmed) {
    await createSection(trimmed);
  }
  _showAddSection.value = false;
}

/** 取消新建章节。 */
function cancelAddSection(): void {
  _showAddSection.value = false;
}

// ── 节点操作 ────────────────────────────────────────────────────────

/** 处理节点重命名。 */
async function handleRename(node: OutlineNode, newTitle: string): Promise<void> {
  await renameNode(node, newTitle);
}

/** 处理新建子标题。 */
async function handleAddChild(node: OutlineNode, title: string): Promise<void> {
  await insertChildHeading(node, title);
}
</script>

<template>
  <div class="outline-panel">
    <!-- ── 标题栏 ────────────────────────────────────────────────────── -->
    <div class="outline-panel__header">
      <span class="outline-panel__title">{{ t('main.sidebar.outline.title') }}</span>
      <div class="outline-panel__tools">
        <button
          v-if="hasOutline"
          class="outline-panel__tool-btn"
          :title="t('main.sidebar.outline.collapseAll')"
          @click="collapseAll"
        >
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M5 8l7 7 7-7" />
            <path d="M5 4h14" />
          </svg>
        </button>
        <button
          v-if="hasOutline"
          class="outline-panel__tool-btn"
          :title="t('main.sidebar.outline.expandAll')"
          @click="expandAll"
        >
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M5 16l7-7 7 7" />
            <path d="M5 4h14" />
          </svg>
        </button>
        <button
          class="outline-panel__add-btn"
          :class="{ 'outline-panel__add-btn--disabled': !hasProject || isSaving }"
          :disabled="!hasProject || isSaving"
          :title="t('main.sidebar.outline.addSection')"
          @click="showAddSection"
        >
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
            <path d="M12 5v14M5 12h14" />
          </svg>
        </button>
      </div>
    </div>

    <!-- ── 新建章节输入框 ────────────────────────────────────────────── -->
    <div v-if="_showAddSection" class="outline-panel__add-section">
      <input
        ref="_addSectionInputRef"
        v-model="_addSectionValue"
        class="outline-panel__add-input"
        type="text"
        :placeholder="t('main.sidebar.outline.sectionNamePlaceholder')"
        :disabled="isSaving"
        @keydown.enter="confirmAddSection"
        @keydown.esc="cancelAddSection"
        @blur="confirmAddSection"
      />
    </div>

    <!-- ── 大纲列表（递归渲染） ──────────────────────────────────────── -->
    <ul v-if="hasOutline" class="outline-panel__body">
      <OutlineNodeItem
        v-for="node in outline"
        :key="`${node.sectionId ?? ''}-${node.line}`"
        :node="node"
        @navigate="jumpTo"
        @rename="handleRename"
        @add-child="handleAddChild"
      />
    </ul>

    <!-- ── 空状态 ────────────────────────────────────────────────────── -->
    <div v-else class="outline-panel__empty">
      <svg viewBox="0 0 24 24" width="32" height="32" fill="none" stroke="currentColor" stroke-width="1.5">
        <path d="M4 6h16M4 12h12M4 18h8" />
      </svg>
      <p class="outline-panel__empty-text">
        {{ t('main.sidebar.outline.noProject') }}
      </p>
    </div>
  </div>
</template>

<style scoped>
.outline-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

/* ── 标题栏 ────────────────────────────────────────────────────────── */
.outline-panel__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 10px 8px;
  flex-shrink: 0;
}

.outline-panel__title {
  font-family: var(--fluen-font-sans);
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--fluen-stone);
}

.outline-panel__tools {
  display: flex;
  align-items: center;
  gap: 1px;
}

/* 工具按钮（折叠/展开全部）—— 极简，无背景，hover 才显现 */
.outline-panel__tool-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
  border-radius: 5px;
  transition: background 0.14s ease, color 0.14s ease;
}

.outline-panel__tool-btn:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

/* 新建章节按钮（强调色） */
.outline-panel__add-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border: none;
  background: var(--fluen-accent);
  color: var(--fluen-on-accent);
  cursor: pointer;
  border-radius: 5px;
  transition: opacity 0.14s ease;
  margin-left: 3px;
}

.outline-panel__add-btn:hover:not(:disabled) {
  opacity: 0.88;
}

.outline-panel__add-btn--disabled,
.outline-panel__add-btn:disabled {
  background: var(--fluen-hover);
  color: var(--fluen-stone);
  cursor: not-allowed;
}

/* ── 新建章节输入框 ────────────────────────────────────────────────── */
.outline-panel__add-section {
  padding: 2px 8px 6px;
  flex-shrink: 0;
}

.outline-panel__add-input {
  width: 100%;
  padding: 5px 10px;
  border: 1px solid var(--fluen-accent);
  border-radius: 6px;
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  outline: none;
}

.outline-panel__add-input:disabled {
  opacity: 0.6;
}

/* ── 大纲列表 ──────────────────────────────────────────────────────── */
.outline-panel__body {
  flex: 1;
  overflow-y: auto;
  list-style: none;
  padding: 2px 6px 14px;
  margin: 0;
}

/* ── 空状态 ────────────────────────────────────────────────────────── */
.outline-panel__empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  color: var(--fluen-stone);
}

.outline-panel__empty-text {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 12.5px;
  color: var(--fluen-stone);
}
</style>
