<script setup lang="ts">
/**
 * KnowledgeBasePanel — 知识库浏览面板（侧边栏）。
 *
 * 功能：
 *   - 展示当前项目知识库的所有条目（概念 / 实体 / 综述）
 *   - 按类型筛选（全部 / 概念 / 实体 / 综述）
 *   - 关键词/语义/混合检索（输入触发，防抖 300ms）
 *   - 点击条目在主内容区打开 WikiReader 标签页
 *   - 知识库未初始化 / 无条目 / 无搜索结果等多种空状态
 *
 * 数据来源：
 *   - `useWikiExplorer` composable（单例）：条目列表与检索
 *   - `useProject` composable（单例）：当前项目路径
 *
 * 与 WikiReader 的协作：
 *   - 点击条目 → `layout.openTab({ type: 'wiki', wikiId })`
 *   - WikiReader 通过 `useWikiExplorer.loadEntry` 加载详情
 *   - 关联条目跳转同样通过 openTab 打开新标签页
 */
import { ref, computed, watch, onMounted, inject } from 'vue';
import { useWikiExplorer } from '../../../../../composables/useWikiExplorer';
import { useProject } from '../../../../../composables/useProject';
import { useI18n } from '../../../../../i18n';
import { MAIN_LAYOUT_KEY } from '../../../composables/useMainLayout';
import { openKnowledgeGraphWindow } from '../../../../graph';
import type { WikiType, QueryMatch } from '../../../../../types/knowledgeBase';

const { t } = useI18n();
const {
  entries,
  loading,
  error,
  notInitialized,
  loadEntries,
  search,
  reset,
} = useWikiExplorer();
const { currentProject } = useProject();

/** 主界面布局（inject 自 MainView，用于打开 WikiReader 标签页）。 */
const layout = inject(MAIN_LAYOUT_KEY, null);

/** 当前项目路径（响应式）。 */
const projectPath = computed(() => currentProject.value?.project_path ?? '');
const hasProject = computed(() => !!projectPath.value);

// ── 搜索与筛选状态 ────────────────────────────────────────────────────

/** 搜索输入值。 */
const searchQuery = ref('');
/** 搜索结果（searchQuery 非空时填充）。 */
const searchResults = ref<QueryMatch[]>([]);
/** 是否正在搜索。 */
const isSearching = ref(false);
/** 当前激活的类型筛选。 */
const activeFilter = ref<'all' | WikiType>('all');

/** 类型筛选选项。 */
const filters = [
  { id: 'all' as const, label: t('main.sidebar.knowledge.filterAll') },
  { id: 'concept' as const, label: t('main.sidebar.knowledge.filterConcept') },
  { id: 'entity' as const, label: t('main.sidebar.knowledge.filterEntity') },
  { id: 'summary' as const, label: t('main.sidebar.knowledge.filterSummary') },
];

/** 搜索图标 SVG path（放大镜）。 */
const ICON_SEARCH = 'M21 21l-4.35-4.35M11 19a8 8 0 1 1 0-16 8 8 0 0 1 0 16Z';
/** Wiki 条目标签页图标 SVG path（书本）。 */
const ICON_WIKI = 'M4 19.5A2.5 2.5 0 0 1 6.5 17H20 M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z';

// ── 统一展示项 ────────────────────────────────────────────────────────

/** 统一的列表展示项（兼容 entries 与 search results）。 */
interface DisplayEntry {
  id: string;
  wiki_type: WikiType;
  title: string;
  tags_count: number;
  updated?: string;
  score?: number;
}

/** 当前展示的条目列表（根据搜索状态切换数据源）。 */
const displayEntries = computed<DisplayEntry[]>(() => {
  // 搜索模式
  if (searchQuery.value.trim()) {
    return searchResults.value
      .filter((m) => activeFilter.value === 'all' || m.wiki_type === activeFilter.value)
      .map((m) => ({
        id: m.wiki_id,
        wiki_type: m.wiki_type,
        title: m.title,
        tags_count: 0,
        score: m.score,
      }));
  }
  // 列表模式
  return entries.value
    .filter((e) => activeFilter.value === 'all' || e.wiki_type === activeFilter.value)
    .map((e) => ({
      id: e.id,
      wiki_type: e.wiki_type,
      title: e.title,
      tags_count: e.tags?.length ?? 0,
      updated: e.updated,
    }));
});

