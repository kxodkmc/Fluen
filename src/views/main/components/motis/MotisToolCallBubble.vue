<script setup lang="ts">
/**
 * MotisToolCallBubble — Motis 工具调用气泡。
 *
 * 展示工具名称与执行状态文案。文案风格依据配置切换：
 *   - professional_expression=false → 拟人化文案（随机选取）
 *   - professional_expression=true  → 专业化文案
 *
 * 配置来源：useMascotConfig.professional_expression
 */
import { ref, onMounted, computed } from 'vue';
import { useI18n } from '../../../../i18n';
import { useMascotConfig } from '../../../../composables/useMascotConfig';
import {
  PLAYFUL_TOOL_MESSAGE_KEYS,
  PROFESSIONAL_TOOL_MESSAGE_KEY,
  getRandomMessageKey,
} from '../../constants';

const props = defineProps<{
  /** 工具名称。 */
  toolName?: string;
  /** 工具调用输入参数（展示为可折叠 JSON）。 */
  input?: unknown;
  /** 工具调用执行结果（展示为可折叠内容）。 */
  result?: unknown;
}>();

const { t } = useI18n();

/** 状态文案（mount 时确定，保持稳定）。 */
const statusText = ref('');
/** 输入参数是否展开。 */
const inputExpanded = ref(false);
/** 执行结果是否展开。 */
const resultExpanded = ref(false);

onMounted(async () => {
  try {
    const { loadConfig } = useMascotConfig();
    const config = await loadConfig();
    statusText.value = config.professional_expression
      ? t(PROFESSIONAL_TOOL_MESSAGE_KEY)
      : t(getRandomMessageKey(PLAYFUL_TOOL_MESSAGE_KEYS));
  } catch {
    statusText.value = t(getRandomMessageKey(PLAYFUL_TOOL_MESSAGE_KEYS));
  }
});

/** 输入参数格式化后的展示文本。 */
const inputText = computed(() => {
  if (props.input === undefined || props.input === null) return '';
  return JSON.stringify(props.input, null, 2);
});

/** 是否存在可展示的输入参数。 */
const hasInput = computed(() => inputText.value.trim() !== '');

/** 执行结果格式化后的展示文本。 */
const resultText = computed(() => {
  if (props.result === undefined || props.result === null) return '';
  if (typeof props.result === 'string') return props.result.trim();
  return JSON.stringify(props.result, null, 2);
});

/** 是否存在可展示的执行结果。 */
const hasResult = computed(() => resultText.value.trim() !== '');

/** 切换输入参数展开/折叠。 */
function toggleInput(): void {
  inputExpanded.value = !inputExpanded.value;
}

/** 切换执行结果展开/折叠。 */
function toggleResult(): void {
  resultExpanded.value = !resultExpanded.value;
}
</script>

<template>
  <div class="motis-tool-call">
    <div class="motis-tool-call__icon">
      <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z" />
      </svg>
    </div>
    <div class="motis-tool-call__body">
      <div class="motis-tool-call__row">
        <span v-if="toolName" class="motis-tool-call__name">{{ toolName }}</span>
        <span class="motis-tool-call__status">{{ statusText }}</span>
      </div>
      <!-- 输入参数：可展开查看 JSON -->
      <button
        v-if="hasInput"
        class="motis-tool-call__toggle"
        :class="{ 'motis-tool-call__toggle--expanded': inputExpanded }"
        @click="toggleInput"
      >
        <svg viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <path d="m6 9 6 6 6-6" />
        </svg>
        <span class="motis-tool-call__toggle-label">{{ t('main.motisPanel.toolParams') }}</span>
      </button>
      <pre v-if="hasInput && inputExpanded" class="motis-tool-call__params">{{ inputText }}</pre>
      <!-- 执行结果：可展开查看 -->
      <button
        v-if="hasResult"
        class="motis-tool-call__toggle"
        :class="{ 'motis-tool-call__toggle--expanded': resultExpanded }"
        @click="toggleResult"
      >
        <svg viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <path d="m6 9 6 6 6-6" />
        </svg>
        <span class="motis-tool-call__toggle-label">{{ t('main.motisPanel.toolResult') }}</span>
      </button>
      <pre v-if="hasResult && resultExpanded" class="motis-tool-call__params">{{ resultText }}</pre>
    </div>
  </div>
</template>

<style scoped>
.motis-tool-call {
  align-self: flex-start;
  display: flex;
  align-items: center;
  gap: 8px;
  max-width: 90%;
  padding: 6px 12px;
  border-radius: 14px;
  background: var(--fluen-surface);
  border: 1px solid var(--fluen-hairline);
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  color: var(--fluen-slate);
}

.motis-tool-call__icon {
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--fluen-info-bg);
  color: var(--fluen-info);
}

.motis-tool-call__body {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.motis-tool-call__row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.motis-tool-call__name {
  font-weight: 600;
  color: var(--fluen-ink);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.motis-tool-call__status {
  color: var(--fluen-stone);
}

/* 输入参数展开开关 */
.motis-tool-call__toggle {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  align-self: flex-start;
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  font-family: var(--fluen-font-sans);
  font-size: 11px;
  cursor: pointer;
  padding: 2px 4px;
  border-radius: 4px;
  transition: background 0.15s ease;
}

.motis-tool-call__toggle:hover {
  background: var(--fluen-hover);
}

.motis-tool-call__toggle svg {
  transition: transform 0.2s ease;
}

.motis-tool-call__toggle--expanded svg {
  transform: rotate(180deg);
}

.motis-tool-call__toggle-label {
  white-space: nowrap;
}

/* 输入参数内容（JSON） */
.motis-tool-call__params {
  margin: 4px 0 0;
  padding: 6px 8px;
  border-radius: 8px;
  background: var(--fluen-surface);
  border: 1px solid var(--fluen-hairline);
  font-family: var(--fluen-font-mono);
  font-size: 11px;
  line-height: 1.5;
  color: var(--fluen-slate);
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 200px;
  overflow-y: auto;
}
</style>
