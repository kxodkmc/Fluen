<script setup lang="ts">
/**
 * DataTypeChoice — 数据类型选择步骤。
 *
 * 导入 CSV 后让用户确认数据用途（问卷 / 实验），
 * 选择结果决定分析工作台推荐的分析方法组合。
 */
import { ref } from 'vue';
import { useI18n } from '../../../../../../i18n';
import type { DatasetKind } from '../../../../../../types/dataAnalysis';

const emit = defineEmits<{ (e: 'confirm', kind: DatasetKind): void }>();
const { t } = useI18n();

const picked = ref<DatasetKind | null>(null);

const kinds: { id: DatasetKind; icon: 'clipboard' | 'flask' }[] = [
  { id: 'questionnaire', icon: 'clipboard' },
  { id: 'experiment', icon: 'flask' },
];

function confirm(): void {
  if (picked.value) emit('confirm', picked.value);
}
</script>

<template>
  <div class="dtc">
    <div class="dtc__head">
      <p class="dtc__title">{{ t('main.sidebar.data.chooseType') }}</p>
      <p class="dtc__hint">{{ t('main.sidebar.data.chooseTypeHint') }}</p>
    </div>

    <div class="dtc__cards">
      <button
        v-for="k in kinds"
        :key="k.id"
        class="dtc-card"
        :class="{ 'dtc-card--active': picked === k.id }"
        @click="picked = k.id"
      >
        <span class="dtc-card__icon">
          <!-- 问卷 -->
          <svg v-if="k.icon === 'clipboard'" viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <rect x="5" y="4" width="14" height="17" rx="2" />
            <path d="M9 4.5V3h6v1.5" />
            <path d="M8.5 10h7M8.5 14h7M8.5 18h4" />
          </svg>
          <!-- 实验 -->
          <svg v-else viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M10 3h4M11 3v6.5L5.5 18a2.4 2.4 0 0 0 2.1 3.5h8.8a2.4 2.4 0 0 0 2.1-3.5L13 9.5V3" />
            <path d="M7.5 15h9" />
          </svg>
        </span>
        <span class="dtc-card__name">{{ t(`main.sidebar.data.type_${k.id}`) }}</span>
        <span class="dtc-card__desc">{{ t(`main.sidebar.data.type_${k.id}_desc`) }}</span>
      </button>
    </div>

    <button class="dtc__confirm" :disabled="!picked" @click="confirm">
      {{ t('main.sidebar.data.confirmType') }}
    </button>
  </div>
</template>

<style scoped>
.dtc {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 8px 2px;
}
.dtc__title {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--fluen-ink);
}
.dtc__hint {
  margin: 4px 0 0 0;
  font-size: 12px;
  line-height: 1.5;
  color: var(--fluen-stone);
}
.dtc__cards {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.dtc-card {
  display: grid;
  grid-template-columns: auto 1fr;
  grid-template-rows: auto auto;
  column-gap: 10px;
  align-items: center;
  text-align: left;
  padding: 10px 12px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 10px;
  background: var(--fluen-surface);
  cursor: pointer;
  transition: border-color 0.15s ease, background 0.15s ease, box-shadow 0.15s ease;
}
.dtc-card:hover {
  border-color: var(--fluen-accent);
  background: var(--fluen-hover);
}
.dtc-card--active {
  border-color: var(--fluen-accent);
  box-shadow: 0 0 0 1px var(--fluen-accent) inset;
  background: var(--fluen-hover);
}
.dtc-card__icon {
  grid-row: 1 / span 2;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 34px;
  height: 34px;
  border-radius: 8px;
  background: var(--fluen-hover);
  color: var(--fluen-accent);
}
.dtc-card__name {
  font-size: 13px;
  font-weight: 600;
  color: var(--fluen-ink);
}
.dtc-card__desc {
  font-size: 11px;
  color: var(--fluen-stone);
  margin-top: 2px;
}
.dtc__confirm {
  border: none;
  border-radius: 8px;
  padding: 8px 0;
  font-size: 13px;
  font-weight: 600;
  background: var(--fluen-ink);
  color: var(--fluen-on-primary, #fff);
  cursor: pointer;
  transition: opacity 0.15s ease;
}
.dtc__confirm:hover:not(:disabled) {
  opacity: 0.88;
}
.dtc__confirm:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
