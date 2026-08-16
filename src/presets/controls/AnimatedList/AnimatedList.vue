<script lang="ts">
export interface AnimatedListProps {
  /** An array of items to display in the scrollable list. */
  items?: unknown[];
  /** Callback function triggered when an item is selected. Receives the selected item and its index. */
  onItemSelect?: (item: unknown, index: number) => void;
  /** Toggle to display the top and bottom gradient overlays. */
  showGradients?: boolean;
  /** Toggle to enable keyboard navigation via arrow and tab keys. */
  enableArrowNavigation?: boolean;
  /** Additional CSS class names for the main container. */
  className?: string;
  /** Additional CSS class names for each list item. */
  itemClassName?: string;
  /** Toggle to display or hide the custom scrollbar. */
  displayScrollbar?: boolean;
  /** Initial index of the selected item. Set to -1 for no selection. */
  initialSelectedIndex?: number;
}

export type AnimatedListEmits = {
  /** Emitted whenever an item is selected (item, index). */
  (e: 'itemSelected', item: unknown, index: number): void;
};
</script>

<script setup lang="ts">
import { onMounted, onUnmounted, ref, useTemplateRef, watch } from 'vue';
import AnimatedListItem from './AnimatedListItem.vue';

/* Slot: `item` — custom per-item content (scoped with `{ item, index }`).
   Falls back to the plain-text rendering when the slot is not provided. */
defineSlots<{
  item?: (props: { item: unknown; index: number }) => unknown;
}>();

/* ------------------------------------------------------------------ *
 * Utils
 * ------------------------------------------------------------------ */

/** Merge class names, filtering out falsy values and inactive conditional entries. */
const cn = (...classes: Array<string | false | undefined | Record<string, boolean>>): string => {
  return classes
    .flatMap((entry) => {
      if (entry && typeof entry === 'object') {
        return Object.entries(entry)
          .filter(([, active]) => active)
          .map(([name]) => name);
      }
      return entry ? [entry] : [];
    })
    .join(' ');
};

/* ------------------------------------------------------------------ *
 * Props & Emits
 * ------------------------------------------------------------------ */

const props = withDefaults(defineProps<AnimatedListProps>(), {
  items: () => [
    'Item 1',
    'Item 2',
    'Item 3',
    'Item 4',
    'Item 5',
    'Item 6',
    'Item 7',
    'Item 8',
    'Item 9',
    'Item 10',
    'Item 11',
    'Item 12',
    'Item 13',
    'Item 14',
    'Item 15'
  ],
  showGradients: true,
  enableArrowNavigation: true,
  className: '',
  itemClassName: '',
  displayScrollbar: true,
  initialSelectedIndex: -1
});

const emit = defineEmits<AnimatedListEmits>();

/* ------------------------------------------------------------------ *
 * Template refs
 * ------------------------------------------------------------------ */

const listRef = useTemplateRef<HTMLDivElement>('listRef');

/* ------------------------------------------------------------------ *
 * Reactive state
 * ------------------------------------------------------------------ */

const selectedIndex = ref(props.initialSelectedIndex);
const keyboardNav = ref(false);
const topGradientOpacity = ref(0);
const bottomGradientOpacity = ref(1);

/* ------------------------------------------------------------------ *
 * Selection handlers
 * ------------------------------------------------------------------ */

const selectItem = (item: unknown, index: number): void => {
  selectedIndex.value = index;
  props.onItemSelect?.(item, index);
  emit('itemSelected', item, index);
};

const handleItemMouseEnter = (index: number): void => {
  selectedIndex.value = index;
};

const handleItemClick = (item: unknown, index: number): void => {
  selectItem(item, index);
};

/* ------------------------------------------------------------------ *
 * Scroll / gradient handlers
 * ------------------------------------------------------------------ */

const handleScroll = (e: Event): void => {
  const target = e.target as HTMLDivElement;
  const { scrollTop, scrollHeight, clientHeight } = target;
  topGradientOpacity.value = Math.min(scrollTop / 50, 1);
  const bottomDistance = scrollHeight - (scrollTop + clientHeight);
  bottomGradientOpacity.value = scrollHeight <= clientHeight ? 0 : Math.min(bottomDistance / 50, 1);
};

/* ------------------------------------------------------------------ *
 * Keyboard navigation
 * ------------------------------------------------------------------ */

const handleKeyDown = (e: KeyboardEvent): void => {
  const lastIndex = props.items.length - 1;
  if (e.key === 'ArrowDown' || (e.key === 'Tab' && !e.shiftKey)) {
    e.preventDefault();
    keyboardNav.value = true;
    selectedIndex.value = Math.min(selectedIndex.value + 1, lastIndex);
  } else if (e.key === 'ArrowUp' || (e.key === 'Tab' && e.shiftKey)) {
    e.preventDefault();
    keyboardNav.value = true;
    selectedIndex.value = Math.max(selectedIndex.value - 1, 0);
  } else if (e.key === 'Enter') {
    if (selectedIndex.value >= 0 && selectedIndex.value <= lastIndex) {
      e.preventDefault();
      selectItem(props.items[selectedIndex.value], selectedIndex.value);
    }
  }
};

/* ------------------------------------------------------------------ *
 * Watchers
 * ------------------------------------------------------------------ */

