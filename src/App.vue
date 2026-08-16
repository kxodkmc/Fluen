<script setup lang="ts">
/**
 * App root — shows the onboarding flow on first launch.
 *
 * When the user completes onboarding (emits `enter`), the view switches
 * to the main application surface (MainView).
 *
 * The root wrapper manages the frameless window border-radius:
 *   - decorations: false in tauri.conf.json removes the system title bar
 *   - transparent: true lets the rounded corners show through
 *   - the .app-root border-radius is removed while maximized to avoid
 *     clipped corners on a full-screen window
 */
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { OnboardingView, MainView, SettingsView, KnowledgeGraphView } from './views';
import { ThemeProvider } from './theme';
import { useAppConfig } from './composables/useAppConfig';
import { useI18n } from './i18n';
import { startShortcuts, stopShortcuts } from './shortcuts';

const onboarded = ref(false);

/** 当前视图状态：onboarding 完成后在 main / settings 之间切换。 */
const currentView = ref<'main' | 'settings'>('main');

/**
 * 是否以网状图子窗口模式启动。
 *
 * 子窗口由 `openKnowledgeGraphWindow` 创建，URL 携带 `?view=graph` 参数。
 * 该窗口跳过 onboarding 与主界面，直接渲染 `KnowledgeGraphView`。
 */
const isGraphWindow = computed(() => {
  try {
    const params = new URLSearchParams(window.location.search);
    return params.get('view') === 'graph';
  } catch {
    return false;
  }
});

const handleEnter = (): void => {
  onboarded.value = true;
};

/** 跳转到设置页面。 */
function handleNavigateSettings(): void {
  currentView.value = 'settings';
}

/** 从设置页面返回主界面。 */
function handleBackToMain(): void {
  currentView.value = 'main';
}

/* ── 无边框窗口圆角状态 ───────────────────────────────────────────────── */
const appWindow = getCurrentWindow();
const isMaximized = ref(false);
let unlistenResize: (() => void) | null = null;

/* ── i18n 实例必须在 setup 顶层获取（onMounted 内调用会触发
 * "Must be called at the top of a setup function" 错误） ───────────── */
const { setLocale } = useI18n();
const { loadConfig } = useAppConfig();

onMounted(async () => {
  /* ── 启动全局快捷键监听（capture 阶段统一响应，拦截 WebView2 默认行为） ── */
  startShortcuts();

  isMaximized.value = await appWindow.isMaximized();
  unlistenResize = await appWindow.onResized(async () => {
    isMaximized.value = await appWindow.isMaximized();
  });

  /* ── 启动时从持久化配置同步语言到 i18n 实例 ───────────────────────── */
  // persist: false 避免循环写回（loadConfig 已是配置源）
  const { language } = await loadConfig();
  setLocale(language, { persist: false });
});

onUnmounted(() => {
  unlistenResize?.();
  stopShortcuts();
});
</script>

<template>
  <ThemeProvider>
    <!-- 网状图子窗口：跳过主界面流程，直接渲染网状图视图 -->
    <KnowledgeGraphView v-if="isGraphWindow" />
    <div v-else class="app-root" :class="{ 'app-root--maximized': isMaximized }">
      <OnboardingView v-if="!onboarded" @enter="handleEnter" />
      <SettingsView
        v-else-if="currentView === 'settings'"
        @back="handleBackToMain"
      />
      <MainView v-else @navigate-settings="handleNavigateSettings" />
    </div>
  </ThemeProvider>
</template>

<style>
:root {
  font-family: var(--fluen-font-sans);
  color: var(--fluen-ink);
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

html,
body,
#app {
  height: 100%;
  margin: 0;
}

body {
  background: transparent;
}

.app-root {
  height: 100%;
  border-radius: 12px;
  overflow: hidden;
}

.app-root--maximized {
  border-radius: 0;
}
</style>


