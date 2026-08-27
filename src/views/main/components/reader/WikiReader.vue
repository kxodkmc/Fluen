<template>
  <div class="wiki-reader">
    <!-- 加载中 -->
    <div v-if="loading" class="wiki-reader__status">
      <div class="wiki-reader__spinner"></div>
      <span>{{ t('main.sidebar.knowledge.reader.loading') }}</span>
    </div>

    <!-- 错误 -->
    <div v-else-if="error" class="wiki-reader__status wiki-reader__status--error">
      <span class="wiki-reader__error-icon">⚠</span>
      <span>{{ error }}</span>
    </div>

    <!-- 未找到 -->
    <div v-else-if="!detail" class="wiki-reader__status">
      <span>{{ t('main.sidebar.knowledge.reader.notFound') }}</span>
    </div>

    <!-- 正常渲染 -->
    <template v-else>
      <!-- ── 顶部工具栏 ─────────────────────────────────────────────── -->
      <div class="wiki-reader__toolbar">
        <span
          class="wiki-reader__type"
          :class="'wiki-reader__type--' + detail.wiki_type"
        >{{ typeLabel(detail.wiki_type) }}</span>
        <h1 class="wiki-reader__title" :title="detail.title">{{ detail.title }}</h1>
        <button
          class="wiki-reader__close"
          :title="t('main.sidebar.knowledge.reader.close')"
          @click="emit('close')"
        >
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            <path d="M18 6 6 18M6 6l12 12" />
          </svg>
        </button>
      </div>

      <!-- 标签行 -->
      <div v-if="detail.tag_titles?.length" class="wiki-reader__tags">
        <span
          v-for="tag in detail.tag_titles"
          :key="tag"
          class="wiki-reader__tag"
        >{{ tag }}</span>
      </div>

      <!-- ── 主体：正文 + 关联侧边栏 ───────────────────────────────── -->
      <div class="wiki-reader__body">
        <!-- 正文（iframe 隔离渲染） -->
        <div class="wiki-reader__content">
          <iframe
            ref="iframeRef"
            class="wiki-reader__iframe"
            sandbox="allow-same-origin"
            :title="detail.title"
          ></iframe>
        </div>

        <!-- 关联条目侧边栏 -->
        <aside
          class="wiki-reader__relations"
          :class="{'wiki-reader__relations--collapsed': relationsCollapsed}"
        >
          <template v-if="!relationsCollapsed">
            <div class="wiki-reader__relations-header">
              <h3 class="wiki-reader__relations-title">
                {{ t('main.sidebar.knowledge.reader.relations') }}
                <span v-if="relationTotal" class="wiki-reader__relations-count">{{ relationTotal }}</span>
              </h3>
              <button
                class="wiki-reader__relations-toggle"
                :title="t('main.sidebar.knowledge.reader.collapseRelations')"
                @click="relationsCollapsed = true"
              >
                <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="m15 18-6-6 6-6" />
                </svg>
              </button>
            </div>
            <ul v-if="detail.relation_titles?.length" class="wiki-reader__relations-list">
              <li
                v-for="(title, idx) in detail.relation_titles"
                :key="detail.relations?.[idx] ?? title"
                class="wiki-reader__relation"
                :title="title"
                @click="openRelation(detail.relations?.[idx] ?? '', title)"
              >
                <i
                  v-if="relationTypeOf(detail.relations?.[idx] ?? '')"
                  class="wiki-reader__relation-dot"
                  :class="'wiki-reader__relation-dot--' + relationTypeOf(detail.relations?.[idx] ?? '')"
                ></i>
                <span class="wiki-reader__relation-text">{{ title }}</span>
                <svg
                  class="wiki-reader__relation-icon"
                  viewBox="0 0 24 24"
                  width="12"
                  height="12"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <path d="M9 18l6-6-6-6" />
                </svg>
              </li>
            </ul>
            <p v-else class="wiki-reader__relations-empty">
              {{ t('main.sidebar.knowledge.reader.noRelations') }}
            </p>
          </template>

          <!-- 折叠态：关联条目类型统计摘要 -->
          <button
            v-else
            class="wiki-reader__relations-collapsed"
            :title="t('main.sidebar.knowledge.reader.expandRelations')"
            @click="relationsCollapsed = false"
          >
            <svg
              class="wiki-reader__collapsed-link"
              viewBox="0 0 24 24"
              width="13"
              height="13"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
              <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
            </svg>
            <span v-if="relationStats.length" class="wiki-reader__collapsed-stats">
              <span
                v-for="stat in relationStats"
                :key="stat.type"
                class="wiki-reader__collapsed-stat"
                :title="typeLabel(stat.type) + ' ×' + stat.count"
              >
                <i
                  class="wiki-reader__stat-dot"
                  :class="'wiki-reader__stat-dot--' + stat.type"
                ></i>
                <span class="wiki-reader__stat-num">{{ stat.count }}</span>
              </span>
            </span>
            <span v-else class="wiki-reader__collapsed-empty">—</span>
          </button>
        </aside>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
