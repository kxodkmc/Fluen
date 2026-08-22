<script setup lang="ts">
/**
 * MotisActivityGroup — 助手活动分组（可折叠时间线块）。
 *
 * 将连续的思考与工具调用消息合并为一个块，布局参照「摘要行 + 时间线」结构：
 *   - 摘要行：按类别汇总工具执行次数（如「已读取 2 个文件 · 已检索文献 1 次」），
 *     点击折叠/展开整个时间线；仅含思考时显示「思考过程」
 *   - 时间线：每行「图标 + 动词 + 对象」，行间以竖向细线相连
 *   - 思考行：进行中显示点点动画，有内容时可展开查看原文
 *   - 工具行：未返回结果时显示运行态动画；参数与结果可展开查看
 *
 * active=true（生成进行至该组）时自动展开，结束后自动收起；
 * 用户手动切换后仍会跟随后续的 active 变化。
 */
import { ref, computed, watch, onMounted } from 'vue';
import { useI18n } from '../../../../i18n';
import { useMascotConfig } from '../../../../composables/useMascotConfig';
import type { ChatMessage } from '../../types';
import {
  describeToolCall,
  summarizeToolDisplays,
  type ToolCategory,
  type ToolDisplay,
} from './toolDescribe';

const props = defineProps<{
  /** 组内消息（连续的 thinking / tool_call）。 */
  items: ChatMessage[];
  /** 生成是否进行至该组（进行中自动展开、结束后收起）。 */
  active: boolean;
}>();

const { t } = useI18n();

/* ── 展开状态 ─────────────────────────────────────────────────────────── */
/** 整组是否展开（跟随 active 自动切换）。 */
const expanded = ref(props.active);

watch(
  () => props.active,
  (val) => {
    expanded.value = val;
  },
);

/** 切换整组展开/收起。 */
function toggleExpanded(): void {
  expanded.value = !expanded.value;
}

/* ── 行模型 ───────────────────────────────────────────────────────────── */
/** 时间线单行。 */
interface ActivityRow {
  id: string;
  kind: 'thinking' | 'tool';
  /** 原始消息（内容 / 参数 / 结果）。 */
  msg: ChatMessage;
  /** 工具行专属描述。 */
  display?: ToolDisplay;
}

const rows = computed<ActivityRow[]>(() =>
  props.items.map((msg) =>
    msg.kind === 'tool_call'
      ? { id: msg.id, kind: 'tool', msg, display: describeToolCall(msg) }
      : { id: msg.id, kind: 'thinking', msg },
  ),
);

/* ── 摘要文案 ─────────────────────────────────────────────────────────── */
/** 按类别聚合工具计数并翻译为摘要片段（保持出现顺序）。 */
const summaryParts = computed(() => {
  const displays = rows.value
    .filter((row) => row.kind === 'tool')
    .map((row) => row.display as ToolDisplay);
  return summarizeToolDisplays(displays).map((part) =>
    t(part.countKey, { count: part.count }),
  );
});

/** 头部摘要文本：有工具时展示计数片段，否则视为纯思考过程。 */
const summaryText = computed(() =>
  summaryParts.value.length > 0
    ? summaryParts.value.join(' · ')
    : t('main.motisPanel.activity.thinkingOnly'),
);

/* ── 行运行态 ─────────────────────────────────────────────────────────── */
/** 思考行是否仍在思考中。 */
function isThinkingActive(row: ActivityRow): boolean {
  return row.kind === 'thinking' && row.msg.isStreaming !== false;
}

/** 工具行是否仍在执行（生成中且尚未收到结果）。 */
function isToolRunning(row: ActivityRow): boolean {
  return row.kind === 'tool' && props.active && row.msg.toolResult === undefined;
}

/* ── 行详情展开 ───────────────────────────────────────────────────────── */
/** 是否展示思考原文（与 MascotConfig.show_thinking_content 一致）。 */
const showThinkingContent = ref(false);

onMounted(async () => {
  try {
    const config = await useMascotConfig().loadConfig();
    showThinkingContent.value = config.show_thinking_content;
  } catch {
    // 降级：不展示思考原文
  }
});

/** 各行详情展开状态（msg.id → 是否展开）。 */
const openedRows = ref<Record<string, boolean>>({});

/** 切换某行详情展开/收起。 */
function toggleRow(id: string): void {
  openedRows.value = { ...openedRows.value, [id]: !openedRows.value[id] };
}

