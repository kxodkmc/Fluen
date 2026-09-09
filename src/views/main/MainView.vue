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
import { ref, watch, provide, onMounted, onBeforeUnmount } from 'vue';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import TitleBar from './components/TitleBar.vue';
import FunctionPanel from './components/FunctionPanel.vue';
import ContentPanel from './components/ContentPanel.vue';
import { RightPanel } from './components/rightpanel';
import StatusBar from './components/statusbar/StatusBar.vue';
import { useProjectStatus } from './components/statusbar';
import NewProjectDialog from '../../components/NewProjectDialog.vue';
import AboutDialog from '../../components/AboutDialog.vue';
import { useMainLayout, MAIN_LAYOUT_KEY } from './composables/useMainLayout';
import { useMotisChat } from './composables/useMotisChat';
import { useKbAgentChat, registerKbAgentAutoOpen } from './composables/useKbAgentChat';
import { MOTIS_CHAT_KEY } from './components/motis';
import { useProject } from '../../composables/useProject';
import { useFluenEditor } from './components/editor/composables/useFluenEditor';
import { registerCommand, unregisterCommand, bindShortcut, unbindShortcut } from '../../shortcuts';
import { PANEL_CONSTRAINTS } from './constants';
import type { EditorLayoutMode } from './types';

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

/* ── 知识库构建对话流（Kb Agent 面板） ────────────────────────────────── */
// 任何构建启动（右键加入知识库 / 队列遗留任务恢复执行 / 失败重试）都会
// 收到 kb-build:started 事件 → 自动展开右侧 Kb Agent 面板并聚焦该任务。
const kbAgentChat = useKbAgentChat();
registerKbAgentAutoOpen((taskId) => {
  kbAgentChat.focusTask(taskId);
  layout.showRightPanel('kbagent');
});
void kbAgentChat.setupEventListeners();

/* ── 全局快捷键：保存文档 ─────────────────────────────────────────────── */
// 主界面按 Ctrl/Cmd+S 保存当前文档。由全局 capture 监听统一接管：
// 焦点在编辑器内外均生效，并拦截 WebView2 的默认行为（不再被“占用”）。
// 卸载时仅注销命令（绑定保留）——离开主界面（设置页等）后 Ctrl+S 不再
// 触发保存，但全局监听仍会消费该键位以阻止 WebView2 默认行为。
const SAVE_DOCUMENT_COMMAND = 'save-document';

/** 编辑器视图模式 → 快捷键映射（Mod+1/2/3/4）。 */
const EDITOR_LAYOUT_SHORTCUTS: ReadonlyArray<readonly [EditorLayoutMode, string]> = [
  ['source', 'Mod-1'],
  ['live', 'Mod-2'],
  ['preview', 'Mod-3'],
  ['wysiwyg', 'Mod-4'],
];

/** 视图模式命令 id：`editor-layout-{mode}`。 */
function layoutCommandId(mode: EditorLayoutMode): string {
  return `editor-layout-${mode}`;
}

onMounted(() => {
  registerCommand(SAVE_DOCUMENT_COMMAND, () => {
    void useFluenEditor().save();
  });
  bindShortcut('Mod-s', SAVE_DOCUMENT_COMMAND);

  // 编辑器视图切换（Mod+1 仅源码 / Mod+2 半预览 / Mod+3 仅渲染 / Mod+4 预览编辑）
  for (const [mode, key] of EDITOR_LAYOUT_SHORTCUTS) {
    registerCommand(layoutCommandId(mode), () => layout.setEditorLayout(mode));
    bindShortcut(key, layoutCommandId(mode));
  }
});

onBeforeUnmount(() => {
  unregisterCommand(SAVE_DOCUMENT_COMMAND);
  // Mod-s 绑定保留（拦截 WebView2 默认保存行为）；
  // 视图切换的数字键无默认行为需拦截，离开主界面应解绑，避免静默吞键。
  for (const [mode, key] of EDITOR_LAYOUT_SHORTCUTS) {
    unbindShortcut(key, layoutCommandId(mode));
    unregisterCommand(layoutCommandId(mode));
  }
});