/**
 * WikiReader —— 知识库条目阅读器（主内容区）。
 *
 * 由 KnowledgeBasePanel 点击条目后通过 `layout.openTab({ type: 'wiki' })` 打开。
 *
 * 功能：
 *   - 通过 `useWikiExplorer.loadEntry` 加载条目详情（含正文、标签名、关联标题）
 *   - 正文 MD 经后端 `editor_render_html` 渲染为 HTML，iframe 隔离注入
 *   - 顶部工具栏：类型徽标、标题、关闭按钮
 *   - 标签行：展示 tag_titles
 *   - 右侧关联条目侧边栏：点击跳转（打开新 wiki tab 或激活已存在 tab）
 *
 * 关联跳转：
 *   - `WikiEntryDetail.relations`（wikiID 列表）与 `relation_titles`（标题列表）一一对应
 *   - 点击关联条目 → `layout.openTab({ type: 'wiki', wikiId, title })`
 *   - tab id 为 `wiki-${wikiId}`，重复打开会激活已有 tab
 *
 * 渲染策略：
 *   - 复用后端 `editor_render_html` 命令（与编辑器预览一致，支持 fluen-markup、KaTeX、hljs）
 *   - iframe 提供 `<style>` 隔离，避免污染应用布局
 */
import { ref, watch, onMounted, onBeforeUnmount, inject, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useWikiExplorer } from '../../../../composables/useWikiExplorer';
import { useProject } from '../../../../composables/useProject';
import { useI18n } from '../../../../i18n';
import { MAIN_LAYOUT_KEY } from '../../composables/useMainLayout';
import type { WikiEntryDetail, WikiType } from '../../../../types/knowledgeBase';

const props = defineProps<{
  /** 知识库条目 ID。 */
  wikiId: string;
}>();

const emit = defineEmits<{
  close: [];
}>();

const { t } = useI18n();
const { loadEntry, entries, loadEntries } = useWikiExplorer();
const { currentProject } = useProject();
const layout = inject(MAIN_LAYOUT_KEY, null);

/** 项目路径（响应式）。 */
const projectPath = computed(() => currentProject.value?.project_path ?? '');

/** 条目详情。 */
const detail = ref<WikiEntryDetail | null>(null);
/** 是否加载中。 */
const loading = ref(false);
/** 错误信息。 */
const error = ref('');

/** 关联条目侧边栏是否折叠（折叠时展示关联类型统计摘要）。 */
const relationsCollapsed = ref(false);

/** `wikiID → 条目类型` 映射（来自模块级条目列表，用于关联条目分类统计）。 */
const typeById = computed(() => {
  const map = new Map<string, WikiType>();
  for (const entry of entries.value) map.set(entry.id, entry.wiki_type);
  return map;
});

/** 关联条目按类型计数（类型未知的 ID 忽略）。 */
const relationTypeCounts = computed(() => {
  const counts = new Map<WikiType, number>();
  for (const id of detail.value?.relations ?? []) {
    const type = typeById.value.get(id);
    if (!type) continue;
    counts.set(type, (counts.get(type) ?? 0) + 1);
  }
  return counts;
});

/** 折叠态统计条目（固定 概念 → 实体 → 综述 顺序，仅含非零类型）。 */
const relationStats = computed(() => {
  const order: WikiType[] = ['concept', 'entity', 'summary'];
  return order
    .map((type) => ({ type, count: relationTypeCounts.value.get(type) ?? 0 }))
    .filter((item) => item.count > 0);
});

/** 关联条目总数（含类型未知者）。 */
const relationTotal = computed(() => detail.value?.relations?.length ?? 0);

/** 关联跳转时点击的条目类型（未加载列表时可能为 undefined）。 */
function relationTypeOf(wikiId: string): WikiType | undefined {
  return typeById.value.get(wikiId);
}

/** iframe 引用。 */
const iframeRef = ref<HTMLIFrameElement | null>(null);
/** iframe 是否就绪。 */
let iframeReady = false;
/** 最新渲染请求 ID（防竞态）。 */
let renderRequestId = 0;

/** Wiki 条目标签页图标 SVG path（书本）。 */
const ICON_WIKI = 'M4 19.5A2.5 2.5 0 0 1 6.5 17H20 M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z';

/** 类型徽标标签。 */
function typeLabel(type: WikiType): string {
  return t(`main.sidebar.knowledge.typeLabel.${type}`);
}

