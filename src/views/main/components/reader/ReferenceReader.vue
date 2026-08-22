<template>
  <div class="reference-reader">
    <!-- 加载中 -->
    <div v-if="state.loading" class="reader-status">
      <div class="status-spinner"></div>
      <span>{{ t('reader.loading') }}</span>
    </div>

    <!-- 错误 -->
    <div v-else-if="state.error" class="reader-status reader-error">
      <span class="error-icon">⚠</span>
      <span>{{ state.error }}</span>
    </div>

    <!-- 正常渲染 -->
    <template v-else-if="state.content">
      <ReferenceToolbar
        :title="state.content.meta.title"
        :options="state.options"
        :marks-active="showMarksPanel"
        @font-size-change="setFontSize"
        @line-height-change="setLineHeight"
        @marks-toggle="toggleMarksPanel"
      />
      <div class="reader-body">
        <ReferenceContent
          ref="contentRef"
          :html="state.html"
          :options="state.options"
          :title="state.content.meta.title"
          @selection-create="onSelectionCreate"
          @mark-click="onMarkClick"
          @marks-ready="onMarksReady"
        />
        <MarksPanel
          v-if="showMarksPanel"
          :reference-id="referenceId"
          @scroll-to-mark="onScrollToMark"
          @remove-mark="onRemoveMark"
        />
      </div>

      <!-- 颜色选择浮层 -->
      <div
        v-if="pendingSelection"
        class="color-picker-popover"
        :style="popoverStyle"
      >
        <button
          v-for="c in MARK_COLORS"
          :key="c"
          class="picker-dot"
          :class="'color-' + c"
          :title="t('reader.marks.colorMap.' + c)"
          @click="onPickColor(c)"
        ></button>
        <button class="picker-cancel" @click="pendingSelection = null">×</button>
      </div>

      <!-- 跨块选区提示 -->
      <div v-if="crossBlockHint" class="cross-block-toast">
        {{ t('reader.marks.crossBlock') }}
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
/**
 * ReferenceReader —— 文献阅读器主容器。
 *
 * 组合 ReferenceToolbar + ReferenceContent + MarksPanel，
 * 通过 useReferenceReader 加载文献、useReferenceMarks 管理标记。
 *
 * 标记流程：
 *   1. 用户在 iframe 内划线 → ReferenceContent 上报 selection:create
 *   2. 本组件查 BlockMap 构造 MarkAnchor → 弹出颜色选择浮层
 *   3. 用户选色 → createMark → renderMarks 下发高亮
 *   4. 点击高亮 / 面板条目 → scrollToMark 滚动定位
 *
 * 生命周期：
 *   - referenceId 变化 → loadReference + loadMarks
 *   - onBeforeUnmount → clear（阅读器 + 标记状态）
 */

import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from '../../../../i18n';
import { useReferenceReader } from '../../../../composables/useReferenceReader';
import { useReferenceMarks } from '../../../../composables/useReferenceMarks';
import { MARK_COLORS } from '../../../../types/marks';
import type { Mark, MarkAnchor, MarkColor } from '../../../../types/marks';
import type { BlockKey } from '../../../../types/reader';
import type { SerializedMark, SerializedAnchor, SelectionRect } from './marks';
import ReferenceToolbar from './ReferenceToolbar.vue';
import ReferenceContent from './ReferenceContent.vue';
import MarksPanel from './MarksPanel.vue';

const props = defineProps<{
  /** 文献 ID。 */
  referenceId: string;
}>();

const { t } = useI18n();
const { state, loadReference, clear, setFontSize, setLineHeight } =
  useReferenceReader();
const { state: marksState, loadMarks, createMark, clear: marksClear } =
  useReferenceMarks();

const showMarksPanel = ref(false);
const contentRef = ref<InstanceType<typeof ReferenceContent> | null>(null);
/** 颜色选择浮层 pending 数据（含已构造的 MarkAnchor）。 */
const pendingSelection = ref<{
  markAnchor: MarkAnchor;
  text: string;
  rect?: SelectionRect;
} | null>(null);
/** 跨块选区提示。 */
const crossBlockHint = ref(false);

// ── marks 序列化 ──

function blockKeyToString(bk: BlockKey): string {
  return `${bk.block_type}:${bk.source_line}:${bk.occurrence}`;
}

function serializeMarks(marks: readonly Mark[]): SerializedMark[] {
  return marks.map((m) => ({
    id: m.id,
    block_key: blockKeyToString(m.anchor.block_key),
    start_offset: m.anchor.range.start_offset,
    end_offset: m.anchor.range.end_offset,
    text: m.text,
    color: m.color,
    status: m.status,
  }));
}

// ── 浮层定位 ──

const popoverStyle = computed(() => {
  const r = pendingSelection.value?.rect;
  if (!r) return {};
  return {
    left: `${r.left + r.width / 2}px`,
    top: `${r.bottom + 8}px`,
  };
});

// ── 事件处理 ──

