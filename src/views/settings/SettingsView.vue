<script setup lang="ts">
/**
 * SettingsView — 设置页面顶层容器。
 *
 * 布局：
 *   ┌─────────────────────────────────────────────────────┐
 *   │  ← 设置                                      ─ □ ✕  │  Header
 *   ├──────────┬──────────────────────────────────────────┤
 *   │          │                                          │
 *   │  Sidebar │              Content                     │
 *   │  (nav)   │          (active section)                │
 *   │          │                                          │
 *   └──────────┴──────────────────────────────────────────┘
 *
 * Header 包含返回按钮、标题、窗口控制（frameless window 需要）。
 * Sidebar 从 SETTINGS_SECTIONS 注册表渲染分区导航。
 * Content 根据 activeSectionId 动态渲染对应组件。
 *
 * 扩展方式：在 constants.ts 中添加新的 section 注册项即可。
 */
import { ref, computed, provide, onMounted, onUnmounted } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useI18n } from '../../i18n';
import { SETTINGS_SECTIONS, DEFAULT_SETTINGS_SECTION } from './constants';
import { SETTINGS_NAVIGATE_KEY } from './types';
import type { SettingsSectionId } from './types';

defineEmits<{
  /** 返回主界面。 */
  (e: 'back'): void;
}>();

const { t } = useI18n();

/* ── 激活分区 ───────────────────────────────────────────────────────── */
const activeSectionId = ref(DEFAULT_SETTINGS_SECTION);

const activeSection = computed(() =>
  SETTINGS_SECTIONS.find((s) => s.id === activeSectionId.value) ?? SETTINGS_SECTIONS[0],
);

/* ── 分区导航（提供给子分区组件使用） ───────────────────────────────── */
function navigateToSection(id: SettingsSectionId): void {
  activeSectionId.value = id;
}
provide(SETTINGS_NAVIGATE_KEY, navigateToSection);

/* ── 窗口控制（frameless window 需要） ──────────────────────────────── */
const appWindow = getCurrentWindow();
const isMaximized = ref(false);
let unlistenResize: (() => void) | null = null;

onMounted(async () => {
  isMaximized.value = await appWindow.isMaximized();
  unlistenResize = await appWindow.onResized(async () => {
    isMaximized.value = await appWindow.isMaximized();
  });
});

onUnmounted(() => {
  unlistenResize?.();
});

function handleMinimize(): void {
  appWindow.minimize();
}

function handleMaximize(): void {
  appWindow.toggleMaximize();
}

function handleClose(): void {
  appWindow.close();
}
</script>

<template>
  <div class="settings-view">
    <!-- ── Header ─────────────────────────────────────────────────────── -->
    <header class="settings-header" data-tauri-drag-region>
      <div class="settings-header__left" data-tauri-drag-region="false">
        <button class="settings-header__back" :title="t('settings.back')" @click="$emit('back')">
          <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M19 12H5" />
            <path d="m12 19-7-7 7-7" />
          </svg>
        </button>
        <span class="settings-header__title">{{ t('settings.title') }}</span>
      </div>

      <!-- 窗口控制 -->
      <div class="settings-header__window">
        <button class="win-btn" :title="t('main.titleBar.controls.minimize')" @click="handleMinimize">
          <svg viewBox="0 0 12 12" width="12" height="12"><rect y="5.5" width="12" height="1" fill="currentColor" /></svg>
        </button>
        <button
          class="win-btn"
          :title="isMaximized ? t('main.titleBar.controls.restore') : t('main.titleBar.controls.maximize')"
          @click="handleMaximize"
        >
          <svg v-if="!isMaximized" viewBox="0 0 12 12" width="12" height="12">
            <rect x="1" y="1" width="10" height="10" stroke="currentColor" fill="none" stroke-width="1" />
          </svg>
          <svg v-else viewBox="0 0 12 12" width="12" height="12">
            <rect x="1" y="3" width="8" height="8" stroke="currentColor" fill="none" stroke-width="1" />
            <path d="M3 3V1h8v8h-2" stroke="currentColor" fill="none" stroke-width="1" />
          </svg>
        </button>
        <button class="win-btn win-btn--close" :title="t('main.titleBar.controls.close')" @click="handleClose">
          <svg viewBox="0 0 12 12" width="12" height="12"><path d="M1 1l10 10M11 1L1 11" stroke="currentColor" stroke-width="1.2" /></svg>
        </button>
      </div>
    </header>

    <!-- ── 主体：侧边栏 + 内容区 ─────────────────────────────────────── -->
    <div class="settings-body">
      <!-- 侧边栏导航 -->
      <nav class="settings-sidebar">
        <button
          v-for="section in SETTINGS_SECTIONS"
          :key="section.id"
          class="settings-sidebar__item"
          :class="{ 'settings-sidebar__item--active': activeSectionId === section.id }"
          @click="activeSectionId = section.id"
        >
          <svg
            class="settings-sidebar__icon"
            viewBox="0 0 24 24"
            width="18"
            height="18"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            v-html="section.icon"
          />
          <span class="settings-sidebar__label">{{ t(section.labelKey) }}</span>
        </button>
      </nav>

      <!-- 内容区 -->
      <main class="settings-content">
        <component :is="activeSection.component" />
      </main>
    </div>
  </div>
