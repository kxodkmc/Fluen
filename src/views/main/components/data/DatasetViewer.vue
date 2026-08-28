<script setup lang="ts">
/**
 * DatasetViewer — 中间内容栏的数据表查看器。
 *
 * 由内容区 `dataset` 类型标签页挂载，展示一份数据表的宽版预览：
 *   头部：文件名 + 数据类型标签 + 行/变量数
 *   主体：左侧变量列表（点击高亮定位列），右侧数据表格
 *         （表头吸顶、行号列固定、可横向滚动）
 *
 * 仅做展示，不承担分析配置——分析在左侧数据分析面板中进行。
 */
import { ref, computed, watch, onMounted } from 'vue';
import { useI18n } from '../../../../i18n';
import { loadDataset, previewRows } from '../../../../composables/useDataAnalysis';
import type { DatasetKind, DatasetPreview, DatasetSchema } from '../../../../types/dataAnalysis';

const props = defineProps<{
  /** 数据文件绝对路径。 */
  path: string;
  /** 数据类型（问卷 / 实验）。 */
  kind: DatasetKind;
}>();

const { t } = useI18n();

const schema = ref<DatasetSchema | null>(null);
const preview = ref<DatasetPreview | null>(null);
const loading = ref(true);
const error = ref('');

/** 当前高亮的列索引（-1 表示未选中）。 */
const activeCol = ref(-1);

const fileName = computed(() => props.path.split(/[\\/]/).pop() ?? '');

/** 加载 schema 与预览行（路径变化时重新加载）。 */
async function load(): Promise<void> {
  loading.value = true;
  error.value = '';
  schema.value = null;
  preview.value = null;
  activeCol.value = -1;
  try {
    schema.value = await loadDataset(props.path);
    preview.value = await previewRows(props.path);
  } catch (err) {
    error.value = String(err);
  } finally {
    loading.value = false;
  }
}

onMounted(load);

// 切换数据表（标签页复用实例时）跟随路径重新加载
watch(() => props.path, load);

function typeBadge(numeric: boolean): string {
  return numeric ? 'N' : 'T';
}
</script>

