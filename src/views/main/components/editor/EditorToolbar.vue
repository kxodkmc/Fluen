<script setup lang="ts">
/**
 * EditorToolbar — MD 源码编辑工具栏（加粗 / 斜体 / 标题）。
 *
 * 位于编辑区（源码/双栏视图）顶部，按钮点击时调用
 * `useFluenEditor().toggleFormat(kind)` 对当前选区/光标执行格式切换。
 * 标题按钮为下拉菜单，可从 H1-H6 中选择目标级别；格式行为（剥离内部
 * 标记后整体包裹、光标位于标记内取消、标题级别切换等）由
 * `codemirror/formatting.ts` 纯函数层统一实现，本组件仅负责 UI 分发。
 *
 * 仅随编辑视图显示（ContentPanel 中 `v-show` 控制），预览模式不渲染。
 */
import { ref, onMounted, onUnmounted } from 'vue';
import { useI18n } from '../../../../i18n';
import { useFluenEditor } from './composables/useFluenEditor';
import type { MarkdownFormatKind, HeadingLevel } from './codemirror/formatting';

const { t } = useI18n();
const editor = useFluenEditor();

/** 行内格式条目：kind 为格式类型，glyph 为按钮显示字符，hint 为示例提示。 */
const items: { kind: MarkdownFormatKind; glyph: string; label: string; hint: string }[] = [
  {
    kind: 'bold',
    glyph: 'B',
    label: t('main.content.toolbar.bold'),
    hint: '**文本**',
  },
  {
    kind: 'italic',
    glyph: 'I',
    label: t('main.content.toolbar.italic'),
    hint: '*文本*',
  },
];

/** 标题下拉级别：glyph 为按钮/菜单显示，hint 为对应 Markdown 前缀示例。 */
const headingLevels: { level: HeadingLevel; glyph: string; hint: string }[] = [1, 2, 3, 4, 5, 6].map((level) => ({
  level: level as HeadingLevel,
  glyph: `H${level}`,
  hint: `${'#'.repeat(level)} 文本`,
}));

const headingLabel = t('main.content.toolbar.heading');
const headingOpen = ref(false);
const headingRef = ref<HTMLElement | null>(null);

function onFormat(kind: MarkdownFormatKind): void {
  editor.toggleFormat(kind);
}

function toggleHeading(): void {
  headingOpen.value = !headingOpen.value;
}

function onHeadingLevel(level: HeadingLevel): void {
  editor.toggleFormat('heading', level);
  headingOpen.value = false;
}

/** 点击下拉外部时关闭。 */
function handleDocumentClick(e: MouseEvent): void {
  if (!headingOpen.value) return;
  if (headingRef.value && !headingRef.value.contains(e.target as Node)) {
    headingOpen.value = false;
  }
}

/** Escape 键关闭。 */
function handleKeydown(e: KeyboardEvent): void {
  if (e.key === 'Escape' && headingOpen.value) {
    headingOpen.value = false;
  }
}

onMounted(() => {
  document.addEventListener('mousedown', handleDocumentClick);
  document.addEventListener('keydown', handleKeydown);
});

onUnmounted(() => {
  document.removeEventListener('mousedown', handleDocumentClick);
  document.removeEventListener('keydown', handleKeydown);
});
</script>

<template>
  <div class="editor-toolbar" role="toolbar" :aria-label="t('main.content.toolbar.label')">
    <button
      v-for="item in items"
      :key="item.kind"
      class="editor-toolbar__btn"
      :class="`editor-toolbar__btn--${item.kind}`"
      type="button"
      :title="`${item.label}（${item.hint}）`"
      :aria-label="item.label"
      @click="onFormat(item.kind)"
    >
      {{ item.glyph }}
    </button>

    <div ref="headingRef" class="editor-toolbar__heading">
      <button
        class="editor-toolbar__btn editor-toolbar__btn--heading"
        :class="{ 'editor-toolbar__btn--active': headingOpen }"
        type="button"
        :title="`${headingLabel}（${headingLevels[0].hint}）`"
        :aria-label="headingLabel"
        :aria-expanded="headingOpen"
        @click="toggleHeading"
      >
        <span>{{ headingLevels[0].glyph }}</span>
        <svg class="editor-toolbar__chevron" viewBox="0 0 16 16" aria-hidden="true">
          <path
            d="M4 6l4 4 4-4"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </button>

      <Transition name="toolbar-dropdown">
        <div v-if="headingOpen" class="editor-toolbar__menu" role="menu" :aria-label="headingLabel">
          <button
            v-for="opt in headingLevels"
            :key="opt.level"
            class="editor-toolbar__menu-item"
            role="menuitem"
            type="button"
            :title="`${headingLabel} ${opt.level}（${opt.hint}）`"
            @click="onHeadingLevel(opt.level)"
          >
            <span class="editor-toolbar__menu-glyph">{{ opt.glyph }}</span>
            <span class="editor-toolbar__menu-hint">{{ opt.hint }}</span>
          </button>
        </div>
      </Transition>
    </div>
  </div>
</template>

<style scoped>
.editor-toolbar {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
  height: 36px;
  padding: 0 8px;
  background: var(--fluen-surface);
  border-bottom: 1px solid var(--fluen-hairline);
}

.editor-toolbar__btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 28px;
  height: 26px;
  padding: 0 6px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--fluen-slate);
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  line-height: 1;
  cursor: pointer;
  user-select: none;
  transition: background 0.15s ease, color 0.15s ease;
}

.editor-toolbar__btn:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.editor-toolbar__btn:active {
  background: var(--fluen-hairline);
}

.editor-toolbar__btn:focus-visible {
  outline: 2px solid var(--fluen-accent);
  outline-offset: 1px;
}

.editor-toolbar__btn--active {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.editor-toolbar__btn--bold {
  font-weight: 700;
}

.editor-toolbar__btn--italic {
  font-style: italic;
}

.editor-toolbar__btn--heading {
  font-weight: 600;
  font-size: 12px;
}

.editor-toolbar__chevron {
  width: 12px;
  height: 12px;
  margin-left: 2px;
}

.editor-toolbar__heading {
  position: relative;
}

.editor-toolbar__menu {
  position: absolute;
  top: 100%;
  left: 0;
  margin-top: 4px;
  min-width: 132px;
  padding: 4px;
  background: var(--fluen-surface);
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  box-shadow: var(--fluen-shadow-card);
  z-index: 1000;
}

.editor-toolbar__menu-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  width: 100%;
  padding: 5px 8px;
  border: none;
  background: transparent;
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  line-height: 1;
  text-align: left;
  cursor: pointer;
  border-radius: 4px;
  transition: background 0.12s ease;
}

.editor-toolbar__menu-item:hover {
  background: var(--fluen-hover);
}

.editor-toolbar__menu-glyph {
  font-weight: 600;
  font-size: 12px;
}

.editor-toolbar__menu-hint {
  color: var(--fluen-stone);
  font-size: 12px;
}

/* ── 过渡动画 ─────────────────────────────────────────────────────── */
.toolbar-dropdown-enter-active,
.toolbar-dropdown-leave-active {
  transition: opacity 0.15s ease, transform 0.15s ease;
}

.toolbar-dropdown-enter-from,
.toolbar-dropdown-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
