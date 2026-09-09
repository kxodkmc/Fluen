<script setup lang="ts">
/**
 * MotisContextUsage — 「上下文容量」面板。
 *
 * 数据源为 useMotisChat 缓存的两类信息：
 * - `contextUsage`：后端每轮发送前推送的分类估算报告
 *   （`motis:context-usage`，与 `context_usage.rs` 对齐）
 * - `lastActualUsage`：`motis:finish` 返回的真实 token 用量（vendor 上报时）
 *
 * 收起态为头部小按钮（显示占用百分比），点击展开下拉面板：
 * 进度条（输入估算 + 输出预留 vs 上下文窗口）+ 分类占比列表
 * （占比 = 分类 tokens / 展示总量，为 0 的类别隐藏）。
 */
import { computed, inject, onBeforeUnmount, ref, watch } from 'vue';
import { MOTIS_CHAT_KEY } from './symbols';
import { useMotisChat } from '../../composables/useMotisChat';
import { useI18n } from '../../../../i18n';

const { t, locale } = useI18n();
const motisChat = inject(MOTIS_CHAT_KEY, () => useMotisChat(), true);
const { contextUsage, lastActualUsage } = motisChat;

/** 面板展开状态。 */
const open = ref(false);
/** 组件根元素（外部点击判定）。 */
const root = ref<HTMLElement | null>(null);

const report = computed(() => contextUsage.value);

/** 展示总量 = 输入估算 + 输出预留（分类占比的分母）。 */
const displayTotal = computed(() => {
  const r = report.value;
  return r ? r.estimatedPromptTokens + r.maxOutputTokens : 0;
});

/** 占总窗口的百分比（0-100，可超 100 时钳制）。 */
const usedPercent = computed(() => {
  const r = report.value;
  if (!r || r.contextWindow <= 0) return 0;
  return Math.min(100, (displayTotal.value / r.contextWindow) * 100);
});

/** 输入估算段宽（%）。 */
const promptBarPercent = computed(() => {
  const r = report.value;
  if (!r || r.contextWindow <= 0) return 0;
  return Math.min(100, (r.estimatedPromptTokens / r.contextWindow) * 100);
});

/** 输出预留段宽（%）。 */
const outputBarPercent = computed(() => {
  const r = report.value;
  if (!r || r.contextWindow <= 0) return 0;
  return Math.min(100 - promptBarPercent.value, (r.maxOutputTokens / r.contextWindow) * 100);
});

/** 分类行（隐藏 0 值类别，占比相对展示总量）。 */
const rows = computed(() => {
  const r = report.value;
  if (!r || displayTotal.value <= 0) return [];
  return r.categories
    .filter((c) => c.tokens > 0)
    .map((c) => ({
      key: c.key,
      label: categoryLabel(c.key),
      tokens: c.tokens,
      percent: (c.tokens / displayTotal.value) * 100,
    }));
});

/** 分类 key → i18n 文案（key 与后端 context_usage.rs 对齐）。 */
function categoryLabel(key: string): string {
  const map: Record<string, string> = {
    messages: t('main.contextUsage.categories.messages'),
    system_prompt: t('main.contextUsage.categories.systemPrompt'),
    sub_agents: t('main.contextUsage.categories.subAgents'),
    board: t('main.contextUsage.categories.board'),
    tools: t('main.contextUsage.categories.tools'),
    output_reserved: t('main.contextUsage.categories.outputReserved'),
  };
  return map[key] ?? key;
}

/** token 数格式化：zh 用「万」，其余用 K/M。 */
function formatTokens(v: number): string {
  const trim = (n: number, unit: string, base: number) =>
    `${(n / base).toFixed(1).replace(/\.0$/, '')}${unit}`;
  if (locale.value === 'zh-CN') {
    return v >= 10000 ? trim(v, '万', 10000) : String(v);
  }
  if (v >= 1000000) return trim(v, 'M', 1000000);
  if (v >= 1000) return trim(v, 'K', 1000);
  return String(v);
}

/** 收起态按钮文案（如 `12.5%`）。 */
const triggerLabel = computed(() => `${usedPercent.value.toFixed(1)}%`);

/** 头部数字行（如 `6.2万/50万 (12.5%)`）。 */
const summaryLabel = computed(() => {
  const r = report.value;
  if (!r) return '';
  return `${formatTokens(displayTotal.value)}/${formatTokens(r.contextWindow)} (${usedPercent.value.toFixed(1)}%)`;
});

/** 真实用量文案（vendor 上报时展示）。 */
const actualLabel = computed(() => {
  const a = lastActualUsage.value;
  if (!a) return '';
  return t('main.contextUsage.actualDetail', {
    prompt: formatTokens(a.promptTokens),
    completion: formatTokens(a.completionTokens),
  });
});

function toggle(): void {
  if (!report.value) return;
  open.value = !open.value;
}

/* ── 外部点击关闭 ───────────────────────────────────────────────────── */
function onDocClick(e: MouseEvent): void {
  if (!open.value) return;
  const target = e.target as Node | null;
  if (target && root.value && !root.value.contains(target)) {
    open.value = false;
  }
}