/** 划线选区上报：构造 MarkAnchor 并弹出颜色选择浮层。 */
function onSelectionCreate(payload: {
  anchor: SerializedAnchor;
  text: string;
  rect?: SelectionRect;
}): void {
  // 跨块选区：block_key 为空字符串 → 提示不支持
  if (payload.anchor.block_key === '') {
    crossBlockHint.value = true;
    setTimeout(() => {
      crossBlockHint.value = false;
    }, 2500);
    return;
  }
  const entry = state.blockMap?.byKey.get(payload.anchor.block_key);
  if (!entry) return;
  const markAnchor: MarkAnchor = {
    block_key: {
      block_type: entry.block_type,
      source_line: entry.source_line,
      occurrence: entry.occurrence,
    },
    block_fingerprint: entry.fingerprint,
    range: {
      start_offset: payload.anchor.start_offset,
      end_offset: payload.anchor.end_offset,
    },
  };
  pendingSelection.value = { markAnchor, text: payload.text, rect: payload.rect };
}

/** 选色创建标记。 */
async function onPickColor(color: MarkColor): Promise<void> {
  const sel = pendingSelection.value;
  if (!sel) return;
  pendingSelection.value = null;
  const created = await createMark(props.referenceId, sel.markAnchor, sel.text, color);
  if (created) {
    contentRef.value?.renderMarks(serializeMarks(marksState.marks));
  }
}

/** 点击高亮 → 滚动定位。 */
function onMarkClick(markId: string): void {
  contentRef.value?.scrollToMark(markId);
}

/** iframe 就绪：下发当前 marks。 */
function onMarksReady(): void {
  if (marksState.marks.length) {
    contentRef.value?.renderMarks(serializeMarks(marksState.marks));
  }
}

/** 面板：点击条目滚动。 */
function onScrollToMark(markId: string): void {
  contentRef.value?.scrollToMark(markId);
}

/** 面板：删除后移除高亮。 */
function onRemoveMark(markId: string): void {
  contentRef.value?.removeMark(markId);
}

function toggleMarksPanel(): void {
  showMarksPanel.value = !showMarksPanel.value;
}

// ── 生命周期与监听 ──

// referenceId 变化时加载文献与标记
watch(
  () => props.referenceId,
  (id) => {
    if (id) {
      void loadReference(id);
      void loadMarks(id);
    }
  },
  { immediate: true },
);

// HTML 重新渲染后（iframe 重载），延迟下发 marks（缓冲至 marks:ready）
watch(
  () => state.html,
  () => {
    setTimeout(() => {
      if (marksState.marks.length) {
        contentRef.value?.renderMarks(serializeMarks(marksState.marks));
      }
    }, 250);
  },
);

// marks 变化（增删改）→ 重新渲染高亮
watch(
  () => marksState.marks,
  () => {
    contentRef.value?.renderMarks(serializeMarks(marksState.marks));
  },
  { deep: true },
);

onBeforeUnmount(() => {
  clear();
  marksClear();
});
</script>

<style scoped>
.reference-reader {
  position: relative;
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  background: var(--app-bg, #ffffff);
}

.reader-body {
  flex: 1;
  display: flex;
  min-height: 0;
}

.reader-status {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 16px;
  color: var(--app-text-muted, #6b7280);
  font-size: 14px;
}

.reader-error {
  color: #b91c1c;
}

.error-icon {
  font-size: 32px;
}

.status-spinner {
  width: 32px;
  height: 32px;
  border: 3px solid var(--app-border, #e5e7eb);
  border-top-color: var(--app-primary, #2563eb);
  border-radius: 50%;
  animation: reader-spin 0.8s linear infinite;
}

@keyframes reader-spin {
  to {
    transform: rotate(360deg);
  }
}

/* ── 颜色选择浮层 ── */
.color-picker-popover {
  position: absolute;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 8px;
  background: var(--app-bg, #ffffff);
  border: 1px solid var(--app-border, #e5e7eb);
  border-radius: 8px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  transform: translateX(-50%);
  z-index: 10;
}

.picker-dot {
  width: 18px;
  height: 18px;
  padding: 0;
  border: 1px solid transparent;
  border-radius: 50%;
  cursor: pointer;
  transition: transform 0.15s;
}

.picker-dot:hover {
  transform: scale(1.15);
}

.picker-dot.color-yellow { background: rgba(250, 204, 21, 0.8); }
.picker-dot.color-green { background: rgba(34, 197, 94, 0.7); }
.picker-dot.color-blue { background: rgba(59, 130, 246, 0.7); }
.picker-dot.color-pink { background: rgba(236, 72, 153, 0.7); }

.picker-cancel {
  width: 20px;
  height: 20px;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--app-text-muted, #6b7280);
  font-size: 16px;
  line-height: 1;
  cursor: pointer;
  border-radius: 4px;
}

.picker-cancel:hover {
  background: var(--app-bg-alt, #f3f4f6);
  color: var(--app-text, #1a1a2e);
}

/* ── 跨块提示 ── */
.cross-block-toast {
  position: absolute;
  bottom: 24px;
  left: 50%;
  transform: translateX(-50%);
  padding: 8px 16px;
  background: var(--app-text, #1a1a2e);
  color: var(--app-bg, #ffffff);
  font-size: 12px;
  border-radius: 6px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
  z-index: 10;
  pointer-events: none;
}
</style>
