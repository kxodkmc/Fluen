<script setup lang="ts">
/**
 * TitleBarMenuDropdown — 标题栏菜单下拉子菜单组件。
 *
 * 当用户点击标题栏的一级菜单（如"文件"）时，在菜单按钮下方弹出
 * 一列子菜单项。点击子菜单项后 emit `select` 事件并自动关闭。
 *
 * 交互行为：
 *   - 点击菜单按钮切换展开/收起
 *   - 点击子菜单项后自动收起
 *   - 点击组件外部区域自动收起
 *   - 按 Escape 键收起
 *
 * 视觉遵循 DESIGN.md：圆角卡片、hairline 边框、surface 背景、阴影。
 */
import { ref, onMounted, onUnmounted } from 'vue';

/** 子菜单项定义。 */
export interface MenuItem {
  /** 唯一标识（用于 i18n key 匹配与事件回调）。 */
  id: string;
  /** 显示文本。 */
  label: string;
  /** 是否禁用（置灰不可点击）。 */
  disabled?: boolean;
}

const props = defineProps<{
  /** 子菜单项列表。 */
  items: MenuItem[];
  /** 是否展开。 */
  visible: boolean;
}>();

const emit = defineEmits<{
  /** 选中某项时触发。 */
  (e: 'select', id: string): void;
  /** 请求关闭菜单（外部点击 / Escape）。 */
  (e: 'close'): void;
}>();

const dropdownRef = ref<HTMLElement | null>(null);

/** 处理子菜单项点击。 */
function handleItemClick(id: string, disabled?: boolean): void {
  if (disabled) return;
  emit('select', id);
  emit('close');
}

/** 外部点击检测。 */
function handleDocumentClick(e: MouseEvent): void {
  if (!props.visible) return;
  if (dropdownRef.value && !dropdownRef.value.contains(e.target as Node)) {
    emit('close');
  }
}

/** Escape 键关闭。 */
function handleKeydown(e: KeyboardEvent): void {
  if (e.key === 'Escape' && props.visible) {
    emit('close');
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
  <Transition name="dropdown">
    <div v-if="visible" ref="dropdownRef" class="menu-dropdown">
      <button
        v-for="item in items"
        :key="item.id"
        class="menu-dropdown__item"
        :class="{ 'menu-dropdown__item--disabled': item.disabled }"
        data-tauri-drag-region="false"
        @click="handleItemClick(item.id, item.disabled)"
      >
        <span class="menu-dropdown__label">{{ item.label }}</span>
      </button>
    </div>
  </Transition>
</template>

<style scoped>
.menu-dropdown {
  position: absolute;
  top: 100%;
  left: 0;
  margin-top: 4px;
  min-width: 160px;
  padding: 4px;
  background: var(--fluen-surface);
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  box-shadow: var(--fluen-shadow-card);
  z-index: 1000;
  -webkit-app-region: no-drag;
}

.menu-dropdown__item {
  display: flex;
  align-items: center;
  width: 100%;
  padding: 6px 12px;
  border: none;
  background: transparent;
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  font-weight: 400;
  text-align: left;
  cursor: pointer;
  border-radius: 4px;
  transition: background 0.12s ease, color 0.12s ease;
  white-space: nowrap;
}

.menu-dropdown__item:hover {
  background: var(--fluen-hover);
}

.menu-dropdown__item--disabled {
  color: var(--fluen-stone);
  cursor: not-allowed;
  pointer-events: none;
}

/* ── 过渡动画 ─────────────────────────────────────────────────────── */
.dropdown-enter-active,
.dropdown-leave-active {
  transition: opacity 0.15s ease, transform 0.15s ease;
}

.dropdown-enter-from,
.dropdown-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
