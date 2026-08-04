<template>
  <div class="marks-panel">
    <!-- 头部 -->
    <div class="marks-header">
      <span class="marks-title">{{ t('reader.marks.title') }}</span>
      <span class="marks-count">{{ filteredMarks.length }}</span>
    </div>

    <!-- 颜色筛选 -->
    <div class="marks-filter">
      <button
        class="filter-btn"
        :class="{ active: activeFilter === null }"
        :title="t('reader.marks.filterAll')"
        @click="activeFilter = null"
      >
        {{ t('reader.marks.filterAll') }}
      </button>
      <button
        v-for="c in MARK_COLORS"
        :key="c"
        class="filter-btn filter-color"
        :class="['color-' + c, { active: activeFilter === c }]"
        :title="t('reader.marks.colorMap.' + c)"
        @click="activeFilter = c"
      ></button>
    </div>

    <!-- 加载中 -->
    <div v-if="state.loading" class="marks-status">
      <div class="status-spinner"></div>
    </div>

    <!-- 错误 -->
    <div v-else-if="state.error" class="marks-status marks-error">
      <span>{{ state.error }}</span>
    </div>

    <!-- 空列表 -->
    <div v-else-if="filteredMarks.length === 0" class="marks-empty">
      <span>{{ t('reader.marks.empty') }}</span>
    </div>

    <!-- 列表 -->
    <ul v-else class="marks-list">
      <li
        v-for="mark in filteredMarks"
        :key="mark.id"
        class="mark-item"
        :class="{ 'is-orphaned': mark.status === 'orphaned' }"
      >
        <!-- 颜色条 + 文本（点击跳转） -->
        <div class="mark-main" @click="emit('scroll-to-mark', mark.id)">
          <span class="mark-color" :class="'color-' + mark.color"></span>
          <span class="mark-text" :title="mark.text">{{ mark.text }}</span>
        </div>

        <!-- 颜色选择 + 删除 -->
        <div class="mark-actions">
          <div class="color-picker">
            <button
              v-for="c in MARK_COLORS"
              :key="c"
              class="color-dot"
              :class="['color-' + c, { active: mark.color === c }]"
              :title="t('reader.marks.colorMap.' + c)"
              @click.stop="onColorChange(mark, c)"
            ></button>
          </div>
          <button
            class="mark-delete"
            :title="t('reader.marks.delete')"
            @click.stop="onDelete(mark)"
          >
            ×
          </button>
        </div>

        <!-- 附注 -->
        <textarea
          class="mark-note"
          :value="mark.note ?? ''"
          :placeholder="t('reader.marks.notePlaceholder')"
          rows="2"
          @blur="onNoteBlur(mark, $event)"
        ></textarea>
      </li>
    </ul>
  </div>
</template>

<script setup lang="ts">
/**
 * MarksPanel —— 标记列表面板（右侧抽屉）。
 *
 * 职责：
 *   - 展示当前文献的全部标记（颜色 + 文本摘要 + 附注）
 *   - 点击标记 → emit `scroll-to-mark`（父层转发到 iframe 滚动高亮）
 *   - 编辑附注 → 调用 useReferenceMarks.updateMark
 *   - 切换颜色 → 调用 useReferenceMarks.updateMark
 *   - 删除 → 调用 useReferenceMarks.deleteMark + emit `remove-mark`
 *   - 按颜色筛选
 *
 * 数据来源：useReferenceMarks 模块级单例状态。
 */

import { computed, ref } from 'vue';
import { useI18n } from '../../../../i18n';
import { useReferenceMarks } from '../../../../composables/useReferenceMarks';
import { MARK_COLORS, type Mark, type MarkColor } from '../../../../types/marks';

const props = defineProps<{
  /** 文献 ID（用于 CRUD 命令）。 */
  referenceId: string;
}>();

const emit = defineEmits<{
  'scroll-to-mark': [markId: string];
  'remove-mark': [markId: string];
}>();

const { t } = useI18n();
const { state, updateMark, deleteMark } = useReferenceMarks();

const activeFilter = ref<MarkColor | null>(null);

const filteredMarks = computed(() =>
  activeFilter.value === null
    ? state.marks
    : state.marks.filter((m) => m.color === activeFilter.value),
);

async function onColorChange(mark: Mark, color: MarkColor): Promise<void> {
  if (mark.color === color) return;
  await updateMark(props.referenceId, mark.id, mark.note ?? null, color);
}

async function onNoteBlur(mark: Mark, e: Event): Promise<void> {
  const value = (e.target as HTMLTextAreaElement).value;
  const next = value.trim() === '' ? null : value;
  if ((mark.note ?? null) === next) return;
  await updateMark(props.referenceId, mark.id, next, mark.color);
}

async function onDelete(mark: Mark): Promise<void> {
  const ok = await deleteMark(props.referenceId, mark.id);
  if (ok) emit('remove-mark', mark.id);
}
</script>

