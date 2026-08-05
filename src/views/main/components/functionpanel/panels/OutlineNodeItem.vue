<script setup lang="ts">
/**
 * OutlineNodeItem — 大纲节点递归渲染组件（Word 风格）。
 *
 * 通过自引用实现无限层级递归。每个节点渲染为一行：
 *   [折叠按钮] [标题文本] [hover 操作按钮]
 *
 * 设计要点：
 *   - 折叠按钮仅在有子节点时显示，叶子节点保留占位以对齐
 *   - 层级通过字号/字重区分（H1 最大最粗，H6 最小最细）
 *   - 缩进由嵌套 `<ul>` 的 padding-left 累加，无需手动计算
 *   - 折叠状态由 `useOutline` 单例管理，跨大纲重新解析保持稳定
 *
 * 交互：
 *   - 点击标题行 → 跳转到编辑器对应位置
 *   - hover 显示 ✎（重命名）和 +（新建子标题）
 *   - 点击折叠按钮 → 切换子节点显示
 */
import { ref, nextTick, computed } from 'vue';
import type { OutlineNode } from '../../../composables/outlineParser';
import { useOutline } from '../../../composables/useOutline';
import { useI18n } from '../../../../../i18n';

const props = defineProps<{
  /** 当前节点。 */
  node: OutlineNode;
}>();

const { t } = useI18n();
const { activeLine, isCollapsed, toggleCollapse } = useOutline();

/** 当前节点是否为活动节点（光标所在行匹配）。 */
const isActive = computed(() => activeLine.value === props.node.line);

/** 是否有子节点。 */
const hasChildren = computed(() => props.node.children.length > 0);

/** 是否处于折叠状态。 */
const collapsed = computed(() => isCollapsed(props.node));

const emit = defineEmits<{
  (e: 'navigate', line: number): void;
  (e: 'rename', node: OutlineNode, newTitle: string): void;
  (e: 'add-child', node: OutlineNode, title: string): void;
}>();

// ── 编辑状态 ────────────────────────────────────────────────────────

/** 是否处于重命名模式。 */
const _renaming = ref(false);
/** 重命名输入值。 */
const _renameValue = ref('');
/** 重命名输入框引用。 */
const _renameInputRef = ref<HTMLInputElement | null>(null);

/** 是否处于新建子标题模式。 */
const _addingChild = ref(false);
/** 新建子标题输入值。 */
const _childValue = ref('');
/** 新建子标题输入框引用。 */
const _childInputRef = ref<HTMLInputElement | null>(null);

// ── 计算属性 ────────────────────────────────────────────────────────

/** 节点是否可编辑（有 sectionId 的节点才能重命名）。 */
function isEditable(): boolean {
  return props.node.sectionId !== null;
}

/** 节点是否可添加子标题（有 sectionId 且层级 < 6）。 */
function canAddChild(): boolean {
  return props.node.sectionId !== null && props.node.level < 6;
}

// ── 交互处理 ────────────────────────────────────────────────────────

/** 点击标题文本，触发跳转。 */
function handleClick(): void {
  if (!_renaming.value) {
    emit('navigate', props.node.line);
  }
}

/** 进入重命名模式。 */
async function startRename(): Promise<void> {
  _renaming.value = true;
  _renameValue.value = props.node.text;
  await nextTick();
  _renameInputRef.value?.focus();
  _renameInputRef.value?.select();
}

/** 确认重命名。 */
function confirmRename(): void {
  const trimmed = _renameValue.value.trim();
  if (trimmed && trimmed !== props.node.text) {
    emit('rename', props.node, trimmed);
  }
  _renaming.value = false;
}

/** 取消重命名。 */
function cancelRename(): void {
  _renaming.value = false;
}

/** 进入新建子标题模式。 */
async function startAddChild(): Promise<void> {
  _addingChild.value = true;
  _childValue.value = '';
  await nextTick();
  _childInputRef.value?.focus();
}

/** 确认新建子标题。 */
function confirmAddChild(): void {
  const trimmed = _childValue.value.trim();
  if (trimmed) {
    emit('add-child', props.node, trimmed);
  }
  _addingChild.value = false;
}

/** 取消新建子标题。 */
function cancelAddChild(): void {
  _addingChild.value = false;
}
</script>

