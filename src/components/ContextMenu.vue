<script setup lang="ts">
/**
 * ContextMenu — 通用右键上下文菜单。
 *
 * 设计目标：
 *   - **可复用**：通过 items props 配置菜单项，任意业务场景均可使用
 *   - **可扩展**：支持图标、禁用、危险动作、分隔线，未来可扩展子菜单
 *   - **位置自适应**：通过 fixed 定位 + 边界检测，避免超出视窗
 *   - **键鼠交互**：点击外部 / Escape 关闭，点击项触发 select 并关闭
 *
 * 用法：
 * ```vue
 * <ContextMenu
 *   :visible="menuVisible"
 *   :items="menuItems"
 *   :x="menuX"
 *   :y="menuY"
 *   @select="onMenuSelect"
 *   @close="menuVisible = false"
 * />
 * ```
 *
 * 菜单项结构见 [`ContextMenuItem`]。
 */
import { nextTick, onBeforeUnmount, ref, watch } from 'vue';

/** 单个菜单项。 */
export interface ContextMenuItem {
  /** 唯一标识（用于 select 事件回传）。 */
  id: string;
  /** 显示文本（分隔线时可省略）。 */
  label?: string;
  /** SVG path（24×24 viewBox，stroke 风格）。 */
  icon?: string;
  /** 是否禁用。 */
  disabled?: boolean;
  /** 是否为危险动作（红色高亮）。 */
  danger?: boolean;
  /** 是否为分隔线（label/icon/disabled/danger 被忽略）。 */
  divider?: boolean;
}

const props = defineProps<{
  /** 是否显示。 */
  visible: boolean;
  /** 菜单项列表。 */
  items: ContextMenuItem[];
  /** 触发点视窗 X 坐标（fixed 定位）。 */
  x: number;
  /** 触发点视窗 Y 坐标（fixed 定位）。 */
  y: number;
}>();

const emit = defineEmits<{
  /** 选中某项（id 为 item.id，disabled 项不触发）。 */
  (e: 'select', id: string): void;
  /** 关闭菜单（点击外部 / Escape / 选中后）。 */
  (e: 'close'): void;
}>();

const menuRef = ref<HTMLElement | null>(null);

/** 实际渲染位置（自适应边界后）。 */
const menuStyle = ref<{ top: string; left: string }>({ top: '0px', left: '0px' });

/* ── 位置计算（避免超出视窗） ─────────────────────────────────────────── */

function updatePosition(): void {
  if (!menuRef.value) return;
  const rect = menuRef.value.getBoundingClientRect();
  const margin = 4;
  let left = props.x;
  let top = props.y;
  if (left + rect.width + margin > window.innerWidth) {
    left = Math.max(margin, props.x - rect.width);
  }
  if (top + rect.height + margin > window.innerHeight) {
    top = Math.max(margin, props.y - rect.height);
  }
  menuStyle.value = {
    top: `${top}px`,
    left: `${left}px`,
  };
}

/* ── 显示/隐藏监听 ───────────────────────────────────────────────────── */

watch(
  () => props.visible,
  async (visible) => {
    if (visible) {
      // 先以触发点为初始位置渲染，下一帧再修正
      menuStyle.value = { top: `${props.y}px`, left: `${props.x}px` };
      await nextTick();
      updatePosition();
      registerGlobalListeners();
    } else {
      unregisterGlobalListeners();
    }
  },
);

/* ── 全局事件：点击外部关闭、Escape 关闭、滚动/resize 重新定位 ─────────── */

function handleClickOutside(event: MouseEvent): void {
  const target = event.target as Node;
  if (menuRef.value?.contains(target)) return;
  emit('close');
}

function handleEscape(event: KeyboardEvent): void {
  if (event.key === 'Escape') emit('close');
}

function handleScrollOrResize(): void {
  if (props.visible) updatePosition();
}

