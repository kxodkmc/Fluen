<script setup lang="ts">
/**
 * MotisChatHeader — Motis 对话面板头部。
 *
 * 展示 Motis 头像（圆形笑脸）、名称、心情徽章，以及关闭按钮。
 * 头像与名称从 useMascotConfig 读取，心情从 useMascotData 读取。
 *
 * @emits close - 关闭按钮点击时触发（由父组件收起右侧面板）
 */
import { ref, onMounted, computed } from 'vue';
import { useI18n } from '../../../../i18n';
import { useMascotConfig } from '../../../../composables/useMascotConfig';
import { useMascotData } from '../../../../composables/useMascotData';
import type { Mood } from '../../../../types/mascot';
import MotisContextUsage from './MotisContextUsage.vue';

defineEmits<{
  (e: 'close'): void;
}>();

const { t } = useI18n();
const { loadConfig } = useMascotConfig();
const { loadData } = useMascotData();

/** Motis 名称（默认 'Motis'）。 */
const name = ref('Motis');
/** 当前心情。 */
const mood = ref<Mood>('neutral');

onMounted(async () => {
  try {
    const [config, data] = await Promise.all([loadConfig(), loadData()]);
    name.value = config.name || 'Motis';
    mood.value = data.mood;
  } catch {
    // 降级使用默认值
  }
});

/** 心情徽章文案。 */
const moodLabel = computed(() => {
  switch (mood.value) {
    case 'happy':
      return t('main.motisPanel.moodHappy');
    case 'sad':
      return t('main.motisPanel.moodSad');
    default:
      return t('main.motisPanel.moodNeutral');
  }
});
</script>

<template>
  <div class="motis-header">
    <!-- 头像 + 名称 + 心情 -->
    <div class="motis-header__info">
      <div class="motis-header__avatar">
        <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <circle cx="12" cy="12" r="10" />
          <path d="M8 14s1.5 2 4 2 4-2 4-2" />
          <line x1="9" y1="9" x2="9.01" y2="9" />
          <line x1="15" y1="9" x2="15.01" y2="9" />
        </svg>
      </div>
      <span class="motis-header__name">{{ name }}</span>
      <span class="motis-header__mood" :class="`motis-header__mood--${mood}`">
        {{ moodLabel }}
      </span>
    </div>

    <!-- 右侧操作区：上下文容量 + 关闭 -->
    <div class="motis-header__actions">
      <MotisContextUsage />
      <button
        class="motis-header__close"
        :title="t('main.titleBar.controls.close')"
        @click="$emit('close')"
      >
        <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <path d="M18 6L6 18M6 6l12 12" />
        </svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
.motis-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 8px 0 12px;
  height: 44px;
  flex-shrink: 0;
  border-bottom: 1px solid var(--fluen-hairline);
  background: var(--fluen-surface);
}

.motis-header__info {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.motis-header__avatar {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  flex-shrink: 0;
  border-radius: 50%;
  background: var(--fluen-brand-coral);
  color: var(--fluen-on-dark);
}

.motis-header__name {
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  font-weight: 600;
  color: var(--fluen-ink);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.motis-header__mood {
  padding: 2px 8px;
  border-radius: 9999px;
  font-family: var(--fluen-font-sans);
  font-size: 11px;
  font-weight: 500;
  line-height: 1.4;
  flex-shrink: 0;
  border: 1px solid var(--fluen-hairline);
  background: var(--fluen-canvas);
  color: var(--fluen-slate);
}

.motis-header__mood--happy {
  background: var(--fluen-success-bg);
  color: var(--fluen-success-text);
  border-color: transparent;
}

.motis-header__mood--sad {
  background: var(--fluen-info-bg);
  color: var(--fluen-info);
  border-color: transparent;
}

.motis-header__actions {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
}

.motis-header__close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  flex-shrink: 0;
  border: none;
  background: transparent;
  color: var(--fluen-slate);
  cursor: pointer;
  border-radius: 8px;
  transition: background 0.15s ease, color 0.15s ease;
}

.motis-header__close:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}
</style>
