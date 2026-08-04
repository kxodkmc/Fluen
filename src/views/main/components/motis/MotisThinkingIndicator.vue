<script setup lang="ts">
/**
 * MotisThinkingIndicator — Motis 思考指示器。
 *
 * 默认显示"思考中…"文字 + 点点点动画。
 * 当 show_thinking_content 配置为 true 且传入 content 时，
 * 额外展示可折叠的思考增量内容。
 *
 * 配置来源：useMascotConfig.show_thinking_content
 */
import { ref, onMounted } from 'vue';
import { useI18n } from '../../../../i18n';
import { useMascotConfig } from '../../../../composables/useMascotConfig';

const props = defineProps<{
  /** 思考内容增量（show_thinking_content=true 时展示）。 */
  content?: string;
}>();

const { t } = useI18n();
const { loadConfig } = useMascotConfig();

/** 是否展示详细思考内容。 */
const showThinkingContent = ref(false);
/** 思考内容是否展开。 */
const expanded = ref(false);

onMounted(async () => {
  try {
    const config = await loadConfig();
    showThinkingContent.value = config.show_thinking_content;
  } catch {
    // 降级：不展示详细内容
  }
});

/** 切换思考内容展开/折叠。 */
function toggleExpanded(): void {
  expanded.value = !expanded.value;
}
</script>

<template>
  <div class="motis-thinking">
    <!-- 思考中动画 -->
    <div class="motis-thinking__indicator">
      <span class="motis-thinking__text">{{ t('main.motisPanel.thinking') }}</span>
      <span class="motis-thinking__dots">
        <span class="motis-thinking__dot" />
        <span class="motis-thinking__dot" />
        <span class="motis-thinking__dot" />
      </span>
      <!-- 可折叠的展开按钮 -->
      <button
        v-if="showThinkingContent && content"
        class="motis-thinking__toggle"
        @click="toggleExpanded"
      >
        <svg
          viewBox="0 0 24 24"
          width="12"
          height="12"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          :class="{ 'motis-thinking__chevron--expanded': expanded }"
        >
          <path d="m6 9 6 6 6-6" />
        </svg>
      </button>
    </div>

    <!-- 思考内容（展开时显示） -->
    <div v-if="showThinkingContent && content && expanded" class="motis-thinking__content">
      {{ content }}
    </div>
  </div>
</template>

<style scoped>
.motis-thinking {
  align-self: flex-start;
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-width: 90%;
}

.motis-thinking__indicator {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 14px;
  border-radius: 14px;
  border-bottom-left-radius: 4px;
  background: var(--fluen-canvas);
  border: 1px solid var(--fluen-hairline);
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  color: var(--fluen-slate);
}

.motis-thinking__dots {
  display: inline-flex;
  gap: 3px;
}

.motis-thinking__dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--fluen-stone);
  animation: motis-thinking-bounce 1.4s infinite ease-in-out;
}

.motis-thinking__dot:nth-child(2) {
  animation-delay: 0.2s;
}

.motis-thinking__dot:nth-child(3) {
  animation-delay: 0.4s;
}

@keyframes motis-thinking-bounce {
  0%, 60%, 100% { opacity: 0.3; transform: scale(0.8); }
  30% { opacity: 1; transform: scale(1); }
}

.motis-thinking__toggle {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  margin-left: 2px;
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
  border-radius: 4px;
  transition: background 0.15s ease;
}

.motis-thinking__toggle:hover {
  background: var(--fluen-hover);
}

.motis-thinking__toggle svg {
  transition: transform 0.2s ease;
}

.motis-thinking__chevron--expanded {
  transform: rotate(180deg);
}

.motis-thinking__content {
  padding: 8px 12px;
  border-radius: 14px;
  background: var(--fluen-surface);
  border: 1px solid var(--fluen-hairline);
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  line-height: 1.6;
  color: var(--fluen-slate);
  white-space: pre-wrap;
  word-break: break-word;
}
</style>
