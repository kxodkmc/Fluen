<script setup lang="ts">
/**
 * KnowledgeGraphView — 知识库网状图子窗口顶层视图。
 *
 * 作为独立 Tauri 子窗口挂载（由 App.vue 检测 URL `?view=graph` 参数路由进入）。
 *
 * 布局：
 *   ┌──────────────────────────────────────────────────────┐
 *   │  ← 标题                              ─ □ ✕           │  顶部标题栏
 *  ├──────────┬───────────────────────────────────────────┤
 *  │          │                                           │
 *  │  侧边栏  │              网状图                       │
 *  │  (选中   │         (ForceGraph)                      │
 *  │   详情)  │                                           │
 *  │          │                                           │
 *  │  筛选    │   缩放控件  ─  ＋  ⌂                       │
 *  ├──────────┴───────────────────────────────────────────┤
 *  │  状态栏：节点数 / 边数 / 加载状态                     │
 *  └──────────────────────────────────────────────────────┘
 *
 * 数据：
 *   - 通过 URL query `project` 获取项目路径
 *   - 调用 `knowledge_list_entries` 加载条目（含 relations）
 *   - 用 `buildGraph` 构建图数据，喂给 `useForceSimulation`
 */
import { ref, computed, onMounted, onBeforeUnmount, watch, nextTick } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from '../../i18n';
import { useAppConfig } from '../../composables/useAppConfig';
import ForceGraph from './components/ForceGraph.vue';
import { useForceSimulation } from './composables/useForceSimulation';
import { buildGraph, filterGraphByType } from './graphBuilder';
import type { WikiEntry, WikiType } from '../../types/knowledgeBase';

const { t, setLocale } = useI18n();
const { loadConfig } = useAppConfig();

// ── 项目路径（来自 URL query） ─────────────────────────────────────

/** 从当前窗口 URL 解析 project 路径参数。 */
function readProjectPath(): string {
  try {
    const params = new URLSearchParams(window.location.search);
    return params.get('project') ?? '';
  } catch {
    return '';
  }
}

const projectPath = readProjectPath();

// ── 数据加载 ────────────────────────────────────────────────────────

const entries = ref<WikiEntry[]>([]);
const loading = ref(false);
const loadError = ref('');
const notInitialized = ref(false);

/** 加载知识库条目列表。 */
async function loadEntries(): Promise<void> {
  if (!projectPath) {
    loadError.value = t('graph.errors.noProject');
    return;
  }
  loading.value = true;
  loadError.value = '';
  notInitialized.value = false;
  try {
    const list = await invoke<WikiEntry[]>('knowledge_list_entries', {
      projectPath,
    });
    entries.value = list;
  } catch (err) {
    const msg = typeof err === 'string' ? err : String(err);
    if (
      msg.includes('未初始化') ||
      msg.includes('not initialized') ||
      msg.includes('index.db')
    ) {
      notInitialized.value = true;
    } else {
      loadError.value = msg;
      console.error('[KnowledgeGraphView] 加载知识库条目失败:', err);
    }
  } finally {
    loading.value = false;
  }
}

// ── 图构建与仿真 ────────────────────────────────────────────────────

const sim = useForceSimulation();
const graphRef = ref<InstanceType<typeof ForceGraph> | null>(null);

/** 当前类型筛选（null = 全部）。 */
const activeFilter = ref<WikiType | null>(null);
/** 当前选中节点 ID。 */
const selectedId = ref<string | null>(null);
/** 当前悬停节点 ID。 */
const hoveredId = ref<string | null>(null);

/** 全图节点与边（未筛选）。 */
const fullGraph = computed(() => buildGraph(entries.value));

/** 筛选后的节点与边。 */
const filteredGraph = computed(() =>
  filterGraphByType(fullGraph.value.nodes, fullGraph.value.edges, activeFilter.value),
);

/** 当筛选变化或数据加载后，重新初始化仿真。 */
watch(
  filteredGraph,
  async (graph) => {
    sim.init(graph.nodes, graph.edges, {
      centerX: 0,
      centerY: 0,
    });
    sim.start();
    // 等待视图尺寸就绪后居中
    await nextTick();
    graphRef.value?.resetView();
    // 筛选变化后清除选中
    selectedId.value = null;
  },
  { immediate: false },
);