/** 加载条目详情。 */
async function loadDetail(): Promise<void> {
  if (!projectPath.value || !props.wikiId) {
    detail.value = null;
    return;
  }
  loading.value = true;
  error.value = '';
  try {
    const result = await loadEntry(projectPath.value, props.wikiId);
    detail.value = result;
  } catch (err) {
    const msg = typeof err === 'string' ? err : (err as Error)?.message ?? String(err);
    error.value = msg;
    detail.value = null;
  } finally {
    loading.value = false;
  }
}

/** 将 HTML 写入 iframe 文档。 */
function writeToIframe(html: string): void {
  const iframe = iframeRef.value;
  if (!iframe) return;
  const doc = iframe.contentDocument;
  if (!doc) return;
  doc.open();
  doc.write(html);
  doc.close();
}

/** 渲染 MD 正文为 HTML 并写入 iframe。 */
async function renderContent(md: string): Promise<void> {
  if (!iframeReady) return;
  const requestId = ++renderRequestId;
  try {
    const html = await invoke<string>('editor_render_html', { content: md });
    if (requestId === renderRequestId) {
      writeToIframe(html);
    }
  } catch (err) {
    if (requestId === renderRequestId) {
      const msg = extractErrorMessage(err);
      writeToIframe(
        `<div style="color:#d32f2f;padding:1em;font-family:system-ui,sans-serif;font-size:14px;">渲染失败：${escapeHtml(msg)}</div>`,
      );
    }
  }
}

/** 从 Tauri 错误中提取可读消息。 */
function extractErrorMessage(err: unknown): string {
  if (err === null || err === undefined) return '未知错误';
  if (typeof err === 'string') return err;
  if (err instanceof Error) return err.message;
  if (typeof err === 'object' && 'message' in err) {
    const msg = (err as { message: unknown }).message;
    if (typeof msg === 'string') return msg;
  }
  try {
    return JSON.stringify(err);
  } catch {
    return String(err);
  }
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

/** 打开关联条目（在新 tab 或激活已存在 tab）。 */
function openRelation(wikiId: string, title: string): void {
  if (!wikiId) return;
  layout?.openTab({
    id: `wiki-${wikiId}`,
    title,
    type: 'wiki',
    wikiId,
    icon: ICON_WIKI,
  });
}

// wikiId 变化时重新加载
watch(
  () => props.wikiId,
  () => {
    void loadDetail();
  },
  { immediate: true },
);

// 项目切换时重新加载
watch(projectPath, () => {
  void loadDetail();
});

// detail 变化时渲染正文
watch(
  () => detail.value?.content,
  (content) => {
    if (content) {
      void renderContent(content);
    }
  },
);

// 条目列表未加载（直接打开 wiki tab 时）补拉一次，供关联类型统计使用
watch(detail, (d) => {
  if (d?.relations?.length && entries.value.length === 0 && projectPath.value) {
    void loadEntries(projectPath.value);
  }
});

onMounted(() => {
  iframeReady = true;
  // 若 detail 已先于 iframe 就绪，立即渲染
  if (detail.value?.content) {
    void renderContent(detail.value.content);
  }
});

onBeforeUnmount(() => {
  iframeReady = false;
});
</script>

<style scoped>
.wiki-reader {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  background: var(--fluen-canvas);
}

/* ── 顶部工具栏 ─────────────────────────────────────────────────────── */
.wiki-reader__toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 16px;
  border-bottom: 1px solid var(--fluen-hairline);
  background: var(--fluen-surface);
  flex-shrink: 0;
}

.wiki-reader__type {
  font-family: var(--fluen-font-sans);
  font-size: 10.5px;
  font-weight: 600;
  padding: 2px 8px;
  border-radius: 9999px;
  flex-shrink: 0;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.wiki-reader__type--concept {
  background: var(--fluen-info-bg);
  color: var(--fluen-info-text);
}

.wiki-reader__type--entity {
  background: var(--fluen-warning-bg);
  color: var(--fluen-warning-text);
}

.wiki-reader__type--summary {
  background: var(--fluen-success-bg);
  color: var(--fluen-success-text);
}

.wiki-reader__title {
  flex: 1;
  min-width: 0;
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 15px;
  font-weight: 600;
  color: var(--fluen-ink);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.wiki-reader__close {
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
  border-radius: 5px;
  flex-shrink: 0;
  transition: background 0.14s ease, color 0.14s ease;
}

.wiki-reader__close:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

/* ── 标签行 ─────────────────────────────────────────────────────────── */
.wiki-reader__tags {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding: 8px 16px;
  border-bottom: 1px solid var(--fluen-hairline);
  background: var(--fluen-surface);
  flex-shrink: 0;
}

.wiki-reader__tag {
  font-family: var(--fluen-font-sans);
  font-size: 11px;
  color: var(--fluen-steel);
  background: var(--fluen-hover);
  border-radius: 4px;
  padding: 2px 8px;
}

/* ── 主体 ───────────────────────────────────────────────────────────── */
.wiki-reader__body {
  flex: 1;
  display: flex;
  min-height: 0;
  overflow: hidden;
}

.wiki-reader__content {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  background: var(--fluen-canvas);
}

.wiki-reader__iframe {
  width: 100%;
  height: 100%;
  border: none;
  background: var(--fluen-canvas);
  display: block;
}

/* ── 关联条目侧边栏 ─────────────────────────────────────────────────── */
.wiki-reader__relations {
  width: 240px;
  flex-shrink: 0;
  border-left: 1px solid var(--fluen-hairline);
  background: var(--fluen-surface);
  overflow-y: auto;
  padding: 12px 0;
}

.wiki-reader__relations-title {
  margin: 0;
  padding: 0 0 0 14px;
  font-family: var(--fluen-font-sans);
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--fluen-stone);
  display: flex;
  align-items: center;
  gap: 6px;
}

.wiki-reader__relations-count {
  font-size: 10px;
  font-weight: 600;
  line-height: 1;
  padding: 3px 6px;
  border-radius: 9999px;
  background: var(--fluen-hover);
  color: var(--fluen-steel);
  text-transform: none;
  letter-spacing: 0;
}

.wiki-reader__relations-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
  padding-right: 8px;
}

