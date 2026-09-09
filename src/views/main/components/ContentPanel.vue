<script setup lang="ts">
/**
 * ContentPanel — 中间内容区。
 *
 * 两层结构：
 *   标签栏（Tab Bar）   — 已打开文件的标签页列表
 *   编辑区（Editor）    — 四视图互斥：仅源码 / 半预览(live) / 预览编辑(实验) / 仅渲染
 *     · 仅源码：FluenEditor 源码形态
 *     · 半预览：同一 FluenEditor 开启实时渲染（WYSIWYG，由 livePreview 扩展实现）
 *     · 预览编辑：FluenWysiwygEditor（TipTap 实验视图，v-if 独立挂载）
 *     · 仅渲染：FluenPreview 后端 HTML 预览
 *
 * 无标签页时展示欢迎页（Welcome）。
 * 项目打开后，自动创建一个以项目标题命名的标签页，编辑区渲染 `main.md`。
 *
 * 数据流：`FluenEditor / FluenWysiwygEditor @doc-change(md)` → 本地 `liveMd` ref
 * → `FluenPreview :md`。离开预览编辑视图时将 liveMd 回灌 CM6（内容相同则跳过，
 * 保留撤销历史）。外部 `mainMd` 变化（项目切换、保存归一化）通过 `watch` 同步到
 * `liveMd` 与编辑器。
 *
 * 大纲跳转由 `useOutline.jumpTo` 内部调用 `useFluenEditor().scrollToLine` 完成。
 */
import { ref, watch, onMounted, onUnmounted, computed, inject } from 'vue';
import type { UnlistenFn } from '@tauri-apps/api/event';
import { listen } from '@tauri-apps/api/event';
import type { ContentTab } from '../types';
import { useI18n } from '../../../i18n';
import { useProject } from '../../../composables/useProject';
import { FluenEditor, FluenPreview, FluenWysiwygEditor, EditorToolbar, EditorLayoutSwitch, EditorQuoteToolbar, useFluenEditor } from './editor';
import { MAIN_LAYOUT_KEY } from '../composables/useMainLayout';
import { ReferenceReader, WikiReader } from './reader';
import DatasetViewer from './data/DatasetViewer.vue';
import RecentProjects from './welcome/RecentProjects.vue';

const { t } = useI18n();
const { hasProject, config, mainMd, refreshProject } = useProject();

/* ── 编辑器视图模式（注入 MainView 共享布局实例） ─────────────────── */
const layout = inject(MAIN_LAYOUT_KEY);
const editorLayout = computed(() => layout?.editorLayout.value ?? 'preview');
const editor = useFluenEditor();

// 视图切换语义：
//   - 进入/离开半预览时驱动同一 CM6 实例的实时渲染开关（StateEffect，零损耗）
//   - 离开预览编辑时把 TipTap 最新产出（liveMd）回灌 CM6，两个编辑面内容一致；
//     内容相同则跳过，避免丢弃 CM6 撤销历史
//   - v-show 显隐后让 CM6 立即重新测量，避免容器尺寸从 0 恢复时的测量延迟
watch(
  editorLayout,
  (mode, prev) => {
    if (prev === 'wysiwyg' && mode !== 'wysiwyg' && editor.getMd() !== liveMd.value) {
      editor.setMd(liveMd.value);
    }
    editor.setLivePreview(mode === 'live');
    editor.requestMeasure();
  },
);


const props = defineProps<{
  /** 已打开的标签页列表 */
  tabs: readonly ContentTab[];
  /** 当前激活的标签页 id */
  activeTabId: string | null;
}>();

defineEmits<{
  (e: 'select-tab', id: string): void;
  (e: 'close-tab', id: string): void;
  (e: 'new-article'): void;
  (e: 'open-article'): void;
}>();

/** 当前激活的标签页对象。 */
const activeTab = computed(
  () => props.tabs.find((t) => t.id === props.activeTabId) ?? null,
);

/* ── 项目标题（用于内容区头部显示） ───────────────────────────────── */
const projectTitle = ref('');
watch(
  [hasProject, config],
  () => {
    projectTitle.value = config.value?.title ?? '';
  },
  { immediate: true },
);

onMounted(() => {
  projectTitle.value = config.value?.title ?? '';
});

