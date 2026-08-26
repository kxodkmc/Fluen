<script setup lang="ts">
/**
 * MotisAgentRun — 子智能体委派运行面板。
 *
 * 挂在 `delegate_agent` 工具调用行的展开详情处（由
 * MotisActivityGroup 渲染），展示子智能体委派的实时运行状态：
 *
 *   - 头部：目标智能体名 + 任务描述 + 状态（运行中点点动画 /
 *     完成耗时与 token / 失败原因）
 *   - 思考过程 / 实时输出：子智能体 LLM 增量流（agent-thought /
 *     agent-text 事件累积，流式追加并自动贴底）
 *   - 活动列表：子智能体内部工具调用（名称 + 状态 + 耗时），
 *     行可展开查看参数与结果详情
 *
 * 数据由 `motis:agent-*` 事件流维护（见 useMotisChat），组件纯展示。
 */
import { computed, nextTick, ref, watch } from 'vue';
import { useI18n } from '../../../../i18n';
import type { AgentRun } from '../../types';

const props = defineProps<{
  /** 委派运行记录（useMotisChat 按 tool_call_id 维护）。 */
  run: AgentRun;
}>();

const { t } = useI18n();

/** 智能体显示名 i18n key（settings.motis.agents.*，与设置页一致）。 */
const AGENT_NAME_KEYS: Record<string, string> = {
  essay_writing: 'settings.motis.agents.essayWriting',
  essay_review: 'settings.motis.agents.essayReview',
  essay_critique: 'settings.motis.agents.essayCritique',
  knowledge_builder: 'settings.motis.agents.knowledgeBuilder',
  data_analyst: 'settings.motis.agents.dataAnalyst',
};

const agentName = computed(() => {
  const key = AGENT_NAME_KEYS[props.run.agentId];
  return key ? t(key) : props.run.agentId;
});

const isRunning = computed(() => props.run.status === 'running');
const isFailed = computed(() => props.run.status === 'failed');

/** 格式化耗时（毫秒 → 保留一位小数的秒）。 */
function formatDuration(ms?: number): string {
  if (ms === undefined) return '';
  return t('main.motisPanel.agentRun.durationSec', { s: (ms / 1000).toFixed(1) });
}

/** 完成态摘要：耗时（成功时附 token 用量）。 */
const doneSummary = computed(() => {
  const parts = [formatDuration(props.run.durationMs)];
  if (props.run.tokensUsed !== undefined) {
    parts.push(t('main.motisPanel.agentRun.tokens', { n: props.run.tokensUsed }));
  }
  return parts.filter(Boolean).join(' · ');
});

/* ── 思考 / 实时输出流（增量追加时贴底） ─────────────────────────────── */

/** 思考与输出的滚动容器引用。 */
const thoughtEl = ref<HTMLElement | null>(null);
const textEl = ref<HTMLElement | null>(null);

watch(
  () => [props.run.thought?.length ?? 0, props.run.text?.length ?? 0],
  async () => {
    await nextTick();
    if (thoughtEl.value) thoughtEl.value.scrollTop = thoughtEl.value.scrollHeight;
    if (textEl.value) textEl.value.scrollTop = textEl.value.scrollHeight;
  },
);

/* ── 活动详情展开 ─────────────────────────────────────────────────────── */

/** 各活动详情展开状态（活动 id → 是否展开）。 */
const openedActivities = ref<Record<string, boolean>>({});

/** 切换某活动详情展开/收起。 */
function toggleActivity(id: string): void {
  openedActivities.value = { ...openedActivities.value, [id]: !openedActivities.value[id] };
}