<template>
  <li class="outline-li">
    <!-- 当前节点行 -->
    <div class="outline-row" :class="{ 'outline-row--active': isActive }">
      <!-- 折叠/展开按钮 -->
      <button
        v-if="hasChildren"
        class="outline-toggle"
        :title="collapsed ? t('main.sidebar.outline.expand') : t('main.sidebar.outline.collapse')"
        @click.stop="toggleCollapse(node)"
      >
        <svg viewBox="0 0 16 16" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path v-if="!collapsed" d="M4 6l4 4 4-4" />
          <path v-else d="M6 4l4 4-4 4" />
        </svg>
      </button>
      <span v-else class="outline-toggle outline-toggle--placeholder" aria-hidden="true" />

      <!-- 标题文本 / 重命名输入框 -->
      <div class="outline-content">
        <span
          v-if="!_renaming"
          class="outline-text"
          :class="`outline-text--h${node.level}`"
          :title="node.text"
          @click="handleClick"
        >{{ node.text }}</span>
        <input
          v-else
          ref="_renameInputRef"
          v-model="_renameValue"
          class="outline-input"
          :class="`outline-input--h${node.level}`"
          type="text"
          @keydown.enter="confirmRename"
          @keydown.esc="cancelRename"
          @blur="confirmRename"
        />
      </div>

      <!-- hover 操作按钮 -->
      <div v-if="isEditable() && !_renaming" class="outline-actions">
        <button
          class="outline-action"
          :title="t('main.sidebar.outline.rename')"
          @click.stop="startRename"
        >
          <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12 20h9" />
            <path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z" />
          </svg>
        </button>
        <button
          v-if="canAddChild()"
          class="outline-action"
          :title="t('main.sidebar.outline.addChild')"
          @click.stop="startAddChild"
        >
          <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            <path d="M12 5v14M5 12h14" />
          </svg>
        </button>
      </div>
    </div>

    <!-- 新建子标题输入框 -->
    <div v-if="_addingChild" class="outline-add-child">
      <span class="outline-toggle outline-toggle--placeholder" aria-hidden="true" />
      <input
        ref="_childInputRef"
        v-model="_childValue"
        class="outline-input outline-input--child"
        type="text"
        :placeholder="t('main.sidebar.outline.sectionNamePlaceholder')"
        @keydown.enter="confirmAddChild"
        @keydown.esc="cancelAddChild"
        @blur="cancelAddChild"
      />
    </div>

    <!-- 递归渲染子节点 -->
    <ul v-if="hasChildren && !collapsed" class="outline-nested">
      <OutlineNodeItem
        v-for="child in node.children"
        :key="`${child.sectionId ?? ''}-${child.line}`"
        :node="child"
        @navigate="emit('navigate', $event)"
        @rename="(n, t) => emit('rename', n, t)"
        @add-child="(n, t) => emit('add-child', n, t)"
      />
    </ul>
  </li>
</template>

<style scoped>
.outline-li {
  list-style: none;
}

/* ── 节点行 ─────────────────────────────────────────────────────── */
.outline-row {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 3px 6px;
  border-radius: 6px;
  transition: background 0.14s ease;
}

.outline-row:hover {
  background: var(--fluen-hover);
}

/* 活动节点：仅用淡色背景点缀，不使用竖线 */
.outline-row--active {
  background: color-mix(in srgb, var(--fluen-accent) 10%, transparent);
}

.outline-row--active:hover {
  background: color-mix(in srgb, var(--fluen-accent) 14%, transparent);
}

/* ── 折叠按钮 ───────────────────────────────────────────────────── */
.outline-toggle {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 14px;
  height: 14px;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
  border-radius: 3px;
  flex-shrink: 0;
  opacity: 0.7;
  transition: opacity 0.14s ease, color 0.14s ease;
}

.outline-toggle:hover {
  opacity: 1;
  color: var(--fluen-ink);
}

/* 叶子节点占位（保持对齐，不响应交互） */
.outline-toggle--placeholder {
  cursor: default;
  pointer-events: none;
  opacity: 0;
}

/* ── 标题文本 ───────────────────────────────────────────────────── */
.outline-content {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
}

.outline-text {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  cursor: pointer;
  font-family: var(--fluen-font-sans);
  color: var(--fluen-ink);
  line-height: 1.5;
  letter-spacing: -0.005em;
}

/* 层级样式：清晰的字重梯度，避免过度装饰 */
.outline-text--h1 {
  font-size: 14px;
  font-weight: 600;
  color: var(--fluen-ink);
  margin-top: 6px;
}

.outline-text--h2 {
  font-size: 13px;
  font-weight: 600;
  color: var(--fluen-ink);
}

.outline-text--h3 {
  font-size: 13px;
  font-weight: 500;
  color: var(--fluen-ink);
}

.outline-text--h4 {
  font-size: 12.5px;
  font-weight: 400;
  color: var(--fluen-slate);
}

.outline-text--h5 {
  font-size: 12.5px;
  font-weight: 400;
  color: var(--fluen-slate);
}

.outline-text--h6 {
  font-size: 12.5px;
  font-weight: 400;
  color: var(--fluen-stone);
}

/* 活动节点文本使用强调色 */
.outline-row--active .outline-text {
  color: var(--fluen-accent);
}

/* ── hover 操作按钮 ─────────────────────────────────────────────── */
.outline-actions {
  display: flex;
  gap: 1px;
  flex-shrink: 0;
  opacity: 0;
  transition: opacity 0.14s ease;
}

.outline-row:hover .outline-actions,
.outline-row:focus-within .outline-actions {
  opacity: 1;
}

.outline-action {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
  border-radius: 4px;
  transition: background 0.14s ease, color 0.14s ease;
}

.outline-action:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

/* ── 输入框（重命名 / 新建子标题） ──────────────────────────────── */
.outline-input {
  flex: 1;
  min-width: 0;
  padding: 3px 8px;
  border: 1px solid var(--fluen-accent);
  border-radius: 6px;
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  outline: none;
}

.outline-input--h1 { font-size: 14px; font-weight: 600; }
.outline-input--h2 { font-size: 13px; font-weight: 600; }
.outline-input--h3 { font-size: 13px; font-weight: 500; }

/* 新建子标题输入框行 */
.outline-add-child {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 3px 6px;
}

/* ── 嵌套子列表 ─────────────────────────────────────────────────── */
.outline-nested {
  list-style: none;
  padding-left: 14px;
  margin: 0;
}
</style>
