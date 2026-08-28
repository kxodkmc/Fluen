<script setup lang="ts">
/**
 * DataAnalysisPanel — 数据分析面板（两级导航）。
 *
 * 一级：数据表列表 —— 展示项目 data 目录下已导入的数据表
 *       （questionnaires / experiments 分组），可导入新 CSV
 *       （选择问卷 / 实验类型后归档到对应子目录）。
 * 二级：数据分析 —— 选中数据表后进入，包含分析配置与结果；
 *       同时在中间内容栏打开该数据表的宽版表格预览。
 *       返回一级可切换另一份数据表。
 *
 * 子组件：
 *   - DataTypeChoice：导入时的数据类型选择
 *   - AnalysisWorkbench：分析类别配置与执行
 *   - AnalysisResult：统计结果渲染
 *   - DatasetViewer（中间内容栏）：数据表预览
 */
import { ref, computed, onMounted, watch, inject } from 'vue';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { useI18n } from '../../../../../i18n';
import { useProject } from '../../../../../composables/useProject';
import {
  loadDataset,
  listDatasets,
  importDataset,
} from '../../../../../composables/useDataAnalysis';
import type {
  DatasetEntry,
  DatasetKind,
  DatasetSchema,
} from '../../../../../types/dataAnalysis';
import type { ContentTab } from '../../../types';
import { MAIN_LAYOUT_KEY } from '../../../composables/useMainLayout';
import DataTypeChoice from './dataAnalysis/DataTypeChoice.vue';
import AnalysisWorkbench from './dataAnalysis/AnalysisWorkbench.vue';
import AnalysisResult from './AnalysisResult.vue';

const { t } = useI18n();
const { currentProject } = useProject();

const projectPath = computed(() => currentProject.value?.project_path ?? '');
const hasProject = computed(() => !!projectPath.value);

/** 主界面布局（inject 自 MainView，用于打开内容栏数据表标签页）。 */
const layout = inject(MAIN_LAYOUT_KEY, null);

/** 数据表标签页图标。 */
const TABLE_ICON = 'M3 5h18v14H3zM3 10h18M9 10v9M15 10v9';

// --- 一级：数据表列表 ---
const entries = ref<DatasetEntry[]>([]);
const listLoading = ref(false);
const listError = ref('');

const questionnaireEntries = computed(() =>
  entries.value.filter((e) => e.kind === 'questionnaire'),
);
const experimentEntries = computed(() =>
  entries.value.filter((e) => e.kind === 'experiment'),
);

async function refreshList(): Promise<void> {
  if (!hasProject.value) return;
  listLoading.value = true;
  listError.value = '';
  try {
    entries.value = await listDatasets(projectPath.value);
  } catch (err) {
    listError.value = String(err);
  } finally {
    listLoading.value = false;
  }
}

onMounted(refreshList);
watch(hasProject, refreshList);

// --- 导入流程（选文件 → 选类型 → 归档） ---
const pendingPath = ref('');
const choosingType = ref(false);
const importing = ref(false);
const importError = ref('');

async function pickFile(): Promise<void> {
  if (!hasProject.value || importing.value) return;
  const selected = await openDialog({
    title: t('main.sidebar.data.importTitle'),
    directory: false,
    multiple: false,
    defaultPath: `${projectPath.value}/data`,
    filters: [{ name: 'CSV', extensions: ['csv'] }],
  });
  if (typeof selected === 'string') {
    pendingPath.value = selected;
    choosingType.value = true;
  }
}

function cancelChooseType(): void {
  choosingType.value = false;
  pendingPath.value = '';
}

async function onConfirmKind(kind: DatasetKind): Promise<void> {
  if (!pendingPath.value) return;
  importing.value = true;
  importError.value = '';
  try {
    const entry = await importDataset(projectPath.value, pendingPath.value, kind);
    choosingType.value = false;
    pendingPath.value = '';
    await refreshList();
    await openDataset(entry);
  } catch (err) {
    importError.value = String(err);
  } finally {
    importing.value = false;
  }
}