/* ── 拖拽调整面板宽度 ─────────────────────────────────────────────────── */
/**
 * 拖动期间在 body 上覆盖一个全屏透明遮罩（`main-view__resize-mask`）：
 *   - 遮罩 `pointer-events: auto` 截获所有鼠标事件，阻止 iframe / contenteditable
 *     抢占 mousemove（iframe 内事件不冒泡到外层 document，会导致拖拽中断）
 *   - mousemove 事件经遮罩冒泡到 document，被 `onResizeMove` 监听器接收
 *
 * 这是经典做法：拖拽期间用一个全屏 overlay 屏蔽底层交互元素的鼠标捕获，
 * 既保证 mousemove 不被 iframe 截获，也避免拖拽过程中误触其他控件。
 */
const isResizing = ref(false);
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
  isResizing.value = true;
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
  isResizing.value = false;
  document.removeEventListener('mousemove', onResizeMove);
  document.removeEventListener('mouseup', stopResize);
}

defineEmits<{
  /** 跳转到设置页面。 */
  (e: 'navigate-settings'): void;
}>();

/* ── 新建文章对话框 ───────────────────────────────────────────────────── */
const showNewProjectDialog = ref(false);

/** “关于”对话框显示状态。 */
const showAboutDialog = ref(false);

/** 处理标题栏菜单选择。 */
async function handleMenuSelect(itemId: string): Promise<void> {
  if (itemId === 'newArticle') {
    showNewProjectDialog.value = true;
  } else if (itemId === 'openArticle') {
    await handleOpenArticle();
  } else if (itemId === 'openLogsDir') {
    await handleOpenLogsDir();
  } else if (itemId === 'about') {
    showAboutDialog.value = true;
  }
}

/** 打开日志存放目录（系统文件管理器）。 */
async function handleOpenLogsDir(): Promise<void> {
  try {
    await invoke('open_logs_dir');
  } catch (err) {
    console.error('[MainView] 打开日志目录失败:', err);
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
// 仅在“项目从无到有打开”时触发（hasProject false→true）。
// 注意：不能监听 config —— editor_save_content 保存成功后会重新加载整个项目，
// 使 config 引用变化。若监听 config，每次保存都会走到这里，
// 而 setActiveActivity('outline') 在大纲已激活时会执行 toggle → 侧边栏被意外折叠。
watch(
  hasProject,
  (opened, wasOpened) => {
    if (opened && !wasOpened && config.value) {
      // 创建或激活项目标签页
      const tabId = `project-${config.value.title}`;
      layout.openTab({
        id: tabId,
        title: config.value.title,
        type: 'editor',
        icon: 'M4 4h12v16H4V4zm2 4h8m-8 4h8m-8 4h5M18 8v12a2 2 0 0 1-2 2',
      });

      // 自动切换到大纲面板。已在大纲时改为“确保展开”，
      // 避免 setActiveActivity 的 toggle 语义（点击已激活项=折叠）把侧边栏收起。
      if (layout.activeActivity.value !== 'outline') {
        layout.setActiveActivity('outline');
      } else if (layout.functionPanelCollapsed.value) {
        layout.setFunctionPanelCollapsed(false);
      }
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
        @new-article="showNewProjectDialog = true"
        @open-article="handleOpenArticle"
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
          @close="layout.toggleRightPanel"
        />
      </template>
    </div>

    <!-- ── 底部状态栏（跨越三面板） ──────────────────────────────────── -->
    <StatusBar />

    <!-- 拖拽遮罩：阻止 iframe / contenteditable 抢占 mousemove -->
    <div v-if="isResizing" class="main-view__resize-mask"></div>

    <!-- 新建文章对话框 -->
    <NewProjectDialog
      :visible="showNewProjectDialog"
      @close="showNewProjectDialog = false"
      @created="handleProjectCreated"
    />

    <!-- “关于”对话框 -->
    <AboutDialog
      :visible="showAboutDialog"
      @close="showAboutDialog = false"
    />
  </div>
</template>

<style scoped>
.main-view {
  position: relative;
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

/* ── 拖拽遮罩 ─────────────────────────────────────────────────────────── */
/* 拖拽期间覆盖整个 main-view，截获 iframe / contenteditable 的 mousemove。
 * 透明背景 + pointer-events:auto → 不影响视觉但拦截所有鼠标事件，
 * 让外层 document 的 mousemove listener 正常触发。
 * cursor: col-resize 提供与分隔条一致的拖动光标体验。 */
.main-view__resize-mask {
  position: absolute;
  inset: 0;
  z-index: 50;
  cursor: col-resize;
  background: transparent;
  user-select: none;
}
</style>
