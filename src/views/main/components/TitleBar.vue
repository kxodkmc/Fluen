<script setup lang="ts">
/**
 * TitleBar — 顶部标题栏编排容器。
 *
 * 将标题栏拆分为三个并行的独立 Dock（圆角矩形）：
 *
 *   ┌──────────────┐  ┌──────────────────┬───┐  ┌──────────────────┐
 *   │ Primary Dock │  │   Search Dock    │ 🐾│  │  Controls Dock   │
 *   │ 图标·标题·菜单│  │  命令面板触发器  │   │  │  折叠·窗口控制   │
 *   └──────────────┘  └──────────────────┴───┘  └──────────────────┘
 *
 * 容器自身**无背景、无边框**，仅做三栏 Grid 编排；
 * 三个 Dock 各自独立浮动（边框 + 圆角 + 表面背景 + 阴影），互不耦合。
 *
 *   - 左栏：auto（按内容自适应）
 *   - 中栏：1fr（占据剩余空间，搜索栏居中）
 *   - 右栏：auto（按内容自适应）
 *
 * 响应式策略：
 *   - 默认（≥ menuCollapse）：三栏完整展示
 *   - ≤ menuCollapse：Primary 进入紧凑态（标题/菜单文字隐藏）
 *   - ≤ searchCollapse：Search 进入紧凑态（仅图标），Controls 隐藏面板折叠按钮
 *
 * 整栏为 Tauri 拖拽区域；子 Dock 内交互元素自行声明 data-tauri-drag-region="false"。
 *
 * 注：未直接套用预设 `presets/layouts/Dock` —— 该预设为绝对定位浮动的放大镜式 Dock，
 * 且 item 仅支持 icon+label，不适合承载菜单文字与搜索输入等复杂内容。
 * 此处借鉴其独立圆角矩形的视觉语言，专为标题栏场景实现。
 */
import { ref, computed, inject, onMounted, onUnmounted } from 'vue';
import TitleBarPrimary from './titlebar/TitleBarPrimary.vue';
import TitleBarSearch from './titlebar/TitleBarSearch.vue';
import TitleBarControls from './titlebar/TitleBarControls.vue';
import { Mascot } from '../../../components/mascot';
import { useMascotConfig } from '../../../composables/useMascotConfig';
import { useMascotData } from '../../../composables/useMascotData';
import { MOTIS_CHAT_KEY } from './motis/symbols';
import { MAIN_LAYOUT_KEY } from '../composables/useMainLayout';
import type { Mood } from '../../../types/mascot';
import { TITLE_BAR_BREAKPOINTS } from '../constants';

defineEmits<{
  (e: 'toggle-panel', panel: 'function' | 'content' | 'ai'): void;
  (e: 'command-palette'): void;
  (e: 'menu-select', itemId: string): void;
  /** 跳转到设置页面。 */
  (e: 'navigate-settings'): void;
}>();

/* ── 响应式断点监听 ─────────────────────────────────────────────────── */
const viewportWidth = ref(typeof window !== 'undefined' ? window.innerWidth : 1280);

function handleResize(): void {
  viewportWidth.value = window.innerWidth;
}

/* ── 宠物助手 ─────────────────────────────────────────────────────────── */
const { loadConfig: loadMascotConfig } = useMascotConfig();
const { loadData: loadMascotData, updateAffinity } = useMascotData();

/** 宠物是否启用。 */
const mascotEnabled = ref(true);
/** 当前心情。 */
const mascotMood = ref<Mood>('neutral');

/* ── Motis 状态气泡（注入 MainView 提供的共享实例） ─────────────────── */
// inject 默认为 null，兼容 TitleBar 独立使用场景（无 MainView provide 时无气泡）
const motisChat = inject(MOTIS_CHAT_KEY, null);
/** 宠物状态气泡文本（来自 useMotisChat.statusBubble）。 */
const mascotStatusBubble = computed(() => motisChat?.statusBubble.value ?? '');

