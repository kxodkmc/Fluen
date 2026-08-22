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
import { ref, computed, reactive, provide, onMounted, onUnmounted } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useI18n } from '../../i18n';
import { SETTINGS_SECTIONS, DEFAULT_SETTINGS_SECTION } from './constants';
import { SETTINGS_NAVIGATE_KEY } from './types';
import type { SettingsSection, SettingsSectionId } from './types';

defineEmits<{
  /** 返回主界面。 */
  (e: 'back'): void;
}>();

const { t } = useI18n();

/* ── 激活分区 ───────────────────────────────────────────────────────── */
const activeSectionId = ref(DEFAULT_SETTINGS_SECTION);

/** 已展开的分组分区 ID 集合。 */
const expandedGroups = reactive(new Set<string>());

/** 递归查找分区（含分组子项）。 */
function findSection(id: string): SettingsSection | null {
  for (const s of SETTINGS_SECTIONS) {
    if (s.id === id) return s;
    const child = s.children?.find((c) => c.id === id);
    if (child) return child;
  }
  return null;
}

/** 返回包含指定子分区的父分组。 */
function parentGroupOf(id: string): SettingsSection | null {
  return SETTINGS_SECTIONS.find((s) => s.children?.some((c) => c.id === id)) ?? null;
}

const activeSection = computed(
  () => findSection(activeSectionId.value) ?? SETTINGS_SECTIONS[0],
);

function isGroupOpen(section: SettingsSection): boolean {
  return expandedGroups.has(section.id);
}

/** 展开 / 折叠分组。展开时若未激活其子项则默认选中第一个。 */
function toggleGroup(section: SettingsSection): void {
  if (isGroupOpen(section)) {
    expandedGroups.delete(section.id);
  } else {
    expandedGroups.add(section.id);
    const activeInside = section.children?.some((c) => c.id === activeSectionId.value);
    if (!activeInside && section.children?.length) {
      activeSectionId.value = section.children[0].id;
    }
  }
}

/** 选中分组子分区（自动展开父分组）。 */
function selectChild(child: SettingsSection): void {
  const parent = parentGroupOf(child.id);
  if (parent) expandedGroups.add(parent.id);
  activeSectionId.value = child.id;
}

/* ── 分区导航（提供给子分区组件使用） ───────────────────────────────── */
function navigateToSection(id: SettingsSectionId): void {
  const parent = parentGroupOf(id);
  if (parent) expandedGroups.add(parent.id);
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
        <template v-for="section in SETTINGS_SECTIONS" :key="section.id">
          <!-- 分组（可折叠） -->
          <div v-if="section.children?.length" class="settings-sidebar__group">
            <button
              class="settings-sidebar__item settings-sidebar__group-toggle"
              @click="toggleGroup(section)"
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
              <svg
                class="settings-sidebar__chevron"
                :class="{ 'settings-sidebar__chevron--open': isGroupOpen(section) }"
                viewBox="0 0 24 24"
                width="14"
                height="14"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path d="m6 9 6 6 6-6" />
              </svg>
            </button>
            <Transition name="nav-expand">
              <div v-if="isGroupOpen(section)" class="settings-sidebar__children">
                <button
                  v-for="child in section.children"
                  :key="child.id"
                  class="settings-sidebar__item settings-sidebar__child"
                  :class="{ 'settings-sidebar__item--active': activeSectionId === child.id }"
                  @click="selectChild(child)"
                >
                  <span class="settings-sidebar__label">{{ t(child.labelKey) }}</span>
                </button>
              </div>
            </Transition>
          </div>

          <!-- 普通分区 -->
          <button
            v-else
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
        </template>
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

/* ── 分组 ───────────────────────────────────────────────────────────── */
.settings-sidebar__group {
  display: flex;
  flex-direction: column;
}

/* 分组标题（父项）：加粗、更深色，与普通分区/子项区分 */
.settings-sidebar__group-toggle {
  justify-content: flex-start;
  font-weight: 500;
  color: var(--fluen-ink);
}

.settings-sidebar__chevron {
  flex-shrink: 0;
  margin-left: auto;
  color: var(--fluen-slate);
  transition: transform 0.2s ease;
}

.settings-sidebar__chevron--open {
  transform: rotate(180deg);
}

/* 子项容器：明确缩进 + 左侧引导线，直观体现层级嵌套 */
.settings-sidebar__children {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin-left: 13px;
  padding-left: 12px;
  border-left: 1px solid var(--fluen-hairline);
  border-left-color: color-mix(in srgb, var(--fluen-stone) 40%, transparent);
}

/* 子项：更浅文字色 + 更小字号，进一步弱化次要层级 */
.settings-sidebar__child {
  font-size: 0.78rem;
  padding: 7px 10px;
  color: var(--fluen-steel);
}

.settings-sidebar__child:hover {
  color: var(--fluen-ink);
}

.settings-sidebar__item--active.settings-sidebar__child {
  color: var(--fluen-accent);
  background: var(--fluen-info-bg);
  font-weight: 500;
  box-shadow: none;
}

/* ── 内容区 ─────────────────────────────────────────────────────────── */
.settings-content {
  flex: 1;
  padding: 32px 40px;
  overflow-y: auto;
  background: var(--fluen-canvas);
}

/* ── 分组展开动画 ───────────────────────────────────────────────────── */
.nav-expand-enter-active,
.nav-expand-leave-active {
  transition: opacity 0.2s ease, max-height 0.2s ease;
  overflow: hidden;
}

.nav-expand-enter-from,
.nav-expand-leave-to {
  opacity: 0;
  max-height: 0;
}

.nav-expand-enter-to,
.nav-expand-leave-from {
  max-height: 200px;
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
