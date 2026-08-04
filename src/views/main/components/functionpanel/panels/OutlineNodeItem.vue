<script setup lang="ts">
/**
 * OutlineNodeItem — 大纲节点递归渲染组件。
 *
 * 通过自引用实现无限层级递归，避免模板中手动嵌套。
 * 每个节点渲染为可点击的标题行，缩进由 CSS 变量 `--indent` 控制。
 *
 * 交互：
 *   - 点击标题行 → 跳转到编辑器对应位置
 *   - hover 显示 ✎（重命名）和 +（新建子标题，所有有 sectionId 的节点）
 *   - 重命名模式下显示内联输入框
 *   - 新建子标题模式下在节点下方显示内联输入框
 */
import { ref, nextTick, computed } from 'vue';
import type { OutlineNode } from '../../../composables/outlineParser';
import { useOutline } from '../../../composables/useOutline';

const props = defineProps<{
  /** 当前节点。 */
  node: OutlineNode;
}>();

const { activeLine } = useOutline();

/** 当前节点是否为活动节点（光标所在行匹配）。 */
const isActive = computed(() => activeLine.value === props.node.line);

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

/** 节点 CSS class。 */
function nodeClass(): Record<string, boolean> {
  return {
    [`outline-node--h${props.node.level}`]: true,
    'outline-node--active': isActive.value,
  };
}

/** 缩进样式。 */
function indentStyle(): Record<string, string> {
  return { '--indent': `${(props.node.level - 1) * 12}px` };
}

/** 子标题输入框缩进样式。 */
function childIndentStyle(): Record<string, string> {
  return { '--indent': `${props.node.level * 12}px` };
}

// ── 交互处理 ────────────────────────────────────────────────────────

/** 点击节点，触发跳转。 */
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
  <div>
    <!-- 当前节点 -->
    <div class="outline-node" :style="indentStyle()">
      <!-- 正常模式 -->
      <template v-if="!_renaming">
        <button
          class="outline-node__btn"
          :class="nodeClass()"
          @click="handleClick"
        >
          <span class="outline-node__marker">H{{ node.level }}</span>
          <span class="outline-node__text">{{ node.text }}</span>
        </button>
        <!-- hover 操作按钮 -->
        <div v-if="isEditable()" class="outline-node__actions">
          <button
            class="outline-node__action"
            title="重命名"
            @click.stop="startRename"
          >
            <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
              <path d="M12 20h9" />
              <path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z" />
            </svg>
          </button>
          <button
            v-if="canAddChild()"
            class="outline-node__action"
            title="新建子标题"
            @click.stop="startAddChild"
          >
            <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
              <path d="M12 5v14M5 12h14" />
            </svg>
          </button>
        </div>
      </template>

      <!-- 重命名模式 -->
      <div v-else class="outline-node__edit">
        <span class="outline-node__marker">H{{ node.level }}</span>
        <input
          ref="_renameInputRef"
          v-model="_renameValue"
          class="outline-node__input"
          type="text"
          @keydown.enter="confirmRename"
          @keydown.esc="cancelRename"
          @blur="confirmRename"
        />
      </div>
    </div>

    <!-- 新建子标题输入框 -->
    <div
      v-if="_addingChild"
      class="outline-node__add-child"
      :style="childIndentStyle()"
    >
      <input
        ref="_childInputRef"
        v-model="_childValue"
        class="outline-node__input"
        type="text"
        placeholder="输入子标题名…"
        @keydown.enter="confirmAddChild"
        @keydown.esc="cancelAddChild"
        @blur="cancelAddChild"
      />
    </div>

    <!-- 递归渲染子节点 -->
    <OutlineNodeItem
      v-for="child in node.children"
      :key="`${child.sectionId ?? ''}-${child.line}`"
      :node="child"
      @navigate="emit('navigate', $event)"
      @rename="(n, t) => emit('rename', n, t)"
      @add-child="(n, t) => emit('add-child', n, t)"
    />
  </div>
</template>

<style scoped>
.outline-node {
  display: flex;
  align-items: center;
  padding-left: var(--indent, 0px);
  position: relative;
}

.outline-node__btn {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: 1;
  min-width: 0;
  padding: 3px 6px;
  border: none;
  background: transparent;
  color: var(--fluen-slate);
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  line-height: 1.4;
  text-align: left;
  cursor: pointer;
  border-radius: 4px;
  transition: background 0.15s ease, color 0.15s ease;
}

.outline-node__btn:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

/* 活动节点高亮（光标所在标题行） */
.outline-node__btn.outline-node--active {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
  box-shadow: inset 2px 0 0 var(--fluen-accent);
}

.outline-node__marker {
  flex-shrink: 0;
  font-size: 10px;
  font-weight: 600;
  color: var(--fluen-stone);
  min-width: 22px;
  text-align: center;
}

.outline-node--h1 .outline-node__text {
  font-weight: 600;
  font-size: 13px;
}

.outline-node--h2 .outline-node__text {
  font-weight: 500;
  font-size: 13px;
}

.outline-node--h3 .outline-node__text,
.outline-node--h4 .outline-node__text,
.outline-node--h5 .outline-node__text,
.outline-node--h6 .outline-node__text {
  font-weight: 400;
  font-size: 12px;
  color: var(--fluen-stone);
}

.outline-node__text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* hover 操作按钮 */
.outline-node__actions {
  display: flex;
  gap: 2px;
  flex-shrink: 0;
  opacity: 0;
  transition: opacity 0.15s ease;
}

.outline-node:hover .outline-node__actions,
.outline-node:focus-within .outline-node__actions {
  opacity: 1;
}

.outline-node__action {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
  border-radius: 4px;
  transition: background 0.15s ease, color 0.15s ease;
}

.outline-node__action:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

/* 重命名模式 */
.outline-node__edit {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: 1;
  padding: 2px 6px;
}

.outline-node__input {
  flex: 1;
  min-width: 0;
  padding: 2px 6px;
  border: 1px solid var(--fluen-accent);
  border-radius: 4px;
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  outline: none;
}

/* 新建子标题输入框 */
.outline-node__add-child {
  display: flex;
  padding: 2px 6px;
  padding-left: var(--indent, 0px);
}

.outline-node__add-child .outline-node__input {
  flex: 1;
}
</style>