// --- 两级导航：list=数据表列表（一级），work=数据分析（二级） ---
const level = ref<'list' | 'work'>('list');

// --- 二级：数据分析 ---
const activeEntry = ref<DatasetEntry | null>(null);
const schema = ref<DatasetSchema | null>(null);
const schemaLoading = ref(false);
const workError = ref('');

type ViewId = 'analysis' | 'results';
const view = ref<ViewId>('analysis');

const views = computed(() => [
  { id: 'analysis' as ViewId, label: t('main.sidebar.data.tabAnalysis') },
  { id: 'results' as ViewId, label: t('main.sidebar.data.tabResults') },
]);

// --- 分析结果 ---
const result = ref<unknown>(null);
const resultType = ref('');

/** 打开一份数据表：侧边栏进入二级，中间内容栏打开表格预览。 */
async function openDataset(entry: DatasetEntry): Promise<void> {
  level.value = 'work';
  activeEntry.value = entry;
  schema.value = null;
  result.value = null;
  resultType.value = '';
  workError.value = '';
  view.value = 'analysis';

  layout?.openTab({
    id: `dataset-${entry.path}`,
    title: entry.name,
    type: 'dataset',
    icon: TABLE_ICON,
    datasetPath: entry.path,
    datasetKind: entry.kind,
  } satisfies ContentTab);

  schemaLoading.value = true;
  try {
    schema.value = await loadDataset(entry.path);
  } catch (err) {
    workError.value = String(err);
  } finally {
    schemaLoading.value = false;
  }
}

/** 返回一级数据表列表（内容栏标签页保留，可随时切换）。 */
function backToList(): void {
  level.value = 'list';
  activeEntry.value = null;
  schema.value = null;
  result.value = null;
  resultType.value = '';
  workError.value = '';
  refreshList();
}

function onRan(type: string, r: unknown): void {
  result.value = r;
  resultType.value = type;
  view.value = 'results';
}
</script>