/* ── 布局状态（注入 MainView 提供的共享实例） ─────────────────────── */
const layout = inject(MAIN_LAYOUT_KEY, null);

async function initMascot(): Promise<void> {
  const config = await loadMascotConfig();
  mascotEnabled.value = config.enabled;
  if (!config.enabled) return;
  const data = await loadMascotData();
  mascotMood.value = data.mood;
}

/**
 * 点击宠物：切换 Motis 面板 + 增加好感度。
 *
 * setActiveRightPanel('motis') 内含切换逻辑：
 *   - 若 Motis 已激活且面板展开 → 收起
 *   - 若未激活或未展开 → 激活并展开
 */
async function handleMascotClick(): Promise<void> {
  // 先切换面板（同步，UI 即时响应）
  layout?.setActiveRightPanel('motis');
  // 再更新好感度（异步，后台执行）
  await updateAffinity(1);
}

onMounted(() => {
  window.addEventListener('resize', handleResize, { passive: true });
  initMascot();
});

onUnmounted(() => {
  window.removeEventListener('resize', handleResize);
});

/* ── 紧凑态判定 ─────────────────────────────────────────────────────── */
const menuCompact = computed(() => viewportWidth.value <= TITLE_BAR_BREAKPOINTS.menuCollapse);
const searchCompact = computed(() => viewportWidth.value <= TITLE_BAR_BREAKPOINTS.searchCollapse);
const controlsCompact = computed(() => viewportWidth.value <= TITLE_BAR_BREAKPOINTS.searchCollapse);
</script>

<template>
  <header class="title-bar" data-tauri-drag-region>
    <!-- 左：图标 + 标题 + 菜单 -->
    <div class="title-bar__cell title-bar__cell--left">
      <TitleBarPrimary :compact="menuCompact" @menu-select="$emit('menu-select', $event)" />
    </div>

    <!-- 中：搜索栏 + 桌面宠物 -->
    <div class="title-bar__cell title-bar__cell--center">
      <TitleBarSearch :compact="searchCompact" @activate="$emit('command-palette')" />
      <Mascot
        v-if="!searchCompact && mascotEnabled"
        :height="28"
        :mood="mascotMood"
        :status-bubble="mascotStatusBubble"
        @click="handleMascotClick"
      />
    </div>

    <!-- 右：折叠 + 窗口控制 -->
    <div class="title-bar__cell title-bar__cell--right">
      <TitleBarControls
        :compact="controlsCompact"
        @toggle-panel="$emit('toggle-panel', $event)"
        @navigate-settings="$emit('navigate-settings')"
      />
    </div>
  </header>
</template>

<style scoped>
.title-bar {
  /* 容器自身无背景、无边框 —— 三个 Dock 各自独立浮动 */
  display: grid;
  grid-template-columns: auto 1fr auto;
  align-items: center;
  gap: 8px;
  height: 44px;
  padding: 0 8px;
  flex-shrink: 0;
  background: transparent;
  border-bottom: none;
  user-select: none;
  -webkit-app-region: drag;
}

/* ── 三栏单元格 ─────────────────────────────────────────────────────── */
.title-bar__cell {
  display: flex;
  align-items: center;
  min-width: 0;
  min-height: 0;
  height: 100%;
}

.title-bar__cell--left {
  justify-content: flex-start;
}

.title-bar__cell--center {
  justify-content: center;
  gap: 6px;
}

.title-bar__cell--center :deep(.dock-search) {
  flex: 1;
  max-width: 480px;
}

.title-bar__cell--right {
  justify-content: flex-end;
}

/* ── 响应式：窄屏 ───────────────────────────────────────────────────── */
@media (max-width: 560px) {
  .title-bar {
    gap: 5px;
    padding: 0 5px;
  }

  .title-bar__cell--center :deep(.dock-search) {
    max-width: none;
  }

  .title-bar__cell--center :deep(.mascot) {
    display: none;
  }
}

@media (max-width: 420px) {
  .title-bar {
    gap: 4px;
    padding: 0 4px;
  }
}
</style>
