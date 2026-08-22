<script setup lang="ts">
/**
 * OutlineRow — 大纲扁平行列展示组件（纯展示，只 emit 意图）。
 *
 * 不直接消费单例状态，所有输入经 props 传入、所有交互以事件上抛，由
 * `OutlineTree` 统一接线到 `useOutlineUi` / `useOutline`。
 *
 * 一行结构：
 *   [折叠滑块] [标题文本 / 重命名输入框] [hover 或聚焦时的操作按钮]
 * 缩进由 `node.depth` 映射为 `padding-left`，不再嵌套 `<ul>`。
 * chevron 方向经 CSS `transform: rotate()` 切换，仅可折叠行渲染图标。
 */
import { ref, computed, watch, nextTick } from 'vue';
import type { FlatHeading } from '../../../composables/outline/outlineParser';
import { useI18n } from '../../../../../i18n';
import { Chevron, Plus, Pencil } from './icons';

const { t } = useI18n();

const props = defineProps<{
  /** 当前标题。 */
  node: FlatHeading;
  /** 是否为活动行（光标所在行）。 */
  active: boolean;
  /** 是否折叠。 */
  collapsed: boolean;
  /** 编辑类型（无则不编辑）。 */
  editing: 'rename' | 'child' | null;
  /** 编辑输入值。 */
  editValue: string;
  /** 是否 hover 本行（控制操作按钮显隐）。 */
  hovered: boolean;
  /** 是否可编辑（有章节归属）。 */
  editable: boolean;
}>();

const emit = defineEmits<{
  (e: 'navigate', line: number): void;
  (e: 'toggle-collapse'): void;
  (e: 'start-rename'): void;
  (e: 'start-add-child'): void;
  (e: 'update:editValue', value: string): void;
  (e: 'commit-edit'): void;
  (e: 'cancel-edit'): void;
  (e: 'hover', id: string | null): void;
}>();

const hasChildren = computed(() => props.node.childrenCount > 0);
const canAddChild = computed(() => props.editable && props.node.level < 6);
const isRenaming = computed(() => props.editing === 'rename');
const isAddingChild = computed(() => props.editing === 'child');
const isEditing = computed(() => props.editing !== null);

/** 本行是否聚焦（键盘可达，悬停或聚焦时显示操作按钮）。 */
const focused = ref(false);

/** 编辑输入框引用（重命名 / 新建子标题共用）。 */
const inputRef = ref<HTMLInputElement | null>(null);

// 进入编辑模式时聚焦输入框；重命名默认全选已有标题。
watch(
  () => props.editing,
  async (kind) => {
    if (kind) {
      await nextTick();
      if (kind === 'rename') inputRef.value?.select();
      inputRef.value?.focus();
    }
  },
);

function onInput(e: Event): void {
  emit('update:editValue', (e.target as HTMLInputElement).value);
}

function handleTextClick(): void {
  if (!isEditing.value) emit('navigate', props.node.line);
}
</script>