/** 选中节点详情（来自 entries）。 */
const selectedEntry = computed<WikiEntry | null>(() => {
  if (!selectedId.value) return null;
  return entries.value.find((e) => e.id === selectedId.value) ?? null;
});

/** 选中节点的关联条目（标题）。 */
const selectedRelations = computed<{ id: string; title: string }[]>(() => {
  if (!selectedEntry.value) return [];
  const rels = selectedEntry.value.relations ?? [];
  return rels
    .map((id) => {
      const e = entries.value.find((x) => x.id === id);
      return e ? { id, title: e.title } : null;
    })
    .filter((x): x is { id: string; title: string } => x !== null);
});

// ── 类型筛选选项 ────────────────────────────────────────────────────

const filters: { id: WikiType | null; label: string }[] = [
  { id: null, label: t('graph.filters.all') },
  { id: 'concept', label: t('graph.filters.concept') },
  { id: 'entity', label: t('graph.filters.entity') },
  { id: 'summary', label: t('graph.filters.summary') },
];

// ── 事件处理 ────────────────────────────────────────────────────────

function onSelectNode(id: string): void {
  selectedId.value = id;
}

function onHoverNode(id: string | null): void {
  hoveredId.value = id;
}

function onDragNode(id: string, x: number, y: number): void {
  sim.fixNode(id, x, y);
}

function onDragEnd(id: string): void {
  sim.releaseNode(id);
}

/** 点击侧边栏关联条目：选中并在图中居中。 */
async function focusRelation(id: string): Promise<void> {
  selectedId.value = id;
  await nextTick();
  graphRef.value?.centerOnNode(id);
}

// ── 窗口控制（frameless 标题栏） ────────────────────────────────────

const appWindow = getCurrentWindow();
const isMaximized = ref(false);
let unlistenResize: (() => void) | null = null;

function handleMinimize(): void {
  appWindow.minimize();
}

function handleToggleMaximize(): void {
  appWindow.toggleMaximize();
}

function handleClose(): void {
  appWindow.close();
}

// ── 生命周期 ────────────────────────────────────────────────────────

onMounted(async () => {
  // 同步持久化的语言配置到 i18n 实例（新窗口需要重新加载）
  try {
    const { language } = await loadConfig();
    setLocale(language, { persist: false });
  } catch {
    // 语言加载失败不阻塞
  }

  // 监听窗口最大化状态（标题栏圆角切换）
  isMaximized.value = await appWindow.isMaximized();
  unlistenResize = await appWindow.onResized(async () => {
    isMaximized.value = await appWindow.isMaximized();
  });

  // 加载数据
  await loadEntries();
  // 首次加载后启动仿真（watch immediate=false，需手动触发一次）
  if (entries.value.length > 0) {
    sim.init(filteredGraph.value.nodes, filteredGraph.value.edges, {
      centerX: 0,
      centerY: 0,
    });
    sim.start();
    await nextTick();
    graphRef.value?.resetView();
  }
});

onBeforeUnmount(() => {
  unlistenResize?.();
  sim.destroy();
});

// ── 统计信息 ────────────────────────────────────────────────────────

const stats = computed(() => ({
  nodes: filteredGraph.value.nodes.length,
  edges: filteredGraph.value.edges.length,
  total: entries.value.length,
}));

// ── 类型标签 ────────────────────────────────────────────────────────

function typeLabel(type: WikiType): string {
  return t(`main.sidebar.knowledge.typeLabel.${type}`);
}
</script>

