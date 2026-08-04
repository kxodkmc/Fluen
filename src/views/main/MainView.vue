<script setup lang="ts">
/**
 * MainView — 主界面顶层布局容器。
 *
 * 编排三段式布局：
 *   ┌─────────────────────────────────────────────────────┐
 *   │                    TitleBar                          │  顶部标题栏
 *   ├──────┬──────────────────────────────┬───────────────┤
 *   │      │                              │               │
 *   │ Func │       Content Panel          │  Right Panel  │
 *   │ Panel│       (编辑器 / 标签页)        │ (Motis/助手)  │
 *   │      │                              │               │
 *   ├──────┴──────────────────────────────┴───────────────┤
 *   │                    StatusBar                         │  底部状态栏
 *   └─────────────────────────────────────────────────────┘
 *
 * 布局状态由 `useMainLayout` composable 统一管理；
 * 项目状态由 `useProject` composable（单例）管理。
 * 各面板组件通过 props / emits 与容器交互。
 */
import { ref, watch, provide } from 'vue';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import TitleBar from './components/TitleBar.vue';
import FunctionPanel from './components/FunctionPanel.vue';
import ContentPanel from './components/ContentPanel.vue';
import { RightPanel } from './components/rightpanel';
import StatusBar from './components/statusbar/StatusBar.vue';
import { useProjectStatus } from './components/statusbar';
import NewProjectDialog from '../../components/NewProjectDialog.vue';
import { useMainLayout, MAIN_LAYOUT_KEY } from './composables/useMainLayout';
import { useMotisChat } from './composables/useMotisChat';
import { MOTIS_CHAT_KEY } from './components/motis';
import { useProject } from '../../composables/useProject';
import { PANEL_CONSTRAINTS } from './constants';
import { useI18n } from '../../i18n';
import type { ContentTab } from './types';

const { t } = useI18n();

/* ── 布局状态 ─────────────────────────────────────────────────────────── */
const layout = useMainLayout();
// 将布局实例 provide 给整个组件树，供 TitleBar、TitleBarSearch 等子组件共享
// （与下方 motisChat 的 provide/inject 模式一致）
provide(MAIN_LAYOUT_KEY, layout);

/* ── 项目状态（单例） ─────────────────────────────────────────────────── */
const { hasProject, config, openProject } = useProject();

/* ── 状态栏：注册项目状态条目 ───────────────────────────────────────────── */
useProjectStatus();

/* ── Motis 对话状态（全局共享实例） ────────────────────────────────────── */
// 在 MainView 创建 useMotisChat 实例并 provide 给整个组件树，
// 确保 MotisPanel、TitleBar(Mascot) 等子组件共享同一份对话状态。
const motisChat = useMotisChat();
provide(MOTIS_CHAT_KEY, motisChat);

/* ── 拖拽调整面板宽度 ─────────────────────────────────────────────────── */
let resizing: 'function' | 'ai' | null = null;
let startX = 0;
let startWidth = 0;

function startResize(e: MouseEvent, panel: 'function' | 'ai'): void {
  e.preventDefault();
  resizing = panel;
  startX = e.clientX;
  startWidth = panel === 'function'
    ? layout.functionPanelWidth.value
    : layout.aiPanelWidth.value;
  document.addEventListener('mousemove', onResizeMove);
  document.addEventListener('mouseup', stopResize);
}

function onResizeMove(e: MouseEvent): void {
  if (!resizing) return;
  const delta = e.clientX - startX;

  if (resizing === 'function') {
    const { min, max } = PANEL_CONSTRAINTS.functionPanel;
    layout.setFunctionPanelWidth(Math.min(max, Math.max(min, startWidth + delta)));
  } else if (resizing === 'ai') {
    const { min, max } = PANEL_CONSTRAINTS.aiPanel;
    layout.setAIPanelWidth(Math.min(max, Math.max(min, startWidth - delta)));
  }
}

function stopResize(): void {
  resizing = null;
  document.removeEventListener('mousemove', onResizeMove);
  document.removeEventListener('mouseup', stopResize);
}

defineEmits<{
  /** 跳转到设置页面。 */
  (e: 'navigate-settings'): void;
}>();

/* ── 标签页操作占位 ───────────────────────────────────────────────────── */
function openPlaceholderTab(): void {
  const id = `tab-${Date.now()}`;
  const tab: ContentTab = {
    id,
    title: t('main.content.newDocName'),
    type: 'file',
    dirty: true,
  };
  layout.openTab(tab);
}

/* ── 新建文章对话框 ───────────────────────────────────────────────────── */
const showNewProjectDialog = ref(false);