<template>
  <div
    class="outline-row"
    :class="{ 'outline-row--active': active }"
    :style="{ '--indent': `${node.depth * 14}px` }"
    tabindex="0"
    @mouseenter="emit('hover', node.id)"
    @mouseleave="emit('hover', null)"
    @focus="focused = true"
    @blur="focused = false"
  >
    <!-- 折叠滑块：固定槽位保证文本对齐，仅可折叠行渲染图标 -->
    <button
      v-if="hasChildren"
      class="outline-toggle"
      :class="{ 'outline-toggle--collapsed': collapsed }"
      :title="collapsed ? t('main.sidebar.outline.expand') : t('main.sidebar.outline.collapse')"
      @click.stop="emit('toggle-collapse')"
    >
      <Chevron :size="9" :stroke-width="1.8" />
    </button>
    <span v-else class="outline-toggle outline-toggle--placeholder" aria-hidden="true" />

    <!-- 标题文本 / 重命名输入框 -->
    <div class="outline-content">
      <span
        v-if="!isRenaming"
        class="outline-text"
        :class="`outline-text--h${node.level}`"
        :title="node.text"
        @click="handleTextClick"
      >{{ node.text }}</span>
      <input
        v-else
        ref="inputRef"
        :value="editValue"
        class="outline-input"
        :class="`outline-input--h${node.level}`"
        type="text"
        @input="onInput"
        @keydown.enter="emit('commit-edit')"
        @keydown.esc="emit('cancel-edit')"
        @blur="emit('commit-edit')"
      />
    </div>

    <!-- 操作按钮：仅在 hover / 聚焦时渲染，任一时刻全局仅一份 -->
    <div v-if="(hovered || focused) && !isEditing" class="outline-actions">
      <button
        v-if="editable"
        class="outline-action"
        :title="t('main.sidebar.outline.rename')"
        @click.stop="emit('start-rename')"
      >
        <Pencil :size="12" />
      </button>
      <button
        v-if="canAddChild"
        class="outline-action"
        :title="t('main.sidebar.outline.addChild')"
        @click.stop="emit('start-add-child')"
      >
        <Plus :size="12" />
      </button>
    </div>
  </div>

  <!-- 新建子标题输入行 -->
  <div v-if="isAddingChild" class="outline-add-child" :style="{ '--indent': `${(node.depth + 1) * 14}px` }">
    <input
      ref="inputRef"
      :value="editValue"
      class="outline-input outline-input--child"
      type="text"
      :placeholder="t('main.sidebar.outline.sectionNamePlaceholder')"
      @input="onInput"
      @keydown.enter="emit('commit-edit')"
      @keydown.esc="emit('cancel-edit')"
      @blur="emit('cancel-edit')"
    />
  </div>
</template>

<style scoped>
/* ── 节点行 ─────────────────────────────────────────────────────── */
/* 行本身不铺底纹，悬停与激活只通过文字颜色区分 */
.outline-row {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 3px 6px;
  padding-left: calc(6px + var(--indent));
  outline: none;
}

/* ── 折叠滑块 ───────────────────────────────────────────────────── */
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
  transition: opacity 0.14s ease, color 0.14s ease, transform 0.14s ease;
}

.outline-toggle:hover {
  opacity: 1;
  color: var(--fluen-ink);
}

/* 折叠时箭头旋转 -90°（单 path，免双 path 切换） */
.outline-toggle--collapsed {
  transform: rotate(-90deg);
}

/* 叶子占位：保持对齐、不响应交互 */
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
  transition: color 0.14s ease;
}

/* 层级样式：清晰的字重梯度，避免过度装饰 */
.outline-text--h1 {
  font-size: 14px;
  font-weight: 600;
  color: var(--fluen-ink);
  margin-top: 6px;
}

.outline-text--h2,
.outline-text--h3 {
  font-size: 13px;
  font-weight: 600;
  color: var(--fluen-ink);
}

.outline-text--h4 {
  font-size: 12.5px;
  font-weight: 400;
  color: var(--fluen-slate);
}

.outline-text--h5,
.outline-text--h6 {
  font-size: 12.5px;
  font-weight: 400;
  color: var(--fluen-stone);
}

/* 三态只用字色区分：
   未选中＝层级墨色；悬停＝掺入强调色的浅蓝；选中＝纯强调色 */
.outline-row:not(.outline-row--active):hover .outline-text {
  color: color-mix(in srgb, var(--fluen-accent) 50%, var(--fluen-ink));
}

/* 选中（光标所在行） */
.outline-row--active .outline-text {
  color: var(--fluen-accent);
}

/* ── hover 操作按钮 ─────────────────────────────────────────────── */
.outline-actions {
  display: flex;
  gap: 1px;
  flex-shrink: 0;
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

.outline-add-child {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 3px 6px;
  padding-left: calc(6px + var(--indent));
}
</style>