watch(open, (expanded) => {
  if (expanded) {
    document.addEventListener('click', onDocClick, true);
  } else {
    document.removeEventListener('click', onDocClick, true);
  }
});

onBeforeUnmount(() => {
  document.removeEventListener('click', onDocClick, true);
});
</script>

<template>
  <div ref="root" class="ctx-usage">
    <!-- 收起态触发按钮 -->
    <button
      class="ctx-usage__trigger"
      :class="{ 'ctx-usage__trigger--active': open }"
      :disabled="!report"
      :title="report ? t('main.contextUsage.trigger') : t('main.contextUsage.empty')"
      @click="toggle"
    >
      <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
        <path d="M12 20a8 8 0 1 1 8-8" />
        <path d="M12 12l4.5-4.5" />
      </svg>
      <span v-if="report" class="ctx-usage__trigger-label">{{ triggerLabel }}</span>
    </button>

    <!-- 展开面板 -->
    <div v-if="open && report" class="ctx-usage__panel">
      <div class="ctx-usage__head">
        <span class="ctx-usage__title">{{ t('main.contextUsage.title') }}</span>
        <span class="ctx-usage__summary">{{ summaryLabel }}</span>
      </div>

      <!-- 进度条：输入估算（实色） + 输出预留（浅色） -->
      <div class="ctx-usage__bar">
        <div class="ctx-usage__bar-prompt" :style="{ width: `${promptBarPercent}%` }" />
        <div class="ctx-usage__bar-output" :style="{ width: `${outputBarPercent}%` }" />
      </div>

      <!-- 分类占比列表 -->
      <ul class="ctx-usage__list">
        <li v-for="row in rows" :key="row.key" class="ctx-usage__item">
          <span class="ctx-usage__dot" />
          <span class="ctx-usage__item-label">{{ row.label }}</span>
          <span class="ctx-usage__item-tokens">{{ formatTokens(row.tokens) }}</span>
          <span class="ctx-usage__item-percent">{{ row.percent.toFixed(1) }}%</span>
        </li>
      </ul>

      <!-- 真实用量 / 估算口径说明 -->
      <div class="ctx-usage__foot">
        <span v-if="actualLabel" class="ctx-usage__actual">
          {{ t('main.contextUsage.actual') }}：{{ actualLabel }}
        </span>
        <span v-else class="ctx-usage__hint">{{ t('main.contextUsage.estimated') }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ctx-usage {
  position: relative;
  flex-shrink: 0;
}

.ctx-usage__trigger {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 28px;
  padding: 0 8px;
  border: none;
  background: transparent;
  color: var(--fluen-slate);
  cursor: pointer;
  border-radius: 8px;
  font-family: var(--fluen-font-mono);
  font-size: 11px;
  transition: background 0.15s ease, color 0.15s ease;
}

.ctx-usage__trigger:hover:not(:disabled) {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.ctx-usage__trigger:disabled {
  opacity: 0.45;
  cursor: default;
}

.ctx-usage__trigger--active {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.ctx-usage__trigger-label {
  line-height: 1;
}

.ctx-usage__panel {
  position: absolute;
  top: calc(100% + 6px);
  right: 0;
  z-index: 30;
  width: 264px;
  padding: 12px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 12px;
  background: var(--fluen-surface);
  box-shadow: var(--fluen-shadow-md, 0 8px 24px rgba(0, 0, 0, 0.12));
}

.ctx-usage__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 8px;
}

.ctx-usage__title {
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  font-weight: 600;
  color: var(--fluen-ink);
}

.ctx-usage__summary {
  font-family: var(--fluen-font-mono);
  font-size: 11px;
  color: var(--fluen-slate);
  white-space: nowrap;
}

.ctx-usage__bar {
  position: relative;
  display: flex;
  height: 6px;
  border-radius: 9999px;
  background: var(--fluen-canvas);
  overflow: hidden;
  margin-bottom: 10px;
}

.ctx-usage__bar-prompt {
  height: 100%;
  background: var(--fluen-accent);
  border-radius: 9999px 0 0 9999px;
}

.ctx-usage__bar-output {
  height: 100%;
  background: var(--fluen-accent);
  opacity: 0.35;
}

.ctx-usage__list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.ctx-usage__item {
  display: flex;
  align-items: center;
  gap: 6px;
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  color: var(--fluen-ink);
}

.ctx-usage__dot {
  width: 6px;
  height: 6px;
  flex-shrink: 0;
  border-radius: 50%;
  background: var(--fluen-accent);
}

.ctx-usage__item-label {
  flex: 1;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.ctx-usage__item-tokens {
  font-family: var(--fluen-font-mono);
  font-size: 11px;
  color: var(--fluen-slate);
}

.ctx-usage__item-percent {
  font-family: var(--fluen-font-mono);
  font-size: 11px;
  color: var(--fluen-slate);
  min-width: 42px;
  text-align: right;
}

.ctx-usage__foot {
  margin-top: 10px;
  padding-top: 8px;
  border-top: 1px solid var(--fluen-hairline);
}

.ctx-usage__actual,
.ctx-usage__hint {
  font-family: var(--fluen-font-sans);
  font-size: 11px;
  color: var(--fluen-slate);
}
</style>
