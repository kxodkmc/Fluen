<script setup lang="ts">
/**
 * OutlinePanel — 文章大纲面板（壳）。
 *
 * 职责：标题栏（全部折叠 / 全部展开 / 新建章节）、空态与"新建章节"输入框。
 * 大纲列表的渲染与结构操作交给 `OutlineTree` / `OutlineRow`，数据与折叠/编辑状态
 * 由 `useOutline`（模块门面）统一提供。
 *
 * 新建章节仍走后端（需生成 `sec-*.md`），外层加 dirty 守卫：编辑器脏时先保存再创建，
 * 避免返回的 `main_md` 覆盖未保存缓冲。
 */
import { ref, nextTick } from 'vue';
import { useOutline } from '../../../composables/outline';
import { useProject } from '../../../../../composables/useProject';
import { useFluenEditor } from '../../editor/composables/useFluenEditor';
import { useI18n } from '../../../../../i18n';
import OutlineTree from './OutlineTree.vue';
import { CollapseAll, ExpandAll, Plus, ListIcon } from './icons';

const { t } = useI18n();
const { outline, hasOutline, hasProject, isSaving, expandAll, collapseAll, jumpTo } = useOutline();
const { createSection } = useProject();

// ── 新建章节状态 ────────────────────────────────────────────────────

/** 是否显示新建章节输入框。 */
const _showAddSection = ref(false);
/** 新建章节输入值。 */
const _addSectionValue = ref('');
/** 新建章节输入框引用。 */
const _addSectionInputRef = ref<HTMLInputElement | null>(null);

/** 显示新建章节输入框。 */
async function showAddSection(): Promise<void> {
  _showAddSection.value = true;
  _addSectionValue.value = '';
  await nextTick();
  _addSectionInputRef.value?.focus();
}

/** 确认新建章节：编辑器脏时先保存，成功后再创建，防止覆盖未保存内容。 */
async function confirmAddSection(): Promise<void> {
  const trimmed = _addSectionValue.value.trim();
  _showAddSection.value = false;
  if (!trimmed) return;

  const editor = useFluenEditor();
  if (editor.isDirty.value && !(await editor.save())) return;
  await createSection(trimmed);
}

/** 取消新建章节。 */
function cancelAddSection(): void {
  _showAddSection.value = false;
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
          <CollapseAll :size="14" />
        </button>
        <button
          v-if="hasOutline"
          class="outline-panel__tool-btn"
          :title="t('main.sidebar.outline.expandAll')"
          @click="expandAll"
        >
          <ExpandAll :size="14" />
        </button>
        <button
          class="outline-panel__add-btn"
          :class="{ 'outline-panel__add-btn--disabled': !hasProject || isSaving }"
          :disabled="!hasProject || isSaving"
          :title="t('main.sidebar.outline.addSection')"
          @click="showAddSection"
        >
          <Plus :size="14" :stroke-width="2.2" />
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

    <!-- ── 大纲列表（扁平渲染） ─────────────────────────────────────── -->
    <div v-if="hasOutline" class="outline-panel__body">
      <OutlineTree :headings="outline" @navigate="jumpTo" />
    </div>

    <!-- ── 空状态 ────────────────────────────────────────────────────── -->
    <div v-else class="outline-panel__empty">
      <ListIcon :size="32" />
      <p class="outline-panel__empty-text">
        {{ hasProject ? t('main.sidebar.outline.empty') : t('main.sidebar.outline.noProject') }}
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