<template>
  <div class="da-panel">
    <!-- 未打开项目 -->
    <template v-if="!hasProject">
      <p class="da-panel__empty">{{ t('main.sidebar.data.noProject') }}</p>
    </template>

    <!-- ── 一级：数据表列表 ────────────────────────────────────────── -->
    <template v-else-if="level === 'list'">
      <div class="da-head">
        <span class="da-head__title">{{ t('main.sidebar.data.selectTable') }}</span>
        <button class="da-head__btn" :disabled="importing" @click="pickFile">
          {{ t('main.sidebar.data.importTitle') }}
        </button>
      </div>

      <!-- 导入中：选择数据类型 -->
      <template v-if="choosingType">
        <DataTypeChoice @confirm="onConfirmKind" />
        <div class="da-import-actions">
          <button class="da-link" :disabled="importing" @click="cancelChooseType">
            {{ t('main.sidebar.data.cancel') }}
          </button>
        </div>
        <p v-if="importError" class="da-panel__error">{{ importError }}</p>
      </template>

      <!-- 列表 -->
      <template v-else>
        <p v-if="importing" class="da-panel__empty">{{ t('main.sidebar.data.importing') }}</p>
        <p v-else-if="listLoading" class="da-panel__empty">{{ t('main.sidebar.data.loadingFile') }}</p>
        <p v-else-if="listError" class="da-panel__error">{{ listError }}</p>

        <template v-else-if="entries.length > 0">
          <div v-if="questionnaireEntries.length" class="da-group">
            <p class="da-group__title">{{ t('main.sidebar.data.groupQuestionnaire') }}</p>
            <button
              v-for="e in questionnaireEntries"
              :key="e.path"
              class="da-entry"
              @click="openDataset(e)"
            >
              <span class="da-entry__icon">
                <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                  <path :d="TABLE_ICON" />
                </svg>
              </span>
              <span class="da-entry__main">
                <span class="da-entry__name">{{ e.name }}</span>
                <span class="da-entry__meta">
                  {{ t('main.sidebar.data.nRows', { n: e.n_rows }) }} ·
                  {{ t('main.sidebar.data.nVars', { n: e.n_vars }) }}
                </span>
              </span>
            </button>
          </div>

          <div v-if="experimentEntries.length" class="da-group">
            <p class="da-group__title">{{ t('main.sidebar.data.groupExperiment') }}</p>
            <button
              v-for="e in experimentEntries"
              :key="e.path"
              class="da-entry"
              @click="openDataset(e)"
            >
              <span class="da-entry__icon">
                <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                  <path :d="TABLE_ICON" />
                </svg>
              </span>
              <span class="da-entry__main">
                <span class="da-entry__name">{{ e.name }}</span>
                <span class="da-entry__meta">
                  {{ t('main.sidebar.data.nRows', { n: e.n_rows }) }} ·
                  {{ t('main.sidebar.data.nVars', { n: e.n_vars }) }}
                </span>
              </span>
            </button>
          </div>
        </template>

        <!-- 空状态 -->
        <div v-else class="da-empty">
          <p class="da-empty__title">{{ t('main.sidebar.data.listEmpty') }}</p>
          <p class="da-empty__desc">{{ t('main.sidebar.data.importHint') }}</p>
          <p class="da-empty__note">{{ t('main.sidebar.data.csvOnly') }}</p>
        </div>
      </template>
    </template>

    <!-- ── 二级：数据分析 ──────────────────────────────────────────── -->
    <template v-else-if="activeEntry">
      <!-- 头部：返回 + 文件名 + 类型标签 -->
      <div class="da-head">
        <div class="da-head__row">
          <button class="da-back" @click="backToList">
            <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M15 18l-6-6 6-6" />
            </svg>
            {{ t('main.sidebar.data.back') }}
          </button>
        </div>
        <div class="da-head__row">
          <span class="da-head__name" :title="activeEntry.path">{{ activeEntry.name }}</span>
          <span class="da-tag" :class="`da-tag--${activeEntry.kind}`">
            {{ t(`main.sidebar.data.type_${activeEntry.kind}`) }}
          </span>
        </div>
        <span class="da-head__meta">
          {{ t('main.sidebar.data.nRows', { n: activeEntry.n_rows }) }} ·
          {{ t('main.sidebar.data.nVars', { n: activeEntry.n_vars }) }}
        </span>
      </div>

      <!-- 视图标签 -->
      <div class="da-views">
        <button
          v-for="v in views"
          :key="v.id"
          class="da-views__btn"
          :class="{ 'da-views__btn--active': view === v.id }"
          @click="view = v.id"
        >
          {{ v.label }}
        </button>
      </div>

      <p v-if="schemaLoading" class="da-panel__empty">{{ t('main.sidebar.data.loadingFile') }}</p>
      <p v-else-if="workError" class="da-panel__error">{{ workError }}</p>

      <!-- 分析配置 -->
      <AnalysisWorkbench
        v-else-if="view === 'analysis' && schema"
        :path="activeEntry.path"
        :schema="schema"
        :kind="activeEntry.kind"
        @ran="onRan"
      />

      <!-- 分析结果 -->
      <template v-else-if="view === 'results'">
        <AnalysisResult v-if="result" :result="result" :type="resultType" />
        <p v-else class="da-panel__empty">{{ t('main.sidebar.data.resultEmpty') }}</p>
      </template>
    </template>
  </div>
</template>