/** 格式化详情值（字符串原样，其余 JSON 缩进两格）。 */
function formatValue(value: unknown): string {
  if (value === undefined || value === null) return '';
  if (typeof value === 'string') return value.trim();
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

/** 行的可展开详情文本（思考原文 / 参数+结果）。 */
function detailText(row: ActivityRow): string {
  if (row.kind === 'thinking') return row.msg.content;
  return [formatValue(row.msg.toolInput), formatValue(row.msg.toolResult)]
    .filter(Boolean)
    .join('\n\n');
}

/** 行是否有可展开详情。 */
function canExpand(row: ActivityRow): boolean {
  if (row.kind === 'thinking') {
    return showThinkingContent.value && row.msg.content.trim() !== '';
  }
  return detailText(row).trim() !== '';
}

/** 行对象文本（固定名词优先，其次原始文本）。 */
function objectTextOf(row: ActivityRow): string {
  if (row.kind !== 'tool' || !row.display) return '';
  if (row.display.objectKey) return t(row.display.objectKey);
  return row.display.objectText ?? '';
}

/* ── 图标（24×24 viewBox 描边路径） ────────────────────────────────────── */
const CATEGORY_ICON_PATHS: Record<ToolCategory | 'thinking', string[]> = {
  thinking: [
    'M12 3l1.9 5.7a2 2 0 0 0 1.4 1.4L21 12l-5.7 1.9a2 2 0 0 0-1.4 1.4L12 21l-1.9-5.7a2 2 0 0 0-1.4-1.4L3 12l5.7-1.9a2 2 0 0 0 1.4-1.4L12 3Z',
  ],
  read: [
    'M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z',
    'M14 2v6h6',
    'M16 13H8',
    'M16 17H8',
  ],
  write: ['M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z'],
  paper: [
    'M2 4h6a4 4 0 0 1 4 4v12a3 3 0 0 0-3-3H2Z',
    'M22 4h-6a4 4 0 0 0-4 4v12a3 3 0 0 1 3-3h7Z',
  ],
  search: ['M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16Z', 'm21 21-4.35-4.35'],
  manuscript: ['M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z', 'M12 20h9'],
  delegate: ['M5 12h14', 'm13 6 6 6-6 6'],
  generic: [
    'M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z',
  ],
};

/** 行图标路径集合。 */
function iconPathsOf(row: ActivityRow): string[] {
  return CATEGORY_ICON_PATHS[row.kind === 'thinking' ? 'thinking' : row.display!.category];
}
</script>

<template>
  <div class="motis-act">
    <!-- 摘要行（折叠开关） -->
    <button type="button" class="motis-act__header" @click="toggleExpanded">
      <span class="motis-act__summary">{{ summaryText }}</span>
      <svg
        class="motis-act__chevron"
        :class="{ 'motis-act__chevron--open': expanded }"
        viewBox="0 0 24 24"
        width="11"
        height="11"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
      >
        <path d="m6 9 6 6 6-6" />
      </svg>
    </button>

    <!-- 时间线 -->
    <div v-if="expanded" class="motis-act__body">
      <div v-for="row in rows" :key="row.id" class="motis-act__item">
        <div
          class="motis-act__row"
          :class="{ 'motis-act__row--expandable': canExpand(row) }"
          @click="canExpand(row) && toggleRow(row.id)"
        >
          <span class="motis-act__icon">
            <svg
              viewBox="0 0 24 24"
              width="11"
              height="11"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path v-for="(d, i) in iconPathsOf(row)" :key="i" :d="d" />
            </svg>
          </span>

          <!-- 思考行：思考中（动画）/ 已思考 -->
          <template v-if="row.kind === 'thinking'">
            <span class="motis-act__label">{{
              isThinkingActive(row)
                ? t('main.motisPanel.thinking')
                : t('main.motisPanel.thinkingDone')
            }}</span>
            <span v-if="isThinkingActive(row)" class="motis-act__dots">
              <span class="motis-act__dot" />
              <span class="motis-act__dot" />
              <span class="motis-act__dot" />
            </span>
          </template>

          <!-- 工具行：动词 + 对象 -->
          <template v-else>
            <span class="motis-act__label">{{ t(row.display!.verbKey) }}</span>
            <span v-if="objectTextOf(row)" class="motis-act__object">{{ objectTextOf(row) }}</span>
            <span v-if="isToolRunning(row)" class="motis-act__dots">
              <span class="motis-act__dot" />
              <span class="motis-act__dot" />
              <span class="motis-act__dot" />
            </span>
          </template>

          <!-- 可展开标记 -->
          <svg
            v-if="canExpand(row)"
            class="motis-act__row-chevron"
            :class="{ 'motis-act__row-chevron--open': openedRows[row.id] }"
            viewBox="0 0 24 24"
            width="10"
            height="10"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
          >
            <path d="m9 6 6 6-6 6" />
          </svg>
        </div>

        <!-- 展开详情（思考原文 / 参数与结果） -->
        <pre v-if="openedRows[row.id]" class="motis-act__detail">{{ detailText(row) }}</pre>
      </div>
    </div>
  </div>
</template>

<style scoped>
.motis-act {
  align-self: flex-start;
  display: flex;
  flex-direction: column;
  max-width: 94%;
  font-family: var(--fluen-font-sans);
}

/* ── 摘要行 ─────────────────────────────────────────────────────────── */
.motis-act__header {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  align-self: flex-start;
  border: none;
  background: transparent;
  padding: 2px 6px;
  border-radius: 6px;
  font-family: inherit;
  font-size: 12px;
  color: var(--fluen-slate);
  cursor: pointer;
  transition: background 0.15s ease;
}

.motis-act__header:hover {
  background: var(--fluen-hover);
}

.motis-act__summary {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 320px;
}

.motis-act__chevron {
  flex-shrink: 0;
  transition: transform 0.2s ease;
  transform: rotate(-90deg);
}

.motis-act__chevron--open {
  transform: rotate(0deg);
}

/* ── 时间线 ─────────────────────────────────────────────────────────── */
.motis-act__body {
  position: relative;
  margin-top: 2px;
  padding-left: 4px;
}

/* 竖向连接线：贯穿首尾行图标中心之间 */
.motis-act__body::before {
  content: '';
  position: absolute;
  left: 12px;
  top: 11px;
  bottom: 11px;
  width: 1px;
  background: var(--fluen-hairline);
}

.motis-act__item {
  position: relative;
}

.motis-act__row {
  display: flex;
  align-items: center;
  gap: 6px;
  min-height: 22px;
  padding: 2px 6px 2px 0;
  border-radius: 6px;
  min-width: 0;
}

.motis-act__row--expandable {
  cursor: pointer;
}

.motis-act__row--expandable:hover {
  background: var(--fluen-hover);
}

/* 图标底色遮住连接线，形成参考图中的分段效果 */
.motis-act__icon {
  position: relative;
  z-index: 1;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 17px;
  height: 17px;
  border-radius: 4px;
  background: var(--fluen-surface);
  color: var(--fluen-stone);
}

.motis-act__row--expandable:hover .motis-act__icon {
  background: var(--fluen-hover);
}

.motis-act__label {
  flex-shrink: 0;
  font-size: 12px;
  line-height: 1.4;
  color: var(--fluen-slate);
}

/* 对象文本（路径/检索词等）：弱化显示，超长省略 */
.motis-act__object {
  flex: 0 1 auto;
  min-width: 0;
  font-size: 12px;
  line-height: 1.4;
  color: var(--fluen-stone);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 240px;
}

.motis-act__row-chevron {
  flex-shrink: 0;
  color: var(--fluen-stone);
  opacity: 0.7;
  transition: transform 0.15s ease;
}

.motis-act__row-chevron--open {
  transform: rotate(90deg);
}

/* ── 运行态点点 ─────────────────────────────────────────────────────── */
.motis-act__dots {
  display: inline-flex;
  gap: 3px;
  flex-shrink: 0;
}

.motis-act__dot {
  width: 4px;
  height: 4px;
  border-radius: 50%;
  background: var(--fluen-stone);
  animation: motis-act-bounce 1.4s infinite ease-in-out;
}

.motis-act__dot:nth-child(2) {
  animation-delay: 0.2s;
}

.motis-act__dot:nth-child(3) {
  animation-delay: 0.4s;
}

@keyframes motis-act-bounce {
  0%,
  60%,
  100% {
    opacity: 0.3;
    transform: scale(0.8);
  }
  30% {
    opacity: 1;
    transform: scale(1);
  }
}

/* ── 行详情 ─────────────────────────────────────────────────────────── */
.motis-act__detail {
  margin: 2px 0 4px 27px;
  padding: 6px 10px;
  border-radius: 8px;
  background: var(--fluen-surface);
  border: 1px solid var(--fluen-hairline);
  font-family: var(--fluen-font-mono);
  font-size: 11px;
  line-height: 1.55;
  color: var(--fluen-slate);
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 220px;
  overflow-y: auto;
}
</style>