/** Bring the keyboard-selected item into view within the scroll container. */
const scrollToIndex = (index: number): void => {
  const container = listRef.value;
  const selectedItem = container?.querySelector(`[data-index="${index}"]`) as HTMLElement | null;
  if (!container || !selectedItem) return;

  const extraMargin = 50;
  const containerRect = container.getBoundingClientRect();
  const itemRect = selectedItem.getBoundingClientRect();
  const itemTop = itemRect.top - containerRect.top;
  const itemBottom = itemRect.bottom - containerRect.top;
  const containerScrollTop = container.scrollTop;
  const containerHeight = container.clientHeight;

  if (itemTop < containerScrollTop + extraMargin) {
    container.scrollTo({ top: itemTop - extraMargin, behavior: 'smooth' });
  } else if (itemBottom > containerScrollTop + containerHeight - extraMargin) {
    container.scrollTo({ top: itemBottom - containerHeight + extraMargin, behavior: 'smooth' });
  }
};

watch(selectedIndex, (index) => {
  if (!keyboardNav.value || index < 0) return;
  scrollToIndex(index);
  keyboardNav.value = false;
});

watch(
  () => props.items,
  (items) => {
    if (selectedIndex.value >= items.length) {
      selectedIndex.value = Math.max(items.length - 1, -1);
    }
  }
);

watch(
  () => props.initialSelectedIndex,
  (index) => {
    selectedIndex.value = index;
  }
);

/* ------------------------------------------------------------------ *
 * Lifecycle
 * ------------------------------------------------------------------ */

onMounted(() => {
  if (props.enableArrowNavigation) {
    window.addEventListener('keydown', handleKeyDown);
  }
});

onUnmounted(() => {
  if (props.enableArrowNavigation) {
    window.removeEventListener('keydown', handleKeyDown);
  }
});
</script>

<template>
  <div :class="cn('al-root', className)">
    <div
      ref="listRef"
      class="al-list"
      :class="{ 'al-list--bare': !displayScrollbar }"
      :style="{ scrollbarWidth: displayScrollbar ? 'thin' : 'none' }"
      role="listbox"
      @scroll="handleScroll"
    >
      <AnimatedListItem
        v-for="(item, index) in items"
        :key="index"
        :index="index"
        :delay="0.1"
        @mouseenter="handleItemMouseEnter(index)"
        @click="handleItemClick(item, index)"
      >
        <div :class="cn('al-item-content', { 'al-item-content--selected': selectedIndex === index }, itemClassName)">
          <slot name="item" :item="item" :index="index">
            <p class="al-item-text">{{ String(item) }}</p>
          </slot>
        </div>
      </AnimatedListItem>
    </div>

    <template v-if="showGradients">
      <div
        class="al-gradient al-gradient--top"
        :style="{ opacity: topGradientOpacity }"
        aria-hidden="true"
      />
      <div
        class="al-gradient al-gradient--bottom"
        :style="{ opacity: bottomGradientOpacity }"
        aria-hidden="true"
      />
    </template>
  </div>
</template>

<style scoped>
/* Self-contained styles (no Tailwind required) — keeps the preset portable.
   Colors derive from the host theme via --fluen-* variables, so the list
   adapts automatically to light and dark mode. */

.al-root {
  position: relative;
  /* Host can override via --fluen-al-width (e.g. 100% to fill a panel). */
  width: var(--fluen-al-width, 500px);
  max-width: 100%;
}

/* ── Scroll container ─────────────────────────────────────────────── */

.al-list {
  /* Host can override via --fluen-al-max-height (e.g. 100% to fill a panel).
     border-box 让 max-height 包含内边距，避免滚动容器超出宿主高度被裁剪。 */
  box-sizing: border-box;
  max-height: var(--fluen-al-max-height, 400px);
  overflow-y: auto;
  padding: 1rem;
  scrollbar-color: var(--fluen-hairline, rgba(128, 128, 128, 0.5)) transparent;
}

.al-list::-webkit-scrollbar {
  width: 8px;
}

.al-list::-webkit-scrollbar-track {
  background: transparent;
}

.al-list::-webkit-scrollbar-thumb {
  background: var(--fluen-hairline, rgba(128, 128, 128, 0.5));
  border-radius: 4px;
}

.al-list--bare::-webkit-scrollbar {
  display: none;
}

/* ── Item surface ─────────────────────────────────────────────────── */

.al-item-content {
  padding: 1rem;
  background: var(--fluen-surface-soft, #1a1a1a);
  border: 1px solid var(--fluen-hairline, rgba(128, 128, 128, 0.25));
  border-radius: 12px;
  transition:
    border-color 0.15s ease,
    background-color 0.15s ease;
}

.al-item-content:hover {
  border-color: var(--fluen-accent, #3b82f6);
}

.al-item-content--selected {
  border-color: var(--fluen-accent, #3b82f6);
  background: var(--fluen-hover, rgba(128, 128, 128, 0.15));
}

.al-item-text {
  margin: 0;
  color: var(--fluen-ink, #f6f6f6);
  font-family: var(--fluen-font-sans, sans-serif);
  font-size: 14px;
  line-height: 1.5;
}

/* ── Gradient overlays ────────────────────────────────────────────── */

.al-gradient {
  position: absolute;
  right: 0;
  left: 0;
  pointer-events: none;
  transition: opacity 0.3s ease;
}

.al-gradient--top {
  top: 0;
  height: 50px;
  background: linear-gradient(to bottom, var(--fluen-canvas, #ffffff), transparent);
}

.al-gradient--bottom {
  bottom: 0;
  height: 100px;
  background: linear-gradient(to top, var(--fluen-canvas, #ffffff), transparent);
}
</style>