/** 类型徽标标签。 */
function typeLabel(type: WikiType): string {
  return t(`main.sidebar.knowledge.typeLabel.${type}`);
}

/** 格式化更新时间为简短日期。 */
function formatDate(iso?: string): string {
  if (!iso) return '';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

// ── 搜索（防抖）──────────────────────────────────────────────────────

let searchTimer: ReturnType<typeof setTimeout> | null = null;

/** 触发搜索（防抖 300ms）。 */
function scheduleSearch(): void {
  if (searchTimer) clearTimeout(searchTimer);
  searchTimer = setTimeout(runSearch, 300);
}

/** 执行搜索。 */
async function runSearch(): Promise<void> {
  const q = searchQuery.value.trim();
  if (!q) {
    searchResults.value = [];
    isSearching.value = false;
    return;
  }
  if (!projectPath.value) return;
  isSearching.value = true;
  try {
    const result = await search(projectPath.value, q, {
      wikiType: activeFilter.value === 'all' ? undefined : activeFilter.value,
      topK: 30,
    });
    if (result.success) {
      searchResults.value = result.results;
    } else {
      searchResults.value = [];
    }
  } catch {
    searchResults.value = [];
  } finally {
    isSearching.value = false;
  }
}

// 搜索输入变化时防抖触发
watch(searchQuery, () => scheduleSearch());

// 类型筛选变化时：若正在搜索则重新搜索
watch(activeFilter, () => {
  if (searchQuery.value.trim()) {
    runSearch();
  }
});

/** 清除搜索。 */
function clearSearch(): void {
  searchQuery.value = '';
  searchResults.value = [];
}

// ── 打开条目 ──────────────────────────────────────────────────────────

/** 在主内容区打开 WikiReader 标签页。 */
function openEntry(entry: DisplayEntry): void {
  layout?.openTab({
    id: `wiki-${entry.id}`,
    title: entry.title,
    type: 'wiki',
    wikiId: entry.id,
    icon: ICON_WIKI,
  });
}

// ── 打开网状图子窗口 ──────────────────────────────────────────────────

/**
 * 在独立子窗口中打开知识库网状图。
 *
 * 无项目或无条目时不响应（按钮已 disabled，此处二次保护）。
 */
async function openGraph(): Promise<void> {
  if (!projectPath.value || entries.value.length === 0) return;
  await openKnowledgeGraphWindow(projectPath.value);
}

/** 网状图按钮是否可用。 */
const canOpenGraph = computed(
  () => hasProject.value && entries.value.length > 0,
);

/** 网状图图标 SVG path（三节点 + 连线）。 */
const ICON_GRAPH = 'M6 6a2.5 2.5 0 1 0 .01 0M18 6a2.5 2.5 0 1 0 .01 0M12 18a2.5 2.5 0 1 0 .01 0M8.2 7.4 14 16M15.8 7.4 10 16M8 6h8';

// ── 生命周期与项目监听 ────────────────────────────────────────────────

/** 加载条目列表（含元信息）。 */
async function reload(): Promise<void> {
  if (!projectPath.value) {
    reset();
    return;
  }
  await loadEntries(projectPath.value);
}

onMounted(reload);

// 项目切换时重新加载
watch(projectPath, () => {
  clearSearch();
  void reload();
});
</script>

<template>
  <div class="kb-panel">
    <!-- ── 标题栏 ────────────────────────────────────────────────────── -->
    <div class="kb-panel__header">
      <div class="kb-panel__title-group">
        <span class="kb-panel__title">{{ t('main.sidebar.knowledge.title') }}</span>
        <span v-if="entries.length" class="kb-panel__count">{{ entries.length }}</span>
      </div>
      <button
        class="kb-panel__graph-btn"
        :disabled="!canOpenGraph"
        :title="t('main.sidebar.knowledge.openGraph')"
        @click="openGraph"
      >
        <svg
          viewBox="0 0 24 24"
          width="14"
          height="14"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path :d="ICON_GRAPH" />
        </svg>
      </button>
    </div>

    <!-- ── 搜索框 ────────────────────────────────────────────────────── -->
    <div class="kb-panel__search">
      <svg
        class="kb-panel__search-icon"
        viewBox="0 0 24 24"
        width="14"
        height="14"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path :d="ICON_SEARCH" />
      </svg>
      <input
        v-model="searchQuery"
        class="kb-panel__search-input"
        :placeholder="t('main.sidebar.knowledge.searchPlaceholder')"
        type="text"
        spellcheck="false"
      />
      <button
        v-if="searchQuery"
        class="kb-panel__search-clear"
        :title="t('main.sidebar.knowledge.clearSearch')"
        @click="clearSearch"
      >
        <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round">
          <path d="M18 6 6 18M6 6l12 12" />
        </svg>
      </button>
    </div>

    <!-- ── 类型筛选 ──────────────────────────────────────────────────── -->
    <div class="kb-panel__filters">
      <button
        v-for="f in filters"
        :key="f.id"
        class="kb-panel__filter"
        :class="{ 'kb-panel__filter--active': activeFilter === f.id }"
        @click="activeFilter = f.id"
      >
        {{ f.label }}
      </button>
    </div>

    <!-- ── 列表区 ────────────────────────────────────────────────────── -->
    <div class="kb-panel__body">
      <!-- 加载中 -->
      <div v-if="loading || isSearching" class="kb-panel__status">
        <div class="kb-panel__spinner"></div>
        <span>{{ t('main.sidebar.knowledge.loading') }}</span>
      </div>

      <!-- 无项目 -->
      <div v-else-if="!hasProject" class="kb-panel__empty">
        <p class="kb-panel__empty-text">{{ t('main.sidebar.knowledge.noProject') }}</p>
      </div>

      <!-- 未初始化 -->
      <div v-else-if="notInitialized" class="kb-panel__empty">
        <p class="kb-panel__empty-text">{{ t('main.sidebar.knowledge.notInitialized') }}</p>
      </div>

      <!-- 错误 -->
      <div v-else-if="error" class="kb-panel__empty">
        <p class="kb-panel__empty-text kb-panel__empty-text--error">{{ error }}</p>
      </div>

      <!-- 无搜索结果 -->
      <div v-else-if="searchQuery && displayEntries.length === 0" class="kb-panel__empty">
        <p class="kb-panel__empty-text">{{ t('main.sidebar.knowledge.noResults') }}</p>
      </div>

      <!-- 无条目 -->
      <div v-else-if="displayEntries.length === 0" class="kb-panel__empty">
        <p class="kb-panel__empty-text">{{ t('main.sidebar.knowledge.empty') }}</p>
      </div>

      <!-- 条目列表 -->
      <ul v-else class="kb-panel__list">
        <li
          v-for="entry in displayEntries"
          :key="entry.id"
          class="kb-entry"
          :title="entry.title"
          @click="openEntry(entry)"
        >
          <div class="kb-entry__title">{{ entry.title }}</div>
          <div class="kb-entry__meta">
            <span
              class="kb-entry__type"
              :class="'kb-entry__type--' + entry.wiki_type"
            >{{ typeLabel(entry.wiki_type) }}</span>
            <span v-if="entry.tags_count" class="kb-entry__tags">{{ entry.tags_count }}</span>
            <span v-if="entry.updated" class="kb-entry__date">{{ formatDate(entry.updated) }}</span>
          </div>
        </li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.kb-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
  background: var(--fluen-surface);
}

