<script setup lang="ts">
/**
 * TableGridPicker — Word 式表格尺寸选择面板。
 *
 * 上半部为悬停网格（默认 8 行 × 10 列）：鼠标扫过高亮 r×c 范围并显示
 * 尺寸，点击即以默认 Markdown 语法插入；下半部为自定义行列与语法形态
 * （Markdown 默认 / HTML 拓展，HTML 表可改写为合并单元格结构，
 * 见《论文标记规范》§5.3 形态 B/C）。
 *
 * 本组件仅负责选择交互，插入动作由宿主（EditorToolbar）执行——
 * 通过 `insert` 事件上交 (rows, cols, syntax)，保持工具与命令解耦。
 */
import { ref } from 'vue';
import { useI18n } from '../../../../../i18n';
import type { TableSyntax } from '../codemirror/tableModel';

withDefaults(defineProps<{ maxRows?: number; maxCols?: number }>(), {
  maxRows: 8,
  maxCols: 10,
});

const emit = defineEmits<{ insert: [rows: number, cols: number, syntax: TableSyntax] }>();

const { t } = useI18n();

/** 自定义行列的收敛上限（网格选择器不受此限，由 maxRows/maxCols 决定）。 */
const ROWS_LIMIT = 30;
const COLS_LIMIT = 20;

/** 悬停范围（1-based 行列数）；离开网格时清空。 */
const hover = ref<{ rows: number; cols: number } | null>(null);
const customRows = ref(3);
const customCols = ref(3);
const syntax = ref<TableSyntax>('markdown');

function onGridEnter(rows: number, cols: number): void {
  hover.value = { rows, cols };
}

function onGridLeave(): void {
  hover.value = null;
}

/** 网格点击：默认 Markdown 语法（最常用路径，零额外输入）。 */
function pickFromGrid(rows: number, cols: number): void {
  emit('insert', rows, cols, 'markdown');
}

/** 自定义插入：行列数收敛到合法区间后提交。 */
function insertCustom(): void {
  const rows = Math.min(ROWS_LIMIT, Math.max(1, Math.floor(customRows.value || 1)));
  const cols = Math.min(COLS_LIMIT, Math.max(1, Math.floor(customCols.value || 1)));
  emit('insert', rows, cols, syntax.value);
}
</script>

<template>
  <div class="table-grid-picker">
    <div class="table-grid-picker__head">
      <span class="table-grid-picker__title">{{ t('main.content.toolbar.insertTable') }}</span>
      <span v-if="hover" class="table-grid-picker__size">{{ hover.cols }} × {{ hover.rows }}</span>
    </div>

    <div
      class="table-grid-picker__grid"
      role="grid"
      :aria-label="t('main.content.toolbar.insertTable')"
      @mouseleave="onGridLeave"
    >
      <template v-for="r in maxRows" :key="r">
        <div
          v-for="c in maxCols"
          :key="c"
          class="table-grid-picker__cell"
          :class="{ 'table-grid-picker__cell--active': hover !== null && r <= hover.rows && c <= hover.cols }"
          role="gridcell"
          @mouseenter="onGridEnter(r, c)"
          @click="pickFromGrid(r, c)"
        ></div>
      </template>
    </div>

    <div class="table-grid-picker__custom">
      <div class="table-grid-picker__row">
        <label class="table-grid-picker__field">
          <span>{{ t('main.content.toolbar.rows') }}</span>
          <input
            v-model.number="customRows"
            class="table-grid-picker__input"
            type="number"
            min="1"
            :max="ROWS_LIMIT"
          />
        </label>
        <label class="table-grid-picker__field">
          <span>{{ t('main.content.toolbar.cols') }}</span>
          <input
            v-model.number="customCols"
            class="table-grid-picker__input"
            type="number"
            min="1"
            :max="COLS_LIMIT"
          />
        </label>
      </div>
      <div class="table-grid-picker__row">
        <select v-model="syntax" class="table-grid-picker__select" :aria-label="t('main.content.toolbar.tableType')">
          <option value="markdown">{{ t('main.content.toolbar.syntaxMarkdown') }}</option>
          <option value="html">{{ t('main.content.toolbar.syntaxHtml') }}</option>
        </select>
        <button class="table-grid-picker__btn" type="button" @click="insertCustom">
          {{ t('main.content.toolbar.insert') }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.table-grid-picker {
  width: 214px;
  padding: 10px;
}

.table-grid-picker__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
}

.table-grid-picker__title {
  color: var(--fluen-slate);
  font-size: 12px;
  font-weight: 600;
}

.table-grid-picker__size {
  color: var(--fluen-stone);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}

/* ── 悬停网格 ─────────────────────────────────────────────────────── */
.table-grid-picker__grid {
  display: grid;
  grid-template-columns: repeat(v-bind(maxCols), 17px);
  gap: 2px;
  padding-bottom: 10px;
  margin-bottom: 10px;
  border-bottom: 1px solid var(--fluen-hairline);
}

.table-grid-picker__cell {
  width: 17px;
  height: 17px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 3px;
  background: var(--fluen-surface-soft);
  cursor: pointer;
}

.table-grid-picker__cell--active {
  background: var(--fluen-hover);
  border-color: var(--fluen-accent);
}

/* ── 自定义插入 ───────────────────────────────────────────────────── */
.table-grid-picker__custom {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.table-grid-picker__row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.table-grid-picker__field {
  display: flex;
  align-items: center;
  gap: 4px;
  color: var(--fluen-slate);
  font-size: 12px;
}

.table-grid-picker__input,
.table-grid-picker__select {
  height: 24px;
  padding: 0 6px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 6px;
  background: var(--fluen-surface);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 12px;
}

.table-grid-picker__input {
  width: 52px;
}

.table-grid-picker__select {
  flex: 1;
  min-width: 0;
  cursor: pointer;
}

.table-grid-picker__btn {
  height: 24px;
  padding: 0 12px;
  border: none;
  border-radius: 6px;
  background: var(--fluen-accent);
  color: var(--fluen-on-accent);
  font-size: 12px;
  cursor: pointer;
  transition: background 0.15s ease;
}

.table-grid-picker__btn:hover {
  background: var(--fluen-accent-hover);
}

.table-grid-picker__btn:focus-visible,
.table-grid-picker__input:focus-visible,
.table-grid-picker__select:focus-visible {
  outline: 2px solid var(--fluen-accent);
  outline-offset: 1px;
}
</style>