<template>
  <div class="kgv" :class="{ 'kgv--maximized': isMaximized }">
    <!-- ── 标题栏 ────────────────────────────────────────────────── -->
    <header class="kgv__titlebar" data-tauri-drag-region>
      <div class="kgv__title-left">
        <svg
          class="kgv__title-icon"
          viewBox="0 0 24 24"
          width="16"
          height="16"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <circle cx="6" cy="6" r="2.5" />
          <circle cx="18" cy="6" r="2.5" />
          <circle cx="12" cy="18" r="2.5" />
          <path d="M8.2 7.5 14 16M15.8 7.5 10 16M8 6h8" />
        </svg>
        <span class="kgv__title-text">{{ t('graph.title') }}</span>
      </div>
      <div class="kgv__title-right">
        <button
          class="kgv__win-btn"
          :title="t('graph.window.minimize')"
          @click="handleMinimize"
        >
          <svg viewBox="0 0 12 12" width="12" height="12"><rect x="2" y="5.5" width="8" height="1" fill="currentColor" /></svg>
        </button>
        <button
          class="kgv__win-btn"
          :title="t('graph.window.maximize')"
          @click="handleToggleMaximize"
        >
          <svg viewBox="0 0 12 12" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1"><rect x="2.5" y="2.5" width="7" height="7" /></svg>
        </button>
        <button
          class="kgv__win-btn kgv__win-btn--close"
          :title="t('graph.window.close')"
          @click="handleClose"
        >
          <svg viewBox="0 0 12 12" width="12" height="12" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"><path d="M3 3l6 6M9 3l-6 6" /></svg>
        </button>
      </div>
    </header>

    <!-- ── 主体 ──────────────────────────────────────────────────── -->
    <div class="kgv__body">
      <!-- 侧边栏：筛选 + 选中详情 -->
      <aside class="kgv__sidebar">
        <!-- 类型筛选 -->
        <section class="kgv__section">
          <h3 class="kgv__section-title">{{ t('graph.filters.title') }}</h3>
          <div class="kgv__filter-group">
            <button
              v-for="f in filters"
              :key="String(f.id)"
              class="kgv__filter"
              :class="{ 'kgv__filter--active': activeFilter === f.id }"
              @click="activeFilter = f.id"
            >
              {{ f.label }}
            </button>
          </div>
        </section>

        <!-- 选中节点详情 -->
        <section class="kgv__section kgv__section--detail">
          <h3 class="kgv__section-title">{{ t('graph.detail.title') }}</h3>
          <div v-if="selectedEntry" class="kgv__detail">
            <div class="kgv__detail-header">
              <span
                class="kgv__detail-type"
                :class="'kgv__detail-type--' + selectedEntry.wiki_type"
              >{{ typeLabel(selectedEntry.wiki_type) }}</span>
              <span class="kgv__detail-degree">
                {{ t('graph.detail.degree', { n: selectedEntry.relations?.length ?? 0 }) }}
              </span>
            </div>
            <h4 class="kgv__detail-title" :title="selectedEntry.title">{{ selectedEntry.title }}</h4>

            <div v-if="selectedRelations.length" class="kgv__detail-relations">
              <span class="kgv__detail-relations-label">
                {{ t('graph.detail.relations') }}
              </span>
              <ul class="kgv__relation-list">
                <li
                  v-for="rel in selectedRelations"
                  :key="rel.id"
                  class="kgv__relation-item"
                  :title="rel.title"
                  @click="focusRelation(rel.id)"
                >
                  {{ rel.title }}
                </li>
              </ul>
            </div>
            <p v-else class="kgv__detail-empty">{{ t('graph.detail.noRelations') }}</p>
          </div>
          <p v-else class="kgv__detail-placeholder">{{ t('graph.detail.placeholder') }}</p>
        </section>
      </aside>

      <!-- 网状图主区域 -->
      <main class="kgv__main">
        <!-- 加载中 -->
        <div v-if="loading" class="kgv__status">
          <div class="kgv__spinner" />
          <span>{{ t('graph.loading') }}</span>
        </div>

        <!-- 未初始化 -->
        <div v-else-if="notInitialized" class="kgv__status">
          <span class="kgv__status-icon">⚠</span>
          <span>{{ t('graph.errors.notInitialized') }}</span>
        </div>

        <!-- 加载错误 -->
        <div v-else-if="loadError" class="kgv__status kgv__status--error">
          <span class="kgv__status-icon">⚠</span>
          <span>{{ loadError }}</span>
        </div>

        <!-- 无项目 -->
        <div v-else-if="!projectPath" class="kgv__status">
          <span>{{ t('graph.errors.noProject') }}</span>
        </div>

        <!-- 空知识库 -->
        <div v-else-if="entries.length === 0" class="kgv__status">
          <span>{{ t('graph.empty') }}</span>
        </div>

        <!-- 正常渲染 -->
        <template v-else>
          <ForceGraph
            ref="graphRef"
            :nodes="sim.nodes.value"
            :edges="sim.edges.value"
            :selected-id="selectedId"
            :hovered-id="hoveredId"
            @select-node="onSelectNode"
            @hover-node="onHoverNode"
            @drag-node="onDragNode"
            @drag-end="onDragEnd"
          />

          <!-- 缩放控件 -->
          <div class="kgv__zoom-controls">
            <button class="kgv__zoom-btn" :title="t('graph.zoom.in')" @click="graphRef?.zoomIn()">
              <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 5v14M5 12h14" /></svg>
            </button>
            <button class="kgv__zoom-btn" :title="t('graph.zoom.out')" @click="graphRef?.zoomOut()">
              <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M5 12h14" /></svg>
            </button>
            <button class="kgv__zoom-btn" :title="t('graph.zoom.reset')" @click="graphRef?.resetView()">
              <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5" /></svg>
            </button>
          </div>

          <!-- 图例 -->
          <div class="kgv__legend">
            <div class="kgv__legend-item">
              <span class="kgv__legend-dot kgv__legend-dot--concept" />
              <span>{{ t('graph.filters.concept') }}</span>
            </div>
            <div class="kgv__legend-item">
              <span class="kgv__legend-dot kgv__legend-dot--entity" />
              <span>{{ t('graph.filters.entity') }}</span>
            </div>
            <div class="kgv__legend-item">
              <span class="kgv__legend-dot kgv__legend-dot--summary" />
              <span>{{ t('graph.filters.summary') }}</span>
            </div>
          </div>
        </template>
      </main>
    </div>

    <!-- ── 状态栏 ────────────────────────────────────────────────── -->
    <footer class="kgv__statusbar">
      <span>{{ t('graph.status.nodes', { n: stats.nodes }) }}</span>
      <span class="kgv__statusbar-sep">·</span>
      <span>{{ t('graph.status.edges', { n: stats.edges }) }}</span>
      <span v-if="activeFilter" class="kgv__statusbar-sep">·</span>
      <span v-if="activeFilter">{{ t('graph.status.filtered', { n: stats.total }) }}</span>
    </footer>
  </div>