/* ── 标题栏 ─────────────────────────────────────────────────────────── */
.kb-panel__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px 6px;
  flex-shrink: 0;
}

.kb-panel__title-group {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.kb-panel__title {
  font-family: var(--fluen-font-sans);
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--fluen-stone);
}

.kb-panel__count {
  font-family: var(--fluen-font-sans);
  font-size: 11px;
  color: var(--fluen-steel);
  background: var(--fluen-hover);
  border-radius: 9999px;
  padding: 1px 7px;
  min-width: 18px;
  text-align: center;
}

/* ── 网状图按钮 ─────────────────────────────────────────────────────── */
.kb-panel__graph-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
  border-radius: 4px;
  flex-shrink: 0;
  transition: background 0.14s ease, color 0.14s ease;
}

.kb-panel__graph-btn:hover:not(:disabled) {
  background: var(--fluen-hover);
  color: var(--fluen-accent);
}

.kb-panel__graph-btn:active:not(:disabled) {
  color: var(--fluen-accent-pressed);
}

.kb-panel__graph-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

/* ── 搜索框 ─────────────────────────────────────────────────────────── */
.kb-panel__search {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0 8px 8px;
  padding: 0 8px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 6px;
  background: var(--fluen-canvas);
  flex-shrink: 0;
  transition: border-color 0.14s ease;
}

