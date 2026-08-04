<script setup lang="ts">
/**
 * ObDropdown — Onboarding 通用下拉选择器。
 *
 * 支持 SVG 图标 + 文本标签的选项展示，点击外部自动收起。
 * 样式与 onboarding 暗色主题保持一致。
 *
 * 下拉面板通过 Teleport 渲染到 body，避免被父级 overflow:hidden 裁剪。
 * 面板位置通过 getBoundingClientRect 动态计算，使用 position:fixed。
 */
import { nextTick, onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from '../../../i18n';
import type { DropdownOption } from '../types';

const props = withDefaults(defineProps<{
  /** 当前选中值。 */
  modelValue: string;
  /** 可选项列表。 */
  options: DropdownOption[];
  /** 占位提示文本。 */
  placeholder?: string;
}>(), {
  placeholder: '',
});

const { t } = useI18n();

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
}>();

const isOpen = ref(false);
const triggerRef = ref<HTMLElement | null>(null);
const menuRef = ref<HTMLElement | null>(null);

/** 下拉面板的 fixed 定位坐标 */
const menuStyle = ref<{
  top: string;
  left: string;
  width: string;
}>({ top: '0px', left: '0px', width: '0px' });

const selectedOption = ref<DropdownOption | undefined>(
  props.options.find((opt) => opt.value === props.modelValue),
);

/* 选中值变化时同步选项引用 */
watch(
  () => props.modelValue,
  (val) => {
    selectedOption.value = props.options.find((opt) => opt.value === val);
  },
);

/* ── 位置计算 ─────────────────────────────────────────────────────────── */

function updateMenuPosition(): void {
  if (!triggerRef.value) return;
  const rect = triggerRef.value.getBoundingClientRect();
  menuStyle.value = {
    top: `${rect.bottom + 6}px`,
    left: `${rect.left}px`,
    width: `${rect.width}px`,
  };
}

/* ── 开关逻辑 ─────────────────────────────────────────────────────────── */

const open = (): void => {
  isOpen.value = true;
  nextTick(() => {
    updateMenuPosition();
  });
};

const close = (): void => {
  isOpen.value = false;
};

const toggle = (): void => {
  if (isOpen.value) close();
  else open();
};

const select = (option: DropdownOption): void => {
  emit('update:modelValue', option.value);
  close();
};

/* ── 全局事件：点击外部关闭、Escape 关闭、滚动/resize 重新定位 ─────────── */

const handleClickOutside = (event: MouseEvent): void => {
  const target = event.target as Node;
  if (
    triggerRef.value?.contains(target) ||
    menuRef.value?.contains(target)
  ) {
    return;
  }
  close();
};

const handleEscape = (event: KeyboardEvent): void => {
  if (event.key === 'Escape') close();
};

const handleScrollOrResize = (): void => {
  if (isOpen.value) updateMenuPosition();
};

if (typeof document !== 'undefined') {
  document.addEventListener('click', handleClickOutside);
  document.addEventListener('keydown', handleEscape);
  window.addEventListener('resize', handleScrollOrResize);
  window.addEventListener('scroll', handleScrollOrResize, true);
}

onBeforeUnmount(() => {
  if (typeof document !== 'undefined') {
    document.removeEventListener('click', handleClickOutside);
    document.removeEventListener('keydown', handleEscape);
    window.removeEventListener('resize', handleScrollOrResize);
    window.removeEventListener('scroll', handleScrollOrResize, true);
  }
});
</script>

