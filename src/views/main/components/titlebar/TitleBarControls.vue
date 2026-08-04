<script setup lang="ts">
/**
 * TitleBarControls — 顶部标题栏右侧 Dock。
 *
 * 承载两类控制：
 *   1. 面板折叠切换（功能区 / AI 区）
 *   2. 窗口控制（最小化 / 最大化·还原 / 关闭）
 *
 * 响应式：
 *   - 宽屏：显示折叠按钮 + 分隔条 + 三个窗口按钮
 *   - ≤ collapse 断点：隐藏面板折叠按钮，仅保留窗口控制
 *
 * 窗口控制按钮调用 Tauri 窗口 API，因为 tauri.conf.json 已关闭系统装饰。
 */
import { ref, onMounted, onUnmounted } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useI18n } from '../../../../i18n';

const { t } = useI18n();

defineProps<{
  /** 是否处于窄屏态（隐藏面板折叠按钮）。 */
  compact?: boolean;
}>();

defineEmits<{
  (e: 'toggle-panel', panel: 'function' | 'content' | 'ai'): void;
  /** 跳转到设置页面。 */
  (e: 'navigate-settings'): void;
}>();

/* ── 窗口控制 ─────────────────────────────────────────────────────────── */
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
  <div class="dock-controls" data-tauri-drag-region>
    <!-- 设置按钮 -->
    <button
      class="dock-controls__toggle"
      :title="t('settings.title')"
      data-tauri-drag-region="false"
      @click="$emit('navigate-settings')"
    >
      <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
        <circle cx="12" cy="12" r="3" />
      </svg>
    </button>

    <span class="dock-controls__divider" />

    <!-- 面板折叠切换 -->
    <div v-if="!compact" class="dock-controls__toggles">
      <button
        class="dock-controls__toggle"
        :title="t('main.titleBar.controls.toggleFunction')"
        data-tauri-drag-region="false"
        @click="$emit('toggle-panel', 'function')"
      >
        <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2">
          <rect x="3" y="3" width="6" height="18" rx="1" />
          <rect x="11" y="3" width="10" height="18" rx="1" />
        </svg>
      </button>
      <button
        class="dock-controls__toggle"
        :title="t('main.titleBar.controls.toggleAi')"
        data-tauri-drag-region="false"
        @click="$emit('toggle-panel', 'ai')"
      >
        <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2">
          <rect x="3" y="3" width="14" height="18" rx="1" />
          <rect x="19" y="3" width="2" height="18" rx="1" />
        </svg>
      </button>
    </div>

    <span v-if="!compact" class="dock-controls__divider" />

    <!-- 窗口控制 -->
    <div class="dock-controls__window">
      <button
        class="dock-controls__win-btn"
        :title="t('main.titleBar.controls.minimize')"
        data-tauri-drag-region="false"
        @click="handleMinimize"
      >
        <svg viewBox="0 0 12 12" width="12" height="12"><rect y="5.5" width="12" height="1" fill="currentColor" /></svg>
      </button>
      <button
        class="dock-controls__win-btn"
        :title="isMaximized ? t('main.titleBar.controls.restore') : t('main.titleBar.controls.maximize')"
        data-tauri-drag-region="false"
        @click="handleMaximize"
      >
        <svg
          v-if="!isMaximized"
          viewBox="0 0 12 12"
          width="12"
          height="12"
        >
          <rect x="1" y="1" width="10" height="10" stroke="currentColor" fill="none" stroke-width="1" />
        </svg>
        <svg
          v-else
          viewBox="0 0 12 12"
          width="12"
          height="12"
        >
          <rect x="1" y="3" width="8" height="8" stroke="currentColor" fill="none" stroke-width="1" />
          <path d="M3 3V1h8v8h-2" stroke="currentColor" fill="none" stroke-width="1" />
        </svg>
      </button>
      <button
        class="dock-controls__win-btn dock-controls__win-btn--close"
        :title="t('main.titleBar.controls.close')"
        data-tauri-drag-region="false"
        @click="handleClose"
      >
        <svg viewBox="0 0 12 12" width="12" height="12"><path d="M1 1l10 10M11 1L1 11" stroke="currentColor" stroke-width="1.2" /></svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
.dock-controls {
  /* 独立浮动的圆角矩形 Dock */
  display: flex;
  align-items: center;
  gap: 4px;
  height: 32px;
  padding: 0 4px 0 6px;
  background: var(--fluen-surface);
  border: 1px solid var(--fluen-hairline);
  border-radius: 12px;
  box-shadow: var(--fluen-shadow-card);
  -webkit-app-region: drag;
  transition: border-color 0.15s ease, box-shadow 0.15s ease;
}

.dock-controls:hover {
  border-color: var(--fluen-accent);
  box-shadow: var(--fluen-shadow-card);
}

/* ── 面板折叠按钮 ─────────────────────────────────────────────────────── */
.dock-controls__toggles {
  display: flex;
  align-items: center;
  gap: 2px;
}

.dock-controls__toggle {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 24px;
  border: none;
  background: transparent;
  color: var(--fluen-slate);
  cursor: pointer;
  border-radius: 4px;
  transition: background 0.15s ease, color 0.15s ease;
  -webkit-app-region: no-drag;
}

.dock-controls__toggle:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.dock-controls__divider {
  width: 1px;
  height: 14px;
  background: var(--fluen-hairline);
  flex-shrink: 0;
  margin: 0 2px;
}

/* ── 窗口控制按钮 ─────────────────────────────────────────────────────── */
.dock-controls__window {
  display: flex;
  align-items: center;
}

.dock-controls__win-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 24px;
  border: none;
  background: transparent;
  color: var(--fluen-slate);
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease;
  -webkit-app-region: no-drag;
}

.dock-controls__win-btn:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.dock-controls__win-btn--close:hover {
  background: var(--fluen-error);
  color: var(--fluen-on-accent);
}
</style>