/** 处理标题栏菜单选择。 */
async function handleMenuSelect(itemId: string): Promise<void> {
  if (itemId === 'newArticle') {
    showNewProjectDialog.value = true;
  } else if (itemId === 'openArticle') {
    await handleOpenArticle();
  }
}

/** 打开文章 — 选择文件夹并加载项目。 */
async function handleOpenArticle(): Promise<void> {
  try {
    const selected = await openDialog({ directory: true, multiple: false });
    if (typeof selected === 'string' && selected) {
      await openProject(selected);
    }
  } catch {
    // 用户取消或出错，静默处理
  }
}

/** 新建文章成功回调。 */
function handleProjectCreated(projectPath: string): void {
  // 创建成功后自动打开项目
  openProject(projectPath);
}

/* ── 项目打开后自动创建标签页 ─────────────────────────────────────────── */
watch(
  [hasProject, config],
  ([opened, cfg]) => {
    if (opened && cfg) {
      // 创建或激活项目标签页
      const tabId = `project-${cfg.title}`;
      layout.openTab({
        id: tabId,
        title: cfg.title,
        type: 'editor',
        icon: 'M4 4h12v16H4V4zm2 4h8m-8 4h8m-8 4h5M18 8v12a2 2 0 0 1-2 2',
      });

      // 自动切换到大纲面板
      layout.setActiveActivity('outline');
    }
  },
  { immediate: true },
);
</script>

<template>
  <div class="main-view">
    <!-- ── 顶部标题栏 ─────────────────────────────────────────────────── -->
    <TitleBar @toggle-panel="layout.togglePanel" @menu-select="handleMenuSelect" @navigate-settings="$emit('navigate-settings')" />

    <!-- ── 主体三段布局 ───────────────────────────────────────────────── -->
    <div class="main-view__body">
      <!-- 左侧功能区：活动栏（始终可见）+ 侧边栏（可收起） -->
      <FunctionPanel
        :active-activity="layout.activeActivity.value"
        :collapsed="layout.functionPanelCollapsed.value"
        :width="layout.functionPanelWidth.value"
        @select-activity="layout.setActiveActivity"
      />
      <!-- 分隔条：仅在功能区展开时显示 -->
      <div
        v-if="!layout.functionPanelCollapsed.value"
        class="main-view__resizer"
        @mousedown="startResize($event, 'function')"
      />

      <!-- 中间内容区 -->
      <ContentPanel
        :tabs="layout.tabs.value"
        :active-tab-id="layout.activeTabId.value"
        @select-tab="layout.setActiveTab"
        @close-tab="layout.closeTab"
      />

      <!-- 右侧面板区（Motis 对话 / 学术助手） -->
      <template v-if="layout.rightPanelVisible.value">
        <!-- 分隔条 -->
        <div
          class="main-view__resizer"
          @mousedown="startResize($event, 'ai')"
        />
        <RightPanel
          :active-right-panel="layout.activeRightPanel.value"
          :style="{ width: `${layout.aiPanelWidth.value}px` }"
          @select-panel="layout.setActiveRightPanel"
          @close="layout.toggleRightPanel"
        />
      </template>
    </div>

    <!-- ── 底部状态栏（跨越三面板） ──────────────────────────────────── -->
    <StatusBar />

    <!-- 开发辅助：打开占位标签页按钮（后续移除） -->
    <button class="main-view__dev-btn" @click="openPlaceholderTab">
      {{ t('main.dev.openTabPlaceholder') }}
    </button>

    <!-- 新建文章对话框 -->
    <NewProjectDialog
      :visible="showNewProjectDialog"
      @close="showNewProjectDialog = false"
      @created="handleProjectCreated"
    />
  </div>
</template>

<style scoped>
.main-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  width: 100%;
  overflow: hidden;
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
}

/* ── 主体布局 ─────────────────────────────────────────────────────────── */
.main-view__body {
  flex: 1;
  display: flex;
  overflow: hidden;
  min-height: 0;
}

/* ── 分隔条 ───────────────────────────────────────────────────────────── */
.main-view__resizer {
  width: 4px;
  flex-shrink: 0;
  cursor: col-resize;
  background: transparent;
  transition: background 0.15s ease;
  z-index: 5;
}

.main-view__resizer:hover {
  background: var(--fluen-accent);
}

/* ── 开发辅助按钮 ─────────────────────────────────────────────────────── */
.main-view__dev-btn {
  position: fixed;
  bottom: 32px;
  right: 50%;
  transform: translateX(50%);
  padding: 6px 14px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 9999px;
  background: var(--fluen-surface);
  color: var(--fluen-slate);
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  cursor: pointer;
  z-index: 100;
  opacity: 0.6;
  transition: opacity 0.2s ease;
}

.main-view__dev-btn:hover {
  opacity: 1;
}
</style>
