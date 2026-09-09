<script setup lang="ts">
/**
 * EditorQuoteToolbar — 论文编辑器划选浮出工具栏。
 *
 * 在论文编辑器（source / live 视图）中框选文本后，于选区上方浮出
 * 「添加到对话」按钮：
 *   - 点击后将选中文本加入 Motis 对话引用（useChatQuotes），
 *     并打开 Motis 面板，输入区上方会展示引用文段卡片
 *   - 选区为空、编辑器失焦或滚动时按钮隐藏/重定位
 *
 * 定位：基于 CM6 `coordsAtPos` 的视口坐标换算为容器相对坐标，
 * 组件以 absolute 悬浮于 `.content-editor`（position: relative）内。
 * 仅由 ContentPanel 在论文编辑视图中挂载。
 */
import { ref, inject, onMounted, onBeforeUnmount } from 'vue';
import { useI18n } from '../../../../i18n';
import { useFluenEditor } from './composables/useFluenEditor';
import { useChatQuotes } from '../../composables/useChatQuotes';
import { useMainLayout, MAIN_LAYOUT_KEY } from '../../composables/useMainLayout';

const { t } = useI18n();
const editor = useFluenEditor();
const quotes = useChatQuotes();

// MainView provide 的共享布局实例；独立挂载时回退为自建实例
const layout = inject(MAIN_LAYOUT_KEY, () => useMainLayout(), true);

const rootRef = ref<HTMLDivElement | null>(null);

/** 是否显示工具栏。 */
const visible = ref(false);
/** 容器相对坐标（px）。 */
const left = ref(0);
const top = ref(0);

/** 滚动重定位/隐藏的捕获监听句柄。 */
let scrollHandler: (() => void) | null = null;
/** 选区变化订阅。 */
let unsubSelection: (() => void) | null = null;

/** 选区变化时更新按钮位置（空选区时隐藏）。 */
function updateFromSelection(): void {
  if (!rootRef.value) return;
  const text = editor.getSelectionText();
  if (!text) {
    visible.value = false;
    return;
  }
  const coords = editor.getSelectionViewportCoords();
  if (!coords) {
    visible.value = false;
    return;
  }
  const rect = rootRef.value.parentElement?.getBoundingClientRect();
  if (!rect) return;
  left.value = coords.left - rect.left;
  top.value = coords.top - rect.top;
  visible.value = true;
}

/** 选区清空或编辑器失焦时隐藏。 */
function checkVisibility(): void {
  if (!editor.getSelectionText()) {
    visible.value = false;
  }
}

onMounted(() => {
  // 选区变化（含折叠清空）与滚动均驱动重定位
  unsubSelection = editor.onSelectionChange(() => updateFromSelection());
  // 编辑器内滚动时选区坐标即时失效：重算位置（选区滚出视口时 coordsAtPos
  // 返回 null，自动隐藏）
  scrollHandler = () => updateFromSelection();
  document.addEventListener('scroll', scrollHandler, true);
  checkVisibility();
});

onBeforeUnmount(() => {
  unsubSelection?.();
  if (scrollHandler) document.removeEventListener('scroll', scrollHandler, true);
});

/**
 * 点击「添加到对话」：写入引用、打开 Motis 面板并清除选区。
 * mousedown 已 preventDefault，点击全程不抢编辑器焦点。
 */
function handleAdd(): void {
  const text = editor.getSelectionText();
  if (text) {
    quotes.addQuote(text);
  }
  editor.clearSelection();
  visible.value = false;
  layout.showRightPanel('motis');
}
</script>

<template>
  <div
    ref="rootRef"
    class="quote-toolbar"
    :class="{ 'quote-toolbar--visible': visible }"
    :style="visible ? { left: `${left}px`, top: `${top}px` } : undefined"
    @mousedown.prevent
  >
    <button
      type="button"
      class="quote-toolbar__btn"
      @click="handleAdd"
    >
      <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
        <path d="M3 21c3-1 5-3.5 5-7V6H3v8h4c0 3-1.5 5-4 6v1z" />
        <path d="M14 21c3-1 5-3.5 5-7V6h-5v8h4c0 3-1.5 5-4 6v1z" />
      </svg>
      <span>{{ t('main.content.selection.addQuoteToChat') }}</span>
    </button>
  </div>
</template>

<style scoped>
.quote-toolbar {
  position: absolute;
  z-index: 30;
  /* 以 (left, top) 为选区上方中点，向左居中 + 上移 */
  transform: translate(-50%, calc(-100% - 10px));
  pointer-events: none;
  opacity: 0;
  visibility: hidden;
  transition: opacity 0.12s ease;
}

.quote-toolbar--visible {
  pointer-events: auto;
  opacity: 1;
  visibility: visible;
}

.quote-toolbar__btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 14px;
  border: 1px solid var(--fluen-primary);
  border-radius: 9999px;
  background: var(--fluen-primary);
  color: var(--fluen-on-primary);
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  font-weight: 600;
  line-height: 1.4;
  cursor: pointer;
  box-shadow: var(--fluen-shadow-card);
  transition: background 0.15s ease, border-color 0.15s ease;
}

.quote-toolbar__btn:hover {
  background: var(--fluen-charcoal);
  border-color: var(--fluen-charcoal);
}

.quote-toolbar__btn:active {
  background: var(--fluen-ink-strong);
}
</style>