</template>

<style scoped>
.settings-view {
  display: flex;
  flex-direction: column;
  height: 100vh;
  width: 100vw;
  overflow: hidden;
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
}

/* ── Header ─────────────────────────────────────────────────────────── */
.settings-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 44px;
  padding: 0 8px 0 12px;
  flex-shrink: 0;
  background: var(--fluen-canvas);
  user-select: none;
  -webkit-app-region: drag;
  border-bottom: 1px solid var(--fluen-hairline);
}

.settings-header__left {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  -webkit-app-region: no-drag;
}

.settings-header__back {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--fluen-slate);
  cursor: pointer;
  border-radius: 6px;
  transition: background 0.15s ease, color 0.15s ease;
}

.settings-header__back:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.settings-header__title {
  font-family: var(--fluen-font-sans);
  font-size: 14px;
  font-weight: 600;
  color: var(--fluen-ink);
}

.settings-header__window {
  display: flex;
  align-items: center;
  -webkit-app-region: no-drag;
}

.win-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--fluen-slate);
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease;
}

.win-btn:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.win-btn--close:hover {
  background: var(--fluen-error);
  color: var(--fluen-on-accent);
}

/* ── 主体 ───────────────────────────────────────────────────────────── */
.settings-body {
  flex: 1;
  display: flex;
  overflow: hidden;
  min-height: 0;
}

/* ── 侧边栏 ─────────────────────────────────────────────────────────── */
.settings-sidebar {
  width: 220px;
  flex-shrink: 0;
  padding: 16px 12px;
  background: var(--fluen-surface);
  border-right: 1px solid var(--fluen-hairline);
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.settings-sidebar__item {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  padding: 8px 12px;
  border: none;
  background: transparent;
  color: var(--fluen-charcoal);
  font-family: var(--fluen-font-sans);
  font-size: 0.85rem;
  font-weight: 400;
  text-align: left;
  cursor: pointer;
  border-radius: 6px;
  transition: background 0.15s ease, color 0.15s ease;
}

.settings-sidebar__item:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.settings-sidebar__item--active {
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  font-weight: 500;
  box-shadow: var(--fluen-shadow-card);
}

.settings-sidebar__icon {
  flex-shrink: 0;
  color: var(--fluen-slate);
}

.settings-sidebar__item--active .settings-sidebar__icon {
  color: var(--fluen-accent);
}

/* ── 内容区 ─────────────────────────────────────────────────────────── */
.settings-content {
  flex: 1;
  padding: 32px 40px;
  overflow-y: auto;
  background: var(--fluen-canvas);
}

/* ── 响应式：窄屏 ───────────────────────────────────────────────────── */
@media (max-width: 768px) {
  .settings-sidebar {
    width: 60px;
    padding: 16px 8px;
  }

  .settings-sidebar__label {
    display: none;
  }

  .settings-sidebar__item {
    justify-content: center;
    padding: 10px;
  }

  .settings-content {
    padding: 20px;
  }
}
</style>