</template>

<style scoped>
.kgv {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--fluen-canvas);
  border-radius: 12px;
  overflow: hidden;
}

.kgv--maximized {
  border-radius: 0;
}

/* ── 标题栏 ─────────────────────────────────────────────────────── */
.kgv__titlebar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 38px;
  padding: 0 6px 0 12px;
  background: var(--fluen-surface);
  border-bottom: 1px solid var(--fluen-hairline);
  flex-shrink: 0;
  user-select: none;
}

.kgv__title-left {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--fluen-stone);
}

.kgv__title-icon {
  color: var(--fluen-accent);
}

.kgv__title-text {
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.04em;
  color: var(--fluen-ink);
}

.kgv__title-right {
  display: flex;
  gap: 2px;
}

.kgv__win-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
  border-radius: 4px;
  transition: background 0.12s ease, color 0.12s ease;
}

.kgv__win-btn:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.kgv__win-btn--close:hover {
  background: #e81123;
  color: #fff;
}

/* ── 主体布局 ───────────────────────────────────────────────────── */
.kgv__body {
  flex: 1;
  display: flex;
  min-height: 0;
}

/* ── 侧边栏 ─────────────────────────────────────────────────────── */
.kgv__sidebar {
  width: 240px;
  flex-shrink: 0;
  border-right: 1px solid var(--fluen-hairline);
  background: var(--fluen-surface);
  overflow-y: auto;
  display: flex;
  flex-direction: column;
}

.kgv__section {
  padding: 14px 12px;
  border-bottom: 1px solid var(--fluen-hairline);
}

.kgv__section--detail {
  flex: 1;
  border-bottom: none;
}

.kgv__section-title {
  margin: 0 0 10px;
  font-family: var(--fluen-font-sans);
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--fluen-stone);
}