<style scoped>
.da-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 12px;
  gap: 10px;
  overflow-y: auto;
}
.da-panel__empty {
  font-size: 13px;
  color: var(--fluen-stone);
  text-align: center;
  padding: 16px 0;
}
.da-panel__error {
  margin: 0;
  font-size: 12px;
  color: var(--fluen-danger, #e8463a);
  word-break: break-all;
}

/* ── 头部 ── */
.da-head {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.da-head__row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.da-head__title {
  font-size: 13px;
  font-weight: 600;
  color: var(--fluen-ink);
  flex: 1;
}
.da-head__btn {
  border: 1px solid var(--fluen-hairline);
  background: var(--fluen-surface);
  color: var(--fluen-ink);
  font-size: 12px;
  padding: 5px 10px;
  border-radius: 9999px;
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease;
}
.da-head__btn:hover:not(:disabled) {
  background: var(--fluen-hover);
  border-color: var(--fluen-accent);
}
.da-head__btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.da-head__name {
  font-size: 13px;
  font-weight: 600;
  color: var(--fluen-ink);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.da-head__meta {
  font-size: 12px;
  color: var(--fluen-stone);
}
.da-back {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  font-size: 12px;
  padding: 2px 4px;
  cursor: pointer;
  border-radius: 6px;
  transition: color 0.15s ease;
}
.da-back:hover {
  color: var(--fluen-ink);
}
.da-tag {
  flex-shrink: 0;
  font-size: 11px;
  line-height: 1;
  padding: 4px 8px;
  border-radius: 9999px;
}
.da-tag--questionnaire {
  color: var(--fluen-success, #1a7f4e);
  background: color-mix(in srgb, var(--fluen-success, #1a7f4e) 12%, transparent);
}
.da-tag--experiment {
  color: var(--fluen-accent);
  background: color-mix(in srgb, var(--fluen-accent) 12%, transparent);
}

/* ── 一级：分组列表 ── */
.da-group {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.da-group__title {
  margin: 4px 0 2px 0;
  font-size: 11px;
  color: var(--fluen-stone);
}
.da-entry {
  display: flex;
  align-items: center;
  gap: 8px;
  border: 1px solid transparent;
  background: transparent;
  padding: 7px 8px;
  border-radius: 8px;
  cursor: pointer;
  text-align: left;
  transition: background 0.15s ease, border-color 0.15s ease;
}
.da-entry:hover {
  background: var(--fluen-hover);
  border-color: var(--fluen-hairline);
}
.da-entry__icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border-radius: 6px;
  background: var(--fluen-hover);
  color: var(--fluen-stone);
  flex-shrink: 0;
}
.da-entry__main {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}
.da-entry__name {
  font-size: 13px;
  color: var(--fluen-ink);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.da-entry__meta {
  font-size: 11px;
  color: var(--fluen-stone);
}

/* 空状态 */
.da-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 40px 12px;
  text-align: center;
}
.da-empty__title {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--fluen-ink);
}
.da-empty__desc {
  margin: 0;
  font-size: 12px;
  line-height: 1.5;
  color: var(--fluen-stone);
  max-width: 240px;
}
.da-empty__note {
  margin: 2px 0 0 0;
  font-size: 11px;
  color: var(--fluen-stone);
}

/* 导入类型选择 */
.da-import-actions {
  display: flex;
  justify-content: center;
}
.da-link {
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  font-size: 12px;
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 6px;
}
.da-link:hover:not(:disabled) {
  color: var(--fluen-ink);
}
.da-link:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* ── 二级：视图标签 ── */
.da-views {
  display: flex;
  gap: 2px;
  border-bottom: 1px solid var(--fluen-hairline);
}
.da-views__btn {
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  font-size: 12px;
  padding: 6px 10px;
  cursor: pointer;
  border-bottom: 2px solid transparent;
  margin-bottom: -1px;
  transition: color 0.15s ease, border-color 0.15s ease;
}
.da-views__btn:hover {
  color: var(--fluen-ink);
}
.da-views__btn--active {
  color: var(--fluen-ink);
  font-weight: 600;
  border-bottom-color: var(--fluen-accent);
}
</style>
