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
import { ref, onMounted } from 'vue';
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
}>();

const { t } = useI18n();

/** 状态文案（mount 时确定，保持稳定）。 */
const statusText = ref('');

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
</script>

<template>
  <div class="motis-tool-call">
    <div class="motis-tool-call__icon">
      <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z" />
      </svg>
    </div>
    <div class="motis-tool-call__body">
      <span v-if="toolName" class="motis-tool-call__name">{{ toolName }}</span>
      <span class="motis-tool-call__status">{{ statusText }}</span>
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
</style>