function registerGlobalListeners(): void {
  if (typeof document === 'undefined') return;
  // 延迟一帧注册，避免触发右键的同一 click 事件立即关闭菜单
  requestAnimationFrame(() => {
    document.addEventListener('click', handleClickOutside);
    document.addEventListener('contextmenu', handleClickOutside);
    document.addEventListener('keydown', handleEscape);
    window.addEventListener('resize', handleScrollOrResize);
    window.addEventListener('scroll', handleScrollOrResize, true);
  });
}

function unregisterGlobalListeners(): void {
  if (typeof document === 'undefined') return;
  document.removeEventListener('click', handleClickOutside);
  document.removeEventListener('contextmenu', handleClickOutside);
  document.removeEventListener('keydown', handleEscape);
  window.removeEventListener('resize', handleScrollOrResize);
  window.removeEventListener('scroll', handleScrollOrResize, true);
}

onBeforeUnmount(() => {
  unregisterGlobalListeners();
});

/* ── 选中处理 ───────────────────────────────────────────────────────── */

function handleSelect(item: ContextMenuItem): void {
  if (item.disabled || item.divider) return;
  emit('select', item.id);
  emit('close');
}
</script>

<template>
  <Teleport to="body">
    <Transition name="ctx-menu-fade">
      <ul
        v-if="visible"
        ref="menuRef"
        class="ctx-menu"
        :style="menuStyle"
        role="menu"
      >
        <template v-for="item in items" :key="item.id">
          <li
            v-if="item.divider"
            class="ctx-menu__divider"
            role="separator"
          />
          <li
            v-else
            class="ctx-menu__item"
            :class="{
              'ctx-menu__item--disabled': item.disabled,
              'ctx-menu__item--danger': item.danger,
            }"
            role="menuitem"
            :aria-disabled="item.disabled"
            @click="handleSelect(item)"
          >
            <span v-if="item.icon" class="ctx-menu__icon">
              <svg
                viewBox="0 0 24 24"
                width="15"
                height="15"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                v-html="item.icon"
              />
            </span>
            <span v-else class="ctx-menu__icon ctx-menu__icon--placeholder" />
            <span class="ctx-menu__label">{{ item.label }}</span>
          </li>
        </template>
      </ul>
    </Transition>
  </Teleport>
</template>

<!--
  Teleport 到 body 的元素不受父组件 scoped 样式影响，
  使用非 scoped 全局样式渲染菜单本体，前缀 ctx-menu 避免冲突。
-->
<style>
.ctx-menu {
  position: fixed;
  z-index: 10000;
  min-width: 160px;
  max-width: 240px;
  margin: 0;
  padding: 4px;
  list-style: none;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-surface);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.18);
  overflow: hidden;
  box-sizing: border-box;
  font-family: var(--fluen-font-sans);
  user-select: none;
}

.ctx-menu__item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 7px 10px;
  border-radius: 6px;
  color: var(--fluen-charcoal);
  font-size: 0.8rem;
  cursor: pointer;
  transition: background-color 0.12s ease, color 0.12s ease;
}

.ctx-menu__item:hover:not(.ctx-menu__item--disabled) {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.ctx-menu__item--danger:hover:not(.ctx-menu__item--disabled) {
  background: var(--fluen-error-bg);
  color: var(--fluen-error);
}

.ctx-menu__item--disabled {
  color: var(--fluen-muted);
  cursor: not-allowed;
}

.ctx-menu__icon {
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  width: 16px;
  height: 16px;
}

.ctx-menu__icon--placeholder {
  width: 16px;
  height: 1px;
  background: transparent;
}

.ctx-menu__label {
  flex: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.ctx-menu__divider {
  height: 1px;
  margin: 4px 6px;
  background: var(--fluen-hairline);
}

/* ── 过渡动画 ────────────────────────────────────────────────────────── */
.ctx-menu-fade-enter-active,
.ctx-menu-fade-leave-active {
  transition: opacity 0.12s ease, transform 0.12s ease;
}

.ctx-menu-fade-enter-from,
.ctx-menu-fade-leave-to {
  opacity: 0;
  transform: scale(0.96);
}
</style>
