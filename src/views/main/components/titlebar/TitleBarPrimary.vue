<script setup lang="ts">
/**
 * TitleBarPrimary — 顶部标题栏左侧 Dock。
 *
 * 承载应用标识（图标 + 标题）与主菜单（文件 / 编辑 / 视图 / 帮助）。
 * 点击菜单按钮弹出一级下拉子菜单。
 *
 * 响应式：
 *   - 宽屏：图标 + 标题 + 完整菜单
 *   - ≤ menuCollapse 断点：标题文字隐藏，菜单文字隐藏（仅保留菜单按钮形态）
 *
 * 整体为 Tauri 拖拽区域；交互元素通过 data-tauri-drag-region="false" 排除拖拽。
 */
import { ref, computed } from 'vue';
import { TITLE_BAR_MENU_IDS, FILE_MENU_ITEM_IDS, HELP_MENU_ITEM_IDS } from '../../constants';
import { APP_NAME } from '../../../../utils/appInfo';
import { useI18n } from '../../../../i18n';
import TitleBarMenuDropdown from './TitleBarMenuDropdown.vue';
import type { MenuItem } from './TitleBarMenuDropdown.vue';

const { t } = useI18n();

defineProps<{
  /** 是否处于窄屏折叠态（标题与菜单文字隐藏）。 */
  compact?: boolean;
}>();

const emit = defineEmits<{
  /** 选择文件菜单子项时触发。 */
  (e: 'menu-select', itemId: string): void;
}>();

/* ── 下拉菜单状态 ─────────────────────────────────────────────────── */
/** 当前展开的菜单 ID（null 表示无展开）。 */
const activeMenu = ref<string | null>(null);

/** 文件菜单子项列表。 */
const fileMenuItems = computed<MenuItem[]>(() =>
  FILE_MENU_ITEM_IDS.map((id) => ({
    id,
    label: t(`main.titleBar.menus.filesItems.${id}`),
  })),
);

/** 帮助菜单子项列表。 */
const helpMenuItems = computed<MenuItem[]>(() =>
  HELP_MENU_ITEM_IDS.map((id) => ({
    id,
    label: t(`main.titleBar.menus.helpItems.${id}`),
  })),
);

/** 切换菜单展开/收起。 */
function toggleMenu(menuId: string): void {
  activeMenu.value = activeMenu.value === menuId ? null : menuId;
}

/** 关闭菜单。 */
function closeMenu(): void {
  activeMenu.value = null;
}

/** 处理子菜单项选择。 */
function handleSelect(itemId: string): void {
  emit('menu-select', itemId);
}
</script>

<template>
  <div class="dock-primary" data-tauri-drag-region>
    <!-- 应用标识：图标 + 标题 -->
    <div class="dock-primary__brand" data-tauri-drag-region="false">
      <svg
        class="dock-primary__logo"
        viewBox="0 0 24 24"
        width="18"
        height="18"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
      >
        <path d="M4 4h12v16H4V4z" />
        <path d="M18 8v12a2 2 0 0 1-2 2" />
        <path d="M8 8h4M8 12h4M8 16h2" />
      </svg>
      <span v-if="!compact" class="dock-primary__title">{{ APP_NAME }}</span>
    </div>

    <span v-if="!compact" class="dock-primary__separator" />

    <!-- 主菜单 -->
    <nav class="dock-primary__menu" :class="{ 'dock-primary__menu--compact': compact }">
      <div
        v-for="id in TITLE_BAR_MENU_IDS"
        :key="id"
        class="dock-primary__menu-wrapper"
      >
        <button
          class="dock-primary__menu-item"
          :class="{
            'dock-primary__menu-item--icon-only': compact,
            'dock-primary__menu-item--active': activeMenu === id,
          }"
          :title="compact ? t('main.titleBar.menus.' + id) : undefined"
          data-tauri-drag-region="false"
          @click="toggleMenu(id)"
        >
          <span v-if="!compact">{{ t('main.titleBar.menus.' + id) }}</span>
          <svg
            v-else
            viewBox="0 0 24 24"
            width="14"
            height="14"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
          >
            <circle cx="5" cy="12" r="1.2" />
            <circle cx="12" cy="12" r="1.2" />
            <circle cx="19" cy="12" r="1.2" />
          </svg>
        </button>

        <!-- 文件菜单子菜单 -->
        <TitleBarMenuDropdown
          v-if="id === 'files'"
          :items="fileMenuItems"
          :visible="activeMenu === 'files'"
          @select="handleSelect"
          @close="closeMenu"
        />

        <!-- 帮助菜单子菜单 -->
        <TitleBarMenuDropdown
          v-if="id === 'help'"
          :items="helpMenuItems"
          :visible="activeMenu === 'help'"
          @select="handleSelect"
          @close="closeMenu"
        />
      </div>
    </nav>
  </div>
</template>

<style scoped>
.dock-primary {
  /* 独立浮动的圆角矩形 Dock */
  display: flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 8px 0 6px;
  background: var(--fluen-surface);
  border: 1px solid var(--fluen-hairline);
  border-radius: 12px;
  box-shadow: var(--fluen-shadow-card);
  -webkit-app-region: drag;
  transition: border-color 0.15s ease, box-shadow 0.15s ease;
}

.dock-primary:hover {
  border-color: var(--fluen-accent);
  box-shadow: var(--fluen-shadow-card);
}

/* ── 品牌区 ──────────────────────────────────────────────────────────── */
.dock-primary__brand {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 2px;
}

.dock-primary__logo {
  flex-shrink: 0;
  color: var(--fluen-accent);
}

.dock-primary__title {
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  font-weight: 600;
  letter-spacing: -0.2px;
  color: var(--fluen-ink);
  white-space: nowrap;
}

.dock-primary__separator {
  width: 1px;
  height: 14px;
  background: var(--fluen-hairline);
  flex-shrink: 0;
}

/* ── 菜单 ─────────────────────────────────────────────────────────────── */
.dock-primary__menu {
  display: flex;
  align-items: center;
  gap: 1px;
}

.dock-primary__menu-wrapper {
  position: relative;
}

.dock-primary__menu-item {
  display: flex;
  align-items: center;
  justify-content: center;
  min-width: 28px;
  height: 24px;
  padding: 0 10px;
  border: none;
  background: transparent;
  color: var(--fluen-slate);
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  font-weight: 400;
  cursor: pointer;
  border-radius: 4px;
  transition: background 0.15s ease, color 0.15s ease;
  -webkit-app-region: no-drag;
}

.dock-primary__menu-item:hover,
.dock-primary__menu-item--active {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.dock-primary__menu-item--icon-only {
  padding: 0;
  width: 24px;
}

/* ── 紧凑态 ───────────────────────────────────────────────────────────── */
.dock-primary__menu--compact {
  gap: 2px;
}

/* ── 响应式：极窄屏 ───────────────────────────────────────────────────── */
@media (max-width: 420px) {
  .dock-primary {
    padding: 0 6px;
  }
}
</style>