<style scoped>
.marks-panel {
  display: flex;
  flex-direction: column;
  width: 280px;
  height: 100%;
  background: var(--app-bg, #ffffff);
  border-left: 1px solid var(--app-border, #e5e7eb);
  flex-shrink: 0;
  overflow: hidden;
}

.marks-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid var(--app-border, #e5e7eb);
  flex-shrink: 0;
}

.marks-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--app-text, #1a1a2e);
}

.marks-count {
  font-size: 12px;
  color: var(--app-text-muted, #6b7280);
  background: var(--app-bg-alt, #f3f4f6);
  padding: 1px 8px;
  border-radius: 10px;
  min-width: 22px;
  text-align: center;
}

/* ── 筛选 ── */
.marks-filter {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 16px;
  border-bottom: 1px solid var(--app-border, #e5e7eb);
  flex-shrink: 0;
}

.filter-btn {
  height: 22px;
  padding: 0 10px;
  border: 1px solid var(--app-border, #e5e7eb);
  border-radius: 11px;
  background: var(--app-bg, #ffffff);
  color: var(--app-text-muted, #6b7280);
  font-size: 11px;
  cursor: pointer;
  transition: all 0.15s;
}

.filter-btn:hover {
  border-color: var(--app-primary, #2563eb);
}

.filter-btn.active {
  background: var(--app-primary, #2563eb);
  border-color: var(--app-primary, #2563eb);
  color: #ffffff;
}

.filter-color {
  width: 22px;
  padding: 0;
}

.filter-color.color-yellow { background: rgba(250, 204, 21, 0.6); }
.filter-color.color-green { background: rgba(34, 197, 94, 0.5); }
.filter-color.color-blue { background: rgba(59, 130, 246, 0.5); }
.filter-color.color-pink { background: rgba(236, 72, 153, 0.5); }
.filter-color.active { box-shadow: 0 0 0 2px var(--app-primary, #2563eb); }

/* ── 状态 ── */
.marks-status {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  color: var(--app-text-muted, #6b7280);
  font-size: 13px;
}

.marks-error {
  color: #b91c1c;
}

.status-spinner {
  width: 24px;
  height: 24px;
  border: 2px solid var(--app-border, #e5e7eb);
  border-top-color: var(--app-primary, #2563eb);
  border-radius: 50%;
  animation: marks-spin 0.8s linear infinite;
}

@keyframes marks-spin {
  to { transform: rotate(360deg); }
}

.marks-empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  color: var(--app-text-muted, #6b7280);
  font-size: 13px;
  text-align: center;
}

/* ── 列表 ── */
.marks-list {
  flex: 1;
  margin: 0;
  padding: 4px 0;
  list-style: none;
  overflow-y: auto;
}

.mark-item {
  padding: 8px 12px;
  border-bottom: 1px solid var(--app-border, #e5e7eb);
}

.mark-item.is-orphaned {
  opacity: 0.55;
}

.mark-main {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  cursor: pointer;
  margin-bottom: 4px;
}

.mark-main:hover .mark-text {
  color: var(--app-primary, #2563eb);
}

.mark-color {
  flex-shrink: 0;
  width: 3px;
  align-self: stretch;
  min-height: 16px;
  border-radius: 2px;
  margin-top: 2px;
}

.mark-color.color-yellow { background: rgba(250, 204, 21, 0.8); }
.mark-color.color-green { background: rgba(34, 197, 94, 0.7); }
.mark-color.color-blue { background: rgba(59, 130, 246, 0.7); }
.mark-color.color-pink { background: rgba(236, 72, 153, 0.7); }

.mark-text {
  flex: 1;
  font-size: 12px;
  line-height: 1.5;
  color: var(--app-text, #1a1a2e);
  word-break: break-word;
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
  transition: color 0.15s;
}

/* ── 操作行 ── */
.mark-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  margin-bottom: 4px;
}

.color-picker {
  display: flex;
  gap: 4px;
}

.color-dot {
  width: 14px;
  height: 14px;
  padding: 0;
  border: 1px solid transparent;
  border-radius: 50%;
  cursor: pointer;
  transition: all 0.15s;
}

.color-dot.color-yellow { background: rgba(250, 204, 21, 0.8); }
.color-dot.color-green { background: rgba(34, 197, 94, 0.7); }
.color-dot.color-blue { background: rgba(59, 130, 246, 0.7); }
.color-dot.color-pink { background: rgba(236, 72, 153, 0.7); }

.color-dot:hover {
  transform: scale(1.15);
}

.color-dot.active {
  border-color: var(--app-text, #1a1a2e);
  box-shadow: 0 0 0 2px var(--app-bg, #ffffff);
}

.mark-delete {
  width: 22px;
  height: 22px;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--app-text-muted, #6b7280);
  font-size: 16px;
  line-height: 1;
  cursor: pointer;
  border-radius: 4px;
  transition: all 0.15s;
}

.mark-delete:hover {
  background: #fee2e2;
  color: #b91c1c;
}

/* ── 附注 ── */
.mark-note {
  width: 100%;
  padding: 4px 6px;
  border: 1px solid var(--app-border, #e5e7eb);
  border-radius: 4px;
  background: var(--app-bg-alt, #f9fafb);
  color: var(--app-text, #1a1a2e);
  font-size: 11px;
  line-height: 1.4;
  font-family: inherit;
  resize: none;
  transition: border-color 0.15s;
}

.mark-note:focus {
  outline: none;
  border-color: var(--app-primary, #2563eb);
  background: var(--app-bg, #ffffff);
}

.mark-note::placeholder {
  color: var(--app-text-muted, #9ca3af);
}
</style>