.kb-panel__search:focus-within {
  border-color: var(--fluen-accent);
}

.kb-panel__search-icon {
  flex-shrink: 0;
  color: var(--fluen-stone);
}

.kb-panel__search-input {
  flex: 1;
  min-width: 0;
  padding: 6px 0;
  border: none;
  background: transparent;
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 12.5px;
  outline: none;
}

.kb-panel__search-input::placeholder {
  color: var(--fluen-steel);
}

.kb-panel__search-clear {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
  border-radius: 4px;
  flex-shrink: 0;
}

.kb-panel__search-clear:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

/* ── 类型筛选 ───────────────────────────────────────────────────────── */
.kb-panel__filters {
  display: flex;
  gap: 2px;
  margin: 0 8px 6px;
  padding: 2px;
  background: var(--fluen-hover);
  border-radius: 6px;
  flex-shrink: 0;
}

.kb-panel__filter {
  flex: 1;
  padding: 4px 0;
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  font-family: var(--fluen-font-sans);
  font-size: 11.5px;
  cursor: pointer;
  border-radius: 4px;
  transition: background 0.14s ease, color 0.14s ease;
}

.kb-panel__filter:hover {
  color: var(--fluen-ink);
}

.kb-panel__filter--active {
  background: var(--fluen-surface);
  color: var(--fluen-ink);
  font-weight: 500;
}

/* ── 列表区 ─────────────────────────────────────────────────────────── */
.kb-panel__body {
  flex: 1;
  overflow-y: auto;
  padding: 2px 6px 14px;
}

.kb-panel__list {
  list-style: none;
  margin: 0;
  padding: 0;
}

/* ── 条目卡片 ───────────────────────────────────────────────────────── */
.kb-entry {
  padding: 8px 10px;
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.14s ease;
}

.kb-entry:hover {
  background: var(--fluen-hover);
}

.kb-entry__title {
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  color: var(--fluen-ink);
  line-height: 1.4;
  overflow: hidden;
  text-overflow: ellipsis;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}

.kb-entry__meta {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 4px;
}

.kb-entry__type {
  font-family: var(--fluen-font-sans);
  font-size: 10px;
  font-weight: 500;
  padding: 1px 6px;
  border-radius: 9999px;
  flex-shrink: 0;
}

.kb-entry__type--concept {
  background: var(--fluen-info-bg);
  color: var(--fluen-info-text);
}

.kb-entry__type--entity {
  background: var(--fluen-warning-bg);
  color: var(--fluen-warning-text);
}

.kb-entry__type--summary {
  background: var(--fluen-success-bg);
  color: var(--fluen-success-text);
}

.kb-entry__tags {
  font-family: var(--fluen-font-sans);
  font-size: 10.5px;
  color: var(--fluen-steel);
  background: var(--fluen-hover);
  border-radius: 4px;
  padding: 1px 5px;
  flex-shrink: 0;
}

.kb-entry__date {
  font-family: var(--fluen-font-sans);
  font-size: 10.5px;
  color: var(--fluen-steel);
  margin-left: auto;
  flex-shrink: 0;
}

/* ── 状态与空状态 ───────────────────────────────────────────────────── */
.kb-panel__status {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 40px 0;
  color: var(--fluen-stone);
  font-size: 12.5px;
}

.kb-panel__spinner {
  width: 24px;
  height: 24px;
  border: 2px solid var(--fluen-hairline);
  border-top-color: var(--fluen-accent);
  border-radius: 50%;
  animation: kb-spin 0.8s linear infinite;
}

@keyframes kb-spin {
  to { transform: rotate(360deg); }
}

.kb-panel__empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 40px 16px;
  text-align: center;
}

.kb-panel__empty-text {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 12.5px;
  color: var(--fluen-stone);
  line-height: 1.5;
}

.kb-panel__empty-text--error {
  color: #b91c1c;
}
</style>
