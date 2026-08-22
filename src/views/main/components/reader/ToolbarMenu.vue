<script setup lang="ts" generic="T extends number | string">
/**
 * ToolbarMenu —— 工具栏下拉菜单。
 *
 * 触发按钮展示「标签 · 当前值」，点击弹出选项列表：
 * - 点击选项即选中并收起；点击外部 / Esc 收起；
 * - 当前选中项以强调色文字标识，无底纹。
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { Chevron } from '../functionpanel/panels/icons';

const props = defineProps<{
  /** 菜单标签（如「字号」）。 */
  label: string;
  /** 全部选项。 */
  options: ReadonlyArray<{ value: T; label: string }>;
  /** 当前选中值。 */
  modelValue: T;
}>();

const emit = defineEmits<{
  'update:modelValue': [value: T];
}>();

const open = ref(false);
const rootRef = ref<HTMLElement | null>(null);

const currentLabel = computed(
  () => props.options.find((o) => o.value === props.modelValue)?.label ?? '',
);

function select(value: T): void {
  emit('update:modelValue', value);
  open.value = false;
}

function onDocPointerDown(e: PointerEvent): void {
  if (open.value && rootRef.value && !rootRef.value.contains(e.target as Node)) {
    open.value = false;
  }
}

function onKeydown(e: KeyboardEvent): void {
  if (e.key === 'Escape') open.value = false;
}

onMounted(() => {
  document.addEventListener('pointerdown', onDocPointerDown);
  document.addEventListener('keydown', onKeydown);
});

onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', onDocPointerDown);
  document.removeEventListener('keydown', onKeydown);
});
</script>

<template>
  <div ref="rootRef" class="toolbar-menu">
    <button
      class="toolbar-menu__trigger"
      :class="{ 'toolbar-menu__trigger--open': open }"
      @click="open = !open"
    >
      <span>{{ label }}</span>
      <span class="toolbar-menu__value">{{ currentLabel }}</span>
      <Chevron :size="10" :stroke-width="2" class="toolbar-menu__chevron" />
    </button>

    <div v-if="open" class="toolbar-menu__list">
      <button
        v-for="opt in options"
        :key="opt.value"
        class="toolbar-menu__item"
        :class="{ 'toolbar-menu__item--active': opt.value === modelValue }"
        @click="select(opt.value)"
      >
        {{ opt.label }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.toolbar-menu {
  position: relative;
}

/* 触发按钮：纯文字 + 值 + 箭头 */
.toolbar-menu__trigger {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 28px;
  padding: 0 6px;
  border: none;
  background: transparent;
  border-radius: 6px;
  color: var(--fluen-stone);
  font-size: 12px;
  cursor: pointer;
  transition: color 0.14s ease;
}

.toolbar-menu__trigger:hover,
.toolbar-menu__trigger--open {
  color: var(--fluen-ink);
}

.toolbar-menu__value {
  color: inherit;
  font-weight: 600;
}

.toolbar-menu__chevron {
  opacity: 0.7;
  transition: transform 0.14s ease;
}

.toolbar-menu__trigger--open .toolbar-menu__chevron {
  transform: rotate(180deg);
}

/* 选项列表：右对齐浮层（工具栏位于窗口右侧） */
.toolbar-menu__list {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  min-width: 88px;
  padding: 4px;
  background: var(--fluen-canvas);
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  box-shadow: var(--fluen-shadow-modal);
  z-index: 20;
}

.toolbar-menu__item {
  display: block;
  width: 100%;
  padding: 5px 10px;
  border: none;
  background: transparent;
  border-radius: 5px;
  color: var(--fluen-ink);
  font-size: 12.5px;
  text-align: left;
  cursor: pointer;
  white-space: nowrap;
  transition: background 0.14s ease, color 0.14s ease;
}

.toolbar-menu__item:hover {
  background: var(--fluen-hover);
}

/* 选中项：仅强调色文字标识 */
.toolbar-menu__item--active {
  color: var(--fluen-accent);
}
</style>
