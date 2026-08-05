<script setup lang="ts">
/**
 * ContentPanel — 中间内容区。
 *
 * 两层结构：
 *   标签栏（Tab Bar）   — 已打开文件的标签页列表
 *   编辑区（Editor）    — 双栏：左 MD 源码编辑（FluenEditor）+ 右实时 HTML 预览（FluenPreview）
 *
 * 无标签页时展示欢迎页（Welcome）。
 * 项目打开后，自动创建一个以项目标题命名的标签页，编辑区渲染 `.temp.md`。
 *
 * 数据流：`FluenEditor @doc-change(md)` → 本地 `liveMd` ref → `FluenPreview :md`。
 * 外部 `tempMd` 变化（项目切换、保存归一化）通过 `watch` 同步到 `liveMd` 与编辑器。
 *
 * 大纲跳转由 `useOutline.jumpTo` 内部调用 `useFluenEditor().scrollToLine` 完成。
 */
import { ref, watch, onMounted, computed } from 'vue';
import type { ContentTab } from '../types';
import { useI18n } from '../../../i18n';
import { useProject } from '../../../composables/useProject';
import { FluenEditor, FluenPreview } from './editor';
import { ReferenceReader, WikiReader } from './reader';
import RecentProjects from './welcome/RecentProjects.vue';

const { t } = useI18n();
const { hasProject, config, tempMd } = useProject();

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
 * 初始值取 `tempMd`；编辑器 `doc-change` 时实时更新；
 * 外部 `tempMd` 变化（项目切换、保存归一化）回写到 `liveMd`。
 */
const liveMd = ref(tempMd.value);

watch(tempMd, (newMd) => {
  if (liveMd.value !== newMd) {
    liveMd.value = newMd;
  }
});

function onDocChange(md: string): void {
  liveMd.value = md;
}
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

      <!-- 文献阅读器 -->
      <ReferenceReader
        v-else-if="activeTab?.type === 'reference' && activeTab.referenceId"
        :reference-id="activeTab.referenceId"
        @close="$emit('close-tab', activeTab.id)"
      />

      <!-- 知识库条目阅读器 -->
      <WikiReader
        v-else-if="activeTab?.type === 'wiki' && activeTab.wikiId"
        :wiki-id="activeTab.wikiId"
        @close="$emit('close-tab', activeTab.id)"
      />

      <!-- 双栏视图（有项目时）：左 MD 源码 + 右 HTML 预览 -->
      <div v-else-if="hasProject" class="content-split">
        <FluenEditor
          :md="tempMd"
          class="content-split__editor"
          @doc-change="onDocChange"
        />
        <FluenPreview :md="liveMd" class="content-split__preview" />
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

/* ── 分栏视图 ─────────────────────────────────────────────────────────── */
.content-split {
  flex: 1;
  display: flex;
  height: 100%;
  width: 100%;
  overflow: hidden;
}

.content-split__editor {
  flex: 1;
  min-width: 0;
  border-right: 1px solid var(--fluen-hairline);
}

.content-split__preview {
  flex: 1;
  min-width: 0;
}
</style>