<template>
  <div class="dsv">
    <!-- 头部：文件名 + 类型 + 规模 -->
    <div class="dsv__head">
      <span class="dsv__name" :title="path">{{ fileName }}</span>
      <span class="dsv-tag" :class="`dsv-tag--${kind}`">
        {{ t(`main.sidebar.data.type_${kind}`) }}
      </span>
      <span v-if="schema" class="dsv__meta">
        {{ t('main.sidebar.data.nRows', { n: schema.n_rows }) }} ·
        {{ t('main.sidebar.data.nVars', { n: schema.n_vars }) }}
      </span>
    </div>

    <!-- 加载失败 -->
    <p v-if="error" class="dsv__error">{{ error }}</p>

    <!-- 主体：变量列表 + 数据表格 -->
    <div v-else class="dsv__body">
      <p v-if="loading" class="dsv__empty">{{ t('main.sidebar.data.loadingFile') }}</p>
      <template v-else-if="schema && preview">
        <div class="dsv__vars">
          <p class="dsv__vars-title">{{ t('main.sidebar.data.variable') }}</p>
          <button
            v-for="(v, i) in schema.variables"
            :key="v.name"
            class="dsv-var"
            :class="{ 'dsv-var--active': activeCol === i }"
            @click="activeCol = activeCol === i ? -1 : i"
          >
            <span class="dsv-var__name" :title="v.label || v.name">{{ v.name }}</span>
            <span class="dsv-var__badge">{{ typeBadge(v.data_type === 'Numeric') }}</span>
          </button>
        </div>

        <div class="dsv__grid">
          <table class="dsv-table">
            <thead>
              <tr>
                <th class="dsv-table__index">#</th>
                <th
                  v-for="(c, i) in preview.columns"
                  :key="c"
                  :class="{ 'dsv-table__cell--active': activeCol === i }"
                  :title="c"
                >{{ c }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="(row, r) in preview.rows" :key="r">
                <td class="dsv-table__index">{{ r + 1 }}</td>
                <td
                  v-for="(cell, c) in row"
                  :key="c"
                  :class="{ 'dsv-table__cell--active': activeCol === c }"
                >{{ cell }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </template>
    </div>

    <!-- 底部提示 -->
    <p v-if="schema && !error" class="dsv__foot">
      {{ t('main.sidebar.data.previewHint', { n: schema.n_rows }) }}
      <span v-if="preview?.truncated">{{ t('main.sidebar.data.previewTruncated') }}</span>
    </p>
  </div>
</template>

<style scoped>
.dsv {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 16px 20px;
  gap: 10px;
  overflow: hidden;
}

/* ── 头部 ── */
.dsv__head {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
}
.dsv__name {
  font-size: 16px;
  font-weight: 700;
  color: var(--fluen-ink);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.dsv__meta {
  font-size: 12px;
  color: var(--fluen-stone);
}
.dsv-tag {
  flex-shrink: 0;
  font-size: 12px;
  line-height: 1;
  padding: 5px 10px;
  border-radius: 9999px;
}
.dsv-tag--questionnaire {
  color: var(--fluen-success, #1a7f4e);
  background: color-mix(in srgb, var(--fluen-success, #1a7f4e) 12%, transparent);
}
.dsv-tag--experiment {
  color: var(--fluen-accent);
  background: color-mix(in srgb, var(--fluen-accent) 12%, transparent);
}

/* ── 主体 ── */
.dsv__body {
  flex: 1;
  min-height: 0;
  display: flex;
  gap: 14px;
}
.dsv__empty {
  margin: 0;
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 13px;
  color: var(--fluen-stone);
}
.dsv__error {
  margin: 0;
  flex: 1;
  font-size: 13px;
  color: var(--fluen-danger, #e8463a);
  word-break: break-all;
}

/* 变量列表 */
.dsv__vars {
  flex: 0 0 200px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.dsv__vars-title {
  margin: 0 0 6px 0;
  font-size: 12px;
  color: var(--fluen-stone);
}
.dsv-var {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  border: none;
  background: transparent;
  padding: 6px 10px;
  border-radius: 6px;
  cursor: pointer;
  text-align: left;
}
.dsv-var:hover {
  background: var(--fluen-hover);
}
.dsv-var--active {
  background: var(--fluen-hover);
  box-shadow: inset 2px 0 0 var(--fluen-accent);
}
.dsv-var__name {
  font-size: 13px;
  color: var(--fluen-ink);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.dsv-var__badge {
  flex-shrink: 0;
  font-size: 10px;
  line-height: 1;
  padding: 3px 5px;
  border-radius: 4px;
  border: 1px solid var(--fluen-hairline);
  color: var(--fluen-stone);
}

/* 数据表格 */
.dsv__grid {
  flex: 1;
  min-width: 0;
  overflow: auto;
  border: 1px solid var(--fluen-hairline);
  border-radius: 10px;
  background: var(--fluen-surface);
}
.dsv-table {
  border-collapse: separate;
  border-spacing: 0;
  font-size: 13px;
  min-width: 100%;
}
.dsv-table th,
.dsv-table td {
  padding: 7px 12px;
  text-align: left;
  white-space: nowrap;
  border-bottom: 1px solid var(--fluen-hairline);
  color: var(--fluen-ink);
  font-variant-numeric: tabular-nums;
}
.dsv-table thead th {
  position: sticky;
  top: 0;
  z-index: 1;
  background: var(--fluen-surface);
  font-weight: 600;
  color: var(--fluen-stone);
  border-bottom: 1px solid var(--fluen-hairline);
}
.dsv-table__index {
  position: sticky;
  left: 0;
  z-index: 2;
  background: var(--fluen-surface);
  color: var(--fluen-stone);
  text-align: right;
  min-width: 36px;
}
.dsv-table thead .dsv-table__index {
  z-index: 3;
}
.dsv-table__cell--active {
  background: var(--fluen-hover) !important;
  box-shadow: inset 0 0 0 1px var(--fluen-accent);
}

/* ── 底部提示 ── */
.dsv__foot {
  margin: 0;
  flex-shrink: 0;
  font-size: 12px;
  color: var(--fluen-stone);
  display: flex;
  gap: 8px;
}
</style>