.kgv__filter-group {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.kgv__filter {
  padding: 4px 10px;
  border: 1px solid var(--fluen-hairline);
  background: var(--fluen-canvas);
  color: var(--fluen-stone);
  font-family: var(--fluen-font-sans);
  font-size: 11.5px;
  cursor: pointer;
  border-radius: 9999px;
  transition: all 0.12s ease;
}

.kgv__filter:hover {
  color: var(--fluen-ink);
  border-color: var(--fluen-accent);
}

.kgv__filter--active {
  background: var(--fluen-accent);
  color: #fff;
  border-color: var(--fluen-accent);
}

/* ── 选中详情 ───────────────────────────────────────────────────── */
.kgv__detail {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.kgv__detail-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.kgv__detail-type {
  font-family: var(--fluen-font-sans);
  font-size: 10px;
  font-weight: 500;
  padding: 2px 8px;
  border-radius: 9999px;
}

.kgv__detail-type--concept {
  background: var(--fluen-info-bg);
  color: var(--fluen-info);
}

.kgv__detail-type--entity {
  background: var(--fluen-warning-bg);
  color: var(--fluen-warning);
}

.kgv__detail-type--summary {
  background: var(--fluen-success-bg);
  color: var(--fluen-success-text);
}

.kgv__detail-degree {
  font-family: var(--fluen-font-sans);
  font-size: 10.5px;
  color: var(--fluen-steel);
}

.kgv__detail-title {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 14px;
  font-weight: 600;
  color: var(--fluen-ink);
  line-height: 1.4;
  word-break: break-word;
}

.kgv__detail-relations {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.kgv__detail-relations-label {
  font-family: var(--fluen-font-sans);
  font-size: 10.5px;
  color: var(--fluen-stone);
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

.kgv__relation-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.kgv__relation-item {
  padding: 6px 8px;
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  color: var(--fluen-ink);
  background: var(--fluen-canvas);
  border-radius: 4px;
  cursor: pointer;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  transition: background 0.12s ease, color 0.12s ease;
}

.kgv__relation-item:hover {
  background: var(--fluen-accent);
  color: #fff;
}

.kgv__detail-empty,
.kgv__detail-placeholder {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  color: var(--fluen-stone);
  line-height: 1.5;
}

.kgv__detail-placeholder {
  font-style: italic;
}

/* ── 网状图主区域 ───────────────────────────────────────────────── */
.kgv__main {
  flex: 1;
  position: relative;
  min-width: 0;
}

/* ── 加载与空状态 ───────────────────────────────────────────────── */
.kgv__status {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--fluen-stone);
  font-family: var(--fluen-font-sans);
  font-size: 13px;
}

.kgv__status--error {
  color: var(--fluen-error);
}

.kgv__status-icon {
  font-size: 24px;
}

.kgv__spinner {
  width: 28px;
  height: 28px;
  border: 2px solid var(--fluen-hairline);
  border-top-color: var(--fluen-accent);
  border-radius: 50%;
  animation: kgv-spin 0.8s linear infinite;
}

@keyframes kgv-spin {
  to { transform: rotate(360deg); }
}

/* ── 缩放控件 ───────────────────────────────────────────────────── */
.kgv__zoom-controls {
  position: absolute;
  right: 14px;
  bottom: 14px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 4px;
  background: var(--fluen-surface);
  border: 1px solid var(--fluen-hairline);
  border-radius: 6px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08);
}

.kgv__zoom-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
  border-radius: 4px;
  transition: background 0.12s ease, color 0.12s ease;
}

.kgv__zoom-btn:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

/* ── 图例 ───────────────────────────────────────────────────────── */
.kgv__legend {
  position: absolute;
  left: 14px;
  bottom: 14px;
  display: flex;
  gap: 12px;
  padding: 6px 10px;
  background: var(--fluen-surface);
  border: 1px solid var(--fluen-hairline);
  border-radius: 6px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08);
}

.kgv__legend-item {
  display: flex;
  align-items: center;
  gap: 5px;
  font-family: var(--fluen-font-sans);
  font-size: 11px;
  color: var(--fluen-stone);
}

.kgv__legend-dot {
  width: 9px;
  height: 9px;
  border-radius: 50%;
}

.kgv__legend-dot--concept {
  background: var(--fluen-info);
}

.kgv__legend-dot--entity {
  background: var(--fluen-warning);
}

.kgv__legend-dot--summary {
  background: var(--fluen-success-text);
}

/* ── 状态栏 ─────────────────────────────────────────────────────── */
.kgv__statusbar {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 24px;
  padding: 0 12px;
  background: var(--fluen-surface);
  border-top: 1px solid var(--fluen-hairline);
  font-family: var(--fluen-font-sans);
  font-size: 10.5px;
  color: var(--fluen-steel);
  flex-shrink: 0;
}

.kgv__statusbar-sep {
  color: var(--fluen-hairline);
}
</style>