.wiki-reader__relations-toggle {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
  border-radius: 4px;
  transition: background 0.14s ease, color 0.14s ease;
}

.wiki-reader__relations-toggle:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

/* 折叠态：窄条，展示关联条目类型统计摘要 */
.wiki-reader__relations--collapsed {
  width: 36px;
  padding: 8px 0;
  display: flex;
  align-items: stretch;
}

.wiki-reader__relations-collapsed {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 8px 0;
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
  border-radius: 6px;
  transition: background 0.14s ease, color 0.14s ease;
}

.wiki-reader__relations-collapsed:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.wiki-reader__collapsed-link {
  flex-shrink: 0;
  color: var(--fluen-steel);
}

.wiki-reader__collapsed-stats {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
}

.wiki-reader__collapsed-stat {
  display: flex;
  align-items: center;
  gap: 4px;
}

.wiki-reader__stat-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
}

.wiki-reader__stat-dot--concept {
  background: var(--fluen-info-text);
}

.wiki-reader__stat-dot--entity {
  background: var(--fluen-warning-text);
}

.wiki-reader__stat-dot--summary {
  background: var(--fluen-success-text);
}

.wiki-reader__stat-num {
  font-family: var(--fluen-font-sans);
  font-size: 11px;
  font-weight: 600;
  color: var(--fluen-steel);
}

.wiki-reader__collapsed-empty {
  font-family: var(--fluen-font-sans);
  font-size: 11px;
  color: var(--fluen-steel);
  opacity: 0.6;
}

.wiki-reader__relations-list {
  list-style: none;
  margin: 0;
  padding: 0;
}

.wiki-reader__relation {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 7px 14px;
  cursor: pointer;
  transition: background 0.14s ease;
}

.wiki-reader__relation:hover {
  background: var(--fluen-hover);
}

.wiki-reader__relation-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
}

.wiki-reader__relation-dot--concept {
  background: var(--fluen-info-text);
}

.wiki-reader__relation-dot--entity {
  background: var(--fluen-warning-text);
}

.wiki-reader__relation-dot--summary {
  background: var(--fluen-success-text);
}

.wiki-reader__relation-text {
  flex: 1;
  min-width: 0;
  font-family: var(--fluen-font-sans);
  font-size: 12.5px;
  color: var(--fluen-ink);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.wiki-reader__relation-icon {
  flex-shrink: 0;
  color: var(--fluen-steel);
  opacity: 0;
  transition: opacity 0.14s ease;
}

.wiki-reader__relation:hover .wiki-reader__relation-icon {
  opacity: 1;
}

.wiki-reader__relations-empty {
  margin: 0;
  padding: 4px 14px;
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  color: var(--fluen-steel);
  line-height: 1.5;
}

/* ── 状态（加载/错误/未找到）─────────────────────────────────────────── */
.wiki-reader__status {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 14px;
  color: var(--fluen-stone);
  font-family: var(--fluen-font-sans);
  font-size: 13px;
}

.wiki-reader__status--error {
  color: #b91c1c;
}

.wiki-reader__error-icon {
  font-size: 28px;
}

.wiki-reader__spinner {
  width: 28px;
  height: 28px;
  border: 2.5px solid var(--fluen-hairline);
  border-top-color: var(--fluen-accent);
  border-radius: 50%;
  animation: wiki-reader-spin 0.8s linear infinite;
}

@keyframes wiki-reader-spin {
  to { transform: rotate(360deg); }
}
</style>