/** 活动是否有可展开详情（结束后必有结果，运行中仅参数）。 */
function canExpand(activityIndex: number): boolean {
  const a = props.run.activities[activityIndex];
  return a.input !== undefined || a.result !== undefined;
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

/** 活动的可展开详情文本（参数 + 结果）。 */
function detailText(activityIndex: number): string {
  const a = props.run.activities[activityIndex];
  return [formatValue(a.input), formatValue(a.result)].filter(Boolean).join('\n\n');
}
</script>

<template>
  <div class="agent-run">
    <!-- 头部：智能体名 + 状态 -->
    <div class="agent-run__header">
      <span class="agent-run__name">{{ agentName }}</span>

      <template v-if="isRunning">
        <span class="agent-run__dots">
          <span class="agent-run__dot" />
          <span class="agent-run__dot" />
          <span class="agent-run__dot" />
        </span>
        <span class="agent-run__status">{{ t('main.motisPanel.agentRun.statusRunning') }}</span>
      </template>
      <template v-else-if="isFailed">
        <span class="agent-run__status agent-run__status--failed">
          {{ t('main.motisPanel.agentRun.statusFailed') }}
        </span>
      </template>
      <template v-else>
        <span class="agent-run__status agent-run__status--done">
          {{ t('main.motisPanel.agentRun.statusDone') }}
        </span>
        <span v-if="doneSummary" class="agent-run__meta">{{ doneSummary }}</span>
      </template>
    </div>

    <!-- 任务描述（超长省略，悬停看全文） -->
    <div class="agent-run__task" :title="run.task">{{ run.task }}</div>

    <!-- 失败原因 -->
    <div v-if="isFailed && run.error" class="agent-run__error">{{ run.error }}</div>

    <!-- 思考过程（依配置收集，流式追加） -->
    <div v-if="run.thought" class="agent-run__stream">
      <div class="agent-run__stream-label">{{ t('main.motisPanel.agentRun.thought') }}</div>
      <pre ref="thoughtEl" class="agent-run__stream-body">{{ run.thought }}</pre>
    </div>

    <!-- 实时输出 -->
    <div v-if="run.text" class="agent-run__stream">
      <div class="agent-run__stream-label">{{ t('main.motisPanel.agentRun.output') }}</div>
      <pre ref="textEl" class="agent-run__stream-body agent-run__stream-body--text">{{ run.text }}</pre>
    </div>

    <!-- 内部工具调用活动列表 -->
    <ul v-if="run.activities.length > 0" class="agent-run__activities">
      <li
        v-for="(activity, i) in run.activities"
        :key="activity.id"
        class="agent-run__act"
        :class="{ 'agent-run__act--expandable': canExpand(i) }"
        @click="canExpand(i) && toggleActivity(activity.id)"
      >
        <div class="agent-run__act-row">
          <span class="agent-run__act-name">{{ activity.name }}</span>
          <span v-if="activity.running" class="agent-run__dots">
            <span class="agent-run__dot" />
            <span class="agent-run__dot" />
            <span class="agent-run__dot" />
          </span>
          <template v-else>
            <span class="agent-run__act-mark" :class="activity.ok ? 'agent-run__act-mark--ok' : 'agent-run__act-mark--fail'">
              {{ activity.ok ? '✓' : '✕' }}
            </span>
            <span v-if="activity.durationMs !== undefined" class="agent-run__act-duration">
              {{ formatDuration(activity.durationMs) }}
            </span>
          </template>
        </div>
        <pre v-if="openedActivities[activity.id]" class="agent-run__detail">{{ detailText(i) }}</pre>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.agent-run {
  display: flex;
  flex-direction: column;
  gap: 3px;
  font-family: var(--fluen-font-sans);
}

/* ── 头部 ───────────────────────────────────────────────────────────── */
.agent-run__header {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}

.agent-run__name {
  font-size: 12px;
  font-weight: 600;
  color: var(--fluen-slate);
}

.agent-run__status {
  font-size: 11px;
  color: var(--fluen-stone);
}

.agent-run__status--done {
  color: var(--fluen-success-text);
}

.agent-run__status--failed {
  color: var(--fluen-error);
}

.agent-run__meta {
  font-size: 11px;
  color: var(--fluen-stone);
}

/* ── 任务与错误 ──────────────────────────────────────────────────────── */
.agent-run__task {
  font-size: 11px;
  line-height: 1.5;
  color: var(--fluen-stone);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.agent-run__error {
  font-size: 11px;
  line-height: 1.5;
  color: var(--fluen-error);
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 80px;
  overflow-y: auto;
}

/* ── 思考 / 实时输出流 ─────────────────────────────────────────────── */
.agent-run__stream {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.agent-run__stream-label {
  font-size: 10px;
  color: var(--fluen-muted);
}

.agent-run__stream-body {
  margin: 0;
  padding: 5px 9px;
  border-radius: 8px;
  background: var(--fluen-surface);
  border: 1px solid var(--fluen-hairline);
  font-family: var(--fluen-font-mono);
  font-size: 10px;
  line-height: 1.55;
  color: var(--fluen-stone);
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 140px;
  overflow-y: auto;
}

.agent-run__stream-body--text {
  color: var(--fluen-slate);
}

/* ── 活动列表 ────────────────────────────────────────────────────────── */
.agent-run__activities {
  margin: 2px 0 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.agent-run__act {
  border-radius: 6px;
}

.agent-run__act--expandable {
  cursor: pointer;
}

.agent-run__act--expandable:hover {
  background: var(--fluen-hover);
}

.agent-run__act-row {
  display: flex;
  align-items: center;
  gap: 6px;
  min-height: 20px;
  padding: 1px 6px;
  border-radius: 6px;
}

.agent-run__act-name {
  flex: 0 1 auto;
  min-width: 0;
  font-family: var(--fluen-font-mono);
  font-size: 11px;
  color: var(--fluen-slate);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 200px;
}

.agent-run__act-mark {
  flex-shrink: 0;
  font-size: 10px;
  line-height: 1;
}

.agent-run__act-mark--ok {
  color: var(--fluen-success-text);
}

.agent-run__act-mark--fail {
  color: var(--fluen-error);
}

.agent-run__act-duration {
  flex-shrink: 0;
  font-size: 10px;
  color: var(--fluen-muted);
}

/* ── 运行态点点 ──────────────────────────────────────────────────────── */
.agent-run__dots {
  display: inline-flex;
  gap: 3px;
  flex-shrink: 0;
}

.agent-run__dot {
  width: 3px;
  height: 3px;
  border-radius: 50%;
  background: var(--fluen-stone);
  animation: agent-run-bounce 1.4s infinite ease-in-out;
}

.agent-run__dot:nth-child(2) {
  animation-delay: 0.2s;
}

.agent-run__dot:nth-child(3) {
  animation-delay: 0.4s;
}

@keyframes agent-run-bounce {
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

/* ── 活动详情 ────────────────────────────────────────────────────────── */
.agent-run__detail {
  margin: 2px 0 3px;
  padding: 5px 9px;
  border-radius: 8px;
  background: var(--fluen-surface);
  border: 1px solid var(--fluen-hairline);
  font-family: var(--fluen-font-mono);
  font-size: 10px;
  line-height: 1.55;
  color: var(--fluen-slate);
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 180px;
  overflow-y: auto;
}
</style>
