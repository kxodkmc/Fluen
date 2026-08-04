<script setup lang="ts">
/**
 * OutlinePanel — 文章大纲面板。
 *
 * 功能：
 *   - 展示 `.temp.md` 的 1-6 级标题树
 *   - 支持按层级范围筛选（H1-H6 切换按钮组）
 *   - 点击标题跳转到编辑器对应位置
 *   - 标题栏 [+] 按钮新建一级标题章节
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
  minLevel,
  maxLevel,
  isSaving,
  jumpTo,
  setMinLevel,
  setMaxLevel,
  renameNode,
  insertChildHeading,
} = useOutline();
const { createSection } = useProject();

/** 层级切换按钮的选项。 */
const LEVEL_OPTIONS = [1, 2, 3, 4, 5, 6] as const;

// ── 新建章节状态 ────────────────────────────────────────────────────

/** 是否显示新建章节输入框。 */
const _showAddSection = ref(false);
/** 新建章节输入值。 */
const _addSectionValue = ref('');
/** 新建章节输入框引用。 */
const _addSectionInputRef = ref<HTMLInputElement | null>(null);

// ── 层级筛选 ────────────────────────────────────────────────────────

/** 某层级是否启用（在 minLevel ~ maxLevel 范围内）。 */
function isLevelActive(level: number): boolean {
  return level >= minLevel.value && level <= maxLevel.value;
}

/** 切换某层级的显示/隐藏。 */
function toggleLevel(level: number): void {
  if (isLevelActive(level)) {
    if (minLevel.value === maxLevel.value) return;
    if (level === minLevel.value) {
      setMinLevel(level + 1);
    } else if (level === maxLevel.value) {
      setMaxLevel(level - 1);
    }
  } else {
    if (level < minLevel.value) {
      setMinLevel(level);
    } else {
      setMaxLevel(level);
    }
  }
}

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
      <button
        class="outline-panel__add-btn"
        :class="{ 'outline-panel__add-btn--disabled': !hasProject || isSaving }"
        :disabled="!hasProject || isSaving"
        :title="t('main.sidebar.outline.addSection')"
        @click="showAddSection"
      >
        <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
          <path d="M12 5v14M5 12h14" />
        </svg>
      </button>
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

    <!-- ── 筛选按钮组 ────────────────────────────────────────────────── -->
    <div v-if="hasOutline" class="outline-panel__filter">
      <button
        v-for="level in LEVEL_OPTIONS"
        :key="level"
        class="filter-btn"
        :class="{ 'filter-btn--active': isLevelActive(level) }"
        :title="t('main.sidebar.outline.levelHint', { level })"
        @click="toggleLevel(level)"
      >
        H{{ level }}
      </button>
    </div>

    <!-- ── 大纲列表（递归渲染） ──────────────────────────────────────── -->
    <div v-if="hasOutline" class="outline-panel__body">
      <OutlineNodeItem
        v-for="node in outline"
        :key="`${node.sectionId ?? ''}-${node.line}`"
        :node="node"
        @navigate="jumpTo"
        @rename="handleRename"
        @add-child="handleAddChild"
      />
    </div>

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
  padding: 10px 12px;
  flex-shrink: 0;
}

.outline-panel__title {
  font-family: var(--fluen-font-sans);
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--fluen-slate);
}

.outline-panel__add-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  background: var(--fluen-accent);
  color: var(--fluen-on-accent);
  cursor: pointer;
  border-radius: 6px;
  transition: all 0.15s ease;
  flex-shrink: 0;
}

.outline-panel__add-btn:hover:not(:disabled) {
  opacity: 0.85;
}

.outline-panel__add-btn--disabled,
.outline-panel__add-btn:disabled {
  background: var(--fluen-hover);
  color: var(--fluen-stone);
  cursor: not-allowed;
}

/* ── 新建章节输入框 ────────────────────────────────────────────────── */
.outline-panel__add-section {
  padding: 0 12px 8px;
  flex-shrink: 0;
}

.outline-panel__add-input {
  width: 100%;
  padding: 4px 8px;
  border: 1px solid var(--fluen-accent);
  border-radius: 4px;
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  outline: none;
}

.outline-panel__add-input:disabled {
  opacity: 0.6;
}

/* ── 筛选按钮组 ────────────────────────────────────────────────────── */
.outline-panel__filter {
  display: flex;
  gap: 4px;
  padding: 0 12px 8px;
  flex-shrink: 0;
}

.filter-btn {
  height: 22px;
  min-width: 30px;
  padding: 0 6px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 4px;
  background: var(--fluen-canvas);
  color: var(--fluen-stone);
  font-family: var(--fluen-font-sans);
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
}

.filter-btn:hover {
  border-color: var(--fluen-accent);
  color: var(--fluen-ink);
}

.filter-btn--active {
  background: var(--fluen-ink);
  border-color: var(--fluen-ink);
  color: var(--fluen-on-accent);
}

/* ── 大纲列表 ──────────────────────────────────────────────────────── */
.outline-panel__body {
  flex: 1;
  overflow-y: auto;
  padding: 0 8px 12px;
}

/* ── 空状态 ────────────────────────────────────────────────────────── */
.outline-panel__empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--fluen-stone);
}

.outline-panel__empty-text {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  color: var(--fluen-stone);
}
</style>