/* ── 双栏数据流：编辑器内容 → 预览 ───────────────────────────────── */
/**
 * 实时 MD 文本——驱动右栏预览。
 *
 * 初始值取 `mainMd`；编辑器 `doc-change` 时实时更新；
 * 外部 `mainMd` 变化（项目切换、保存归一化）回写到 `liveMd`。
 */
const liveMd = ref(mainMd.value);

watch(mainMd, (newMd) => {
  if (liveMd.value !== newMd) {
    liveMd.value = newMd;
  }
});

function onDocChange(md: string): void {
  liveMd.value = md;
}

/* ── AI 写入正文后的自动刷新 ─────────────────────────────────────────── */
/**
 * 监听 `motis:project-updated`（AI 经 manuscript 工具写入正文成功）：
 * 编辑器无未保存修改时重新拉取项目内容——`mainMd` 变化会自动同步到
 * 编辑器（FluenEditor 的 watch）、预览（liveMd）与大纲。
 * 用户正在编辑（脏缓冲）时跳过，避免覆盖未保存的内容。
 */
let unlistenProjectUpdated: UnlistenFn | null = null;

onMounted(async () => {
  if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
    unlistenProjectUpdated = await listen('motis:project-updated', () => {
      if (useFluenEditor().isDirty.value) return;
      void refreshProject();
    });
  }
});

onUnmounted(() => {
  unlistenProjectUpdated?.();
  unlistenProjectUpdated = null;
});
</script>

<template>
  <div class="content-panel">
    <!-- ── 标签栏 ─────────────────────────────────────────────────────── -->
    <div class="tab-bar" v-if="tabs.length > 0">
      <div class="tab-bar__list">
        <div
          v-for="tab in tabs"
          :key="tab.id"
          class="tab-bar__item"
          :class="{ 'tab-bar__item--active': tab.id === activeTabId }"
          role="tab"
          :aria-selected="tab.id === activeTabId"
          tabindex="0"
          @click="$emit('select-tab', tab.id)"
          @keydown.enter="$emit('select-tab', tab.id)"
        >
          <svg
            v-if="tab.icon"
            class="tab-bar__icon"
            viewBox="0 0 24 24"
            width="14"
            height="14"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
          >
            <path :d="tab.icon" />
          </svg>
          <span class="tab-bar__label">{{ tab.title }}</span>
          <span
            v-if="tab.dirty"
            class="tab-bar__dirty"
          />
          <button
            class="tab-bar__close"
            @click.stop="$emit('close-tab', tab.id)"
          >
            <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
              <path d="M18 6 6 18M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>
    </div>

    <!-- ── 编辑区 ─────────────────────────────────────────────────────── -->
    <div class="editor-area">
      <!-- 欢迎页（无标签页时） -->
      <RecentProjects
        v-if="tabs.length === 0"
        @new-article="$emit('new-article')"
        @open-article="$emit('open-article')"
      />

      <!-- 文献阅读器（关闭由标签页负责） -->
      <ReferenceReader
        v-else-if="activeTab?.type === 'reference' && activeTab.referenceId"
        :reference-id="activeTab.referenceId"
      />

      <!-- 知识库条目阅读器 -->
      <WikiReader
        v-else-if="activeTab?.type === 'wiki' && activeTab.wikiId"
        :wiki-id="activeTab.wikiId"
        @close="$emit('close-tab', activeTab.id)"
      />

      <!-- 数据表查看器（数据分析面板选中数据表时打开）；
           key 保证每个数据表标签页持有独立实例，切换时内容随之切换 -->
      <DatasetViewer
        v-else-if="activeTab?.type === 'dataset' && activeTab.datasetPath && activeTab.datasetKind"
        :key="activeTab.id"
        :path="activeTab.datasetPath"
        :kind="activeTab.datasetKind"
      />

      <!-- 四视图编辑区（有项目时）：source / live 共用同一编辑器，wysiwyg 独立挂载 -->
      <div v-else-if="hasProject" class="content-editor">
        <!-- 工具栏（source / live / wysiwyg 显示；命令经 useFluenEditor 路由到当前编辑面） -->
        <EditorToolbar v-show="editorLayout !== 'preview'" />
        <!-- 编辑器侧（source 与 live 模式均可见；live 下为实时渲染形态） -->
        <div v-show="editorLayout === 'source' || editorLayout === 'live'" class="content-editor__editing">
          <FluenEditor
            :md="mainMd"
            class="content-editor__canvas"
            @doc-change="onDocChange"
          />
        </div>
        <!-- 预览编辑侧（实验）：TipTap WYSIWYG，v-if 独立挂载（编辑内容经
             onDocChange 流入 liveMd，与 FluenEditor 共用同一数据通路） -->
        <FluenWysiwygEditor
          v-if="editorLayout === 'wysiwyg'"
          :md="liveMd"
          class="content-editor__wysiwyg"
          @doc-change="onDocChange"
        />
        <!-- 仅渲染侧：后端 HTML 预览 -->
        <FluenPreview v-show="editorLayout === 'preview'" :md="liveMd" class="content-editor__preview" />
        <!-- 视图模式切换（仅编辑视图显示，阅读器不挂载本组件） -->
        <EditorLayoutSwitch />
        <!-- 论文划选「添加到对话」浮层（编辑器上方，框选文本时可见）；
             依赖 CM6 选区坐标，预览编辑视图下隐藏 -->
        <EditorQuoteToolbar v-show="editorLayout !== 'wysiwyg'" />
      </div>

      <!-- 编辑器占位（有标签页但无项目内容时） -->
      <div v-else class="editor-placeholder">
        <p class="editor-placeholder__text">
          {{ t('main.content.editorPlaceholder', { name: activeTab?.title ?? '' }) }}
        </p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.content-panel {
  flex: 1;
  display: flex;
  flex-direction: column;
  height: 100%;
  min-width: 0;
  background: var(--fluen-canvas);
  overflow: hidden;
}