<template>
  <div ref="triggerRef" class="ob-dropdown">
    <!-- 触发器 -->
    <button
      class="ob-dropdown__trigger"
      :class="{ 'ob-dropdown__trigger--open': isOpen }"
      type="button"
      @click="toggle"
    >
      <!-- 图标（预留位） -->
      <span v-if="selectedOption?.icon" class="ob-dropdown__icon">
        <svg
          viewBox="0 0 24 24"
          width="18"
          height="18"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
          v-html="selectedOption.icon"
        />
      </span>
      <span v-else-if="options.find((o) => o.value === modelValue)?.icon" class="ob-dropdown__icon">
        <svg
          viewBox="0 0 24 24"
          width="18"
          height="18"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
          v-html="options.find((o) => o.value === modelValue)!.icon"
        />
      </span>

      <span class="ob-dropdown__label">
        {{ selectedOption?.label ?? (placeholder || t('common.placeholder.select')) }}
      </span>

      <svg
        class="ob-dropdown__chevron"
        :class="{ 'ob-dropdown__chevron--up': isOpen }"
        viewBox="0 0 24 24"
        width="16"
        height="16"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path d="m6 9 6 6 6-6" />
      </svg>
    </button>

    <!-- 下拉面板 — Teleport 到 body 避免被裁剪 -->
    <Teleport to="body">
      <Transition name="ob-dropdown-fade">
        <ul
          v-if="isOpen"
          ref="menuRef"
          class="ob-dropdown__menu"
          :style="menuStyle"
        >
          <li
            v-for="opt in options"
            :key="opt.value"
            class="ob-dropdown__item"
            :class="{ 'ob-dropdown__item--active': opt.value === modelValue }"
            @click="select(opt)"
          >
            <span v-if="opt.icon" class="ob-dropdown__icon">
              <svg
                viewBox="0 0 24 24"
                width="18"
                height="18"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
                v-html="opt.icon"
              />
            </span>
            <span v-else class="ob-dropdown__icon ob-dropdown__icon--placeholder" />

            <span class="ob-dropdown__item-label">{{ opt.label }}</span>

            <svg
              v-if="opt.value === modelValue"
              class="ob-dropdown__check"
              viewBox="0 0 24 24"
              width="16"
              height="16"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M5 13l4 4L19 7" />
            </svg>
          </li>
        </ul>
      </Transition>
    </Teleport>
  </div>
</template>

<style scoped>
.ob-dropdown {
  position: relative;
  width: 100%;
}

/* ── 触发器 ──────────────────────────────────────────────────────────── */
.ob-dropdown__trigger {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  width: 100%;
  padding: 10px 14px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-hover);
  color: var(--fluen-on-dark);
  font-family: var(--fluen-font-sans);
  font-size: 0.9rem;
  cursor: pointer;
  box-sizing: border-box;
  transition: border-color 0.2s ease;
  text-align: left;
}

.ob-dropdown__trigger:hover {
  border-color: var(--fluen-stone);
}

.ob-dropdown__trigger--open {
  border-color: var(--fluen-accent);
}

.ob-dropdown__icon {
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  width: 20px;
  height: 20px;
  color: var(--fluen-slate);
}

.ob-dropdown__icon--placeholder {
  background: var(--fluen-hairline);
  border-radius: 4px;
}

.ob-dropdown__label {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ob-dropdown__chevron {
  flex-shrink: 0;
  color: var(--fluen-stone);
  transition: transform 0.2s ease;
}

.ob-dropdown__chevron--up {
  transform: rotate(180deg);
}

/* ── 下拉面板（Teleport 到 body，使用 fixed 定位） ─────────────────────── */
/* 注意：Teleport 后 .ob-dropdown__menu 不再在 scoped 范围内生效，          */
/* 需要使用 :deep() 或全局样式。这里使用单独的非 scoped style 块。          */
</style>

<!--
  Teleport 到 body 的元素不受父组件 scoped 样式影响，
  需要使用非 scoped 全局样式来渲染下拉面板。
  使用 .ob-dropdown__ 前缀以避免与其他组件冲突。
-->
<style>
.ob-dropdown__menu {
  position: fixed;
  z-index: 9999;
  margin: 0;
  padding: 4px;
  list-style: none;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-surface);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  overflow: hidden;
  box-sizing: border-box;
}

.ob-dropdown__item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 8px 10px;
  border-radius: 6px;
  color: var(--fluen-charcoal);
  font-family: var(--fluen-font-sans);
  font-size: 0.85rem;
  cursor: pointer;
  transition: background-color 0.15s ease, color 0.15s ease;
}

.ob-dropdown__item:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.ob-dropdown__item--active {
  background: var(--fluen-info-bg);
  color: var(--fluen-ink);
}

.ob-dropdown__item-label {
  flex: 1;
}

.ob-dropdown__check {
  flex-shrink: 0;
  color: var(--fluen-accent);
}

/* ── 过渡动画 ────────────────────────────────────────────────────────── */
.ob-dropdown-fade-enter-active,
.ob-dropdown-fade-leave-active {
  transition: opacity 0.15s ease, transform 0.15s ease;
}

.ob-dropdown-fade-enter-from,
.ob-dropdown-fade-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
