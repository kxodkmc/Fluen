<script setup lang="ts">
/**
 * MotisThinkingIndicator — Motis 尾部思考指示器（扁平行样式）。
 *
 * 与活动时间线行保持一致的紧凑布局：图标 + 文案 + 点点动画。
 *   - `active=true`（思考进行中）：显示"思考中…"文字 + 点点点动画。
 *   - `active=false`（思考已结束）：不再显示动画，改显示"已思考"，
 *   点击展开按钮可查看折叠的思考增量内容。
 * - 当 show_thinking_content 配置为 true 且传入 content 时，展示可折叠的思考内容。
 *
 * 配置来源：useMascotConfig.show_thinking_content
 */
import { ref, onMounted } from 'vue';
import { useI18n } from '../../../../i18n';
import { useMascotConfig } from '../../../../composables/useMascotConfig';

const props = defineProps<{
  /** 思考内容增量（show_thinking_content=true 时展示）。 */
  content?: string;
  /** 是否仍在思考中（false 表示思考已结束，隐藏动画）。 */
  active?: boolean;
}>();

const { t } = useI18n();
const { loadConfig } = useMascotConfig();

/** 是否仍在思考进行中（缺省视为进行中，兼容尾部指示器等无入参场景）。 */
const isActive = props.active !== false;

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
    <!-- 扁平行：图标 + 文案 + 点点动画 -->
    <div class="motis-thinking__row">
      <span class="motis-thinking__icon">
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
          <path
            d="M12 3l1.9 5.7a2 2 0 0 0 1.4 1.4L21 12l-5.7 1.9a2 2 0 0 0-1.4 1.4L12 21l-1.9-5.7a2 2 0 0 0-1.4-1.4L3 12l5.7-1.9a2 2 0 0 0 1.4-1.4L12 3Z"
          />
        </svg>
      </span>
      <span class="motis-thinking__text">{{
        isActive ? t('main.motisPanel.thinking') : t('main.motisPanel.thinkingDone')
      }}</span>
      <span v-if="isActive" class="motis-thinking__dots">
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
          width="10"
          height="10"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          :class="{ 'motis-thinking__chevron--expanded': expanded }"
        >
          <path d="m9 6 6 6-6 6" />
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
  max-width: 94%;
  font-family: var(--fluen-font-sans);
}

.motis-thinking__row {
  display: flex;
  align-items: center;
  gap: 6px;
  min-height: 22px;
  padding: 2px 0;
}

.motis-thinking__icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 17px;
  height: 17px;
  border-radius: 4px;
  color: var(--fluen-stone);
}

.motis-thinking__text {
  font-size: 12px;
  line-height: 1.4;
  color: var(--fluen-slate);
}

.motis-thinking__dots {
  display: inline-flex;
  gap: 3px;
}

.motis-thinking__dot {
  width: 4px;
  height: 4px;
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

.motis-thinking__toggle {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
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
  transition: transform 0.15s ease;
}

.motis-thinking__chevron--expanded {
  transform: rotate(90deg);
}

.motis-thinking__content {
  margin: 2px 0 4px 23px;
  padding: 6px 10px;
  border-radius: 8px;
  background: var(--fluen-surface);
  border: 1px solid var(--fluen-hairline);
  font-size: 12px;
  line-height: 1.6;
  color: var(--fluen-slate);
  white-space: pre-wrap;
  word-break: break-word;
}
</style>