/* ── 标签栏 ───────────────────────────────────────────────────────────── */
.tab-bar {
  display: flex;
  flex-shrink: 0;
  background: var(--fluen-surface);
  border-bottom: 1px solid var(--fluen-hairline);
}

.tab-bar__list {
  display: flex;
  align-items: stretch;
  height: 36px;
  overflow-x: auto;
  scrollbar-width: none;
}

.tab-bar__list::-webkit-scrollbar {
  display: none;
}

.tab-bar__item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 8px 0 12px;
  border: none;
  border-right: 1px solid var(--fluen-hairline);
  background: transparent;
  color: var(--fluen-slate);
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  white-space: nowrap;
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease;
}

.tab-bar__item:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.tab-bar__item--active {
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
}

.tab-bar__icon {
  flex-shrink: 0;
  color: var(--fluen-stone);
}

.tab-bar__label {
  max-width: 160px;
  overflow: hidden;
  text-overflow: ellipsis;
}

.tab-bar__dirty {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--fluen-accent);
  flex-shrink: 0;
}

.tab-bar__close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
  border-radius: 4px;
  transition: background 0.15s ease, color 0.15s ease;
  flex-shrink: 0;
}

.tab-bar__close:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

/* ── 编辑区 ───────────────────────────────────────────────────────────── */
.editor-area {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  position: relative;
}

/* 编辑器占位 */
.editor-placeholder {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
}

.editor-placeholder__text {
  font-family: var(--fluen-font-sans);
  font-size: 14px;
  color: var(--fluen-stone);
}

/* ── 四视图编辑区（source / live / wysiwyg / preview 互斥） ────────────── */
.content-editor {
  flex: 1;
  display: flex;
  flex-direction: column;
  height: 100%;
  width: 100%;
  overflow: hidden;
  position: relative; /* 供 EditorLayoutSwitch 右上角浮动定位 */
}

.content-editor__editing {
  flex: 1;
  min-width: 0;
  /* 关键：flex 列项默认 min-height:auto，会被编辑器内容（cm-scroller 的
     min-content = 全文高度）撑开，导致 .cm-scroller 失去视口约束、
     滚轮无法滚动。置 0 让 height:100% 链重新生效。 */
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.content-editor__canvas {
  flex: 1;
  min-width: 0;
}

.content-editor__preview {
  flex: 1;
  min-width: 0;
}

.content-editor__wysiwyg {
  flex: 1;
  min-width: 0;
}
</style>
