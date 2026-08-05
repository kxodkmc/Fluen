<script setup lang="ts">
/**
 * GeneralSection — 通用设置分区。
 *
 * 当前包含：
 * - 最近打开项目显示数量（1-8，默认 4）
 *   控制欢迎页"最近打开"区域展示的项目数量上限。
 *
 * 状态由 `useAppConfig` 管理，变更后立即持久化。
 * 用户调小数量后，调用 `useRecentProjects.trim` 同步裁剪存储列表。
 */
import { ref, onMounted } from 'vue';
import { useI18n } from '../../../i18n';
import { useAppConfig } from '../../../composables/useAppConfig';
import { useRecentProjects } from '../../../composables/useRecentProjects';

const { t } = useI18n();
const { loadConfig, saveConfig } = useAppConfig();
const { trim } = useRecentProjects();

/** 最近打开项目数量最小值。 */
const MIN_COUNT = 1;
/** 最近打开项目数量最大值。 */
const MAX_COUNT = 8;

/** 当前数量（本地状态，保存到 AppConfig）。 */
const count = ref(4);
/** 是否正在保存。 */
const saving = ref(false);

onMounted(async () => {
  const config = await loadConfig();
  count.value = clamp(config.recent_projects_count);
});

/** 将数值限制在合法范围内。 */
function clamp(value: number): number {
  if (Number.isNaN(value)) return 4;
  return Math.min(MAX_COUNT, Math.max(MIN_COUNT, Math.floor(value)));
}

/**
 * 滑块变更处理。
 *
 * 实时更新本地状态（用于即时反馈），持久化在 `change` 事件中完成。
 */
function onInput(event: Event): void {
  const target = event.target as HTMLInputElement;
  count.value = clamp(Number(target.value));
}

/**
 * 滑块释放时持久化到 AppConfig。
 *
 * 调小数量时同步裁剪 `recent_projects.json` 中的存储列表，
 * 确保存储与展示一致。
 */
async function onChange(): Promise<void> {
  if (saving.value) return;
  saving.value = true;
  try {
    const config = await loadConfig();
    const newCount = count.value;
    if (config.recent_projects_count === newCount) return;
    await saveConfig({ ...config, recent_projects_count: newCount });
    // 调小数量时裁剪存储列表
    await trim(newCount);
  } catch (err) {
    console.error('[GeneralSection] 保存最近项目数量失败:', err);
    // 回滚到已保存的值
    const config = await loadConfig();
    count.value = clamp(config.recent_projects_count);
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <div class="section">
    <h2 class="section__title">{{ t('settings.sections.general') }}</h2>
    <p class="section__desc">{{ t('settings.general.description') }}</p>

    <!-- 最近打开项目数量 -->
    <div class="form-group">
      <div class="form-row">
        <label class="form-row__label" for="recent-projects-count">
          {{ t('settings.general.recentProjectsCount') }}
        </label>
        <span class="form-row__value">{{ count }}</span>
      </div>
      <input
        id="recent-projects-count"
        type="range"
        class="slider"
        :min="MIN_COUNT"
        :max="MAX_COUNT"
        step="1"
        :value="count"
        :disabled="saving"
        @input="onInput"
        @change="onChange"
      />
      <div class="slider-marks">
        <span>{{ MIN_COUNT }}</span>
        <span>{{ MAX_COUNT }}</span>
      </div>
      <p class="form-hint">{{ t('settings.general.recentProjectsCountHint') }}</p>
    </div>
  </div>
</template>

<style scoped>
.section {
  padding: 0;
}

.section__title {
  margin: 0 0 0.25rem;
  font-family: var(--fluen-font-sans);
  font-size: 1.4rem;
  font-weight: 600;
  color: var(--fluen-ink);
  letter-spacing: -0.5px;
}

.section__desc {
  margin: 0 0 1.5rem;
  font-family: var(--fluen-font-sans);
  font-size: 0.85rem;
  color: var(--fluen-stone);
}

.form-group {
  margin-top: 0.5rem;
  max-width: 480px;
}

.form-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 0.6rem;
}

.form-row__label {
  font-family: var(--fluen-font-sans);
  font-size: 0.9rem;
  font-weight: 500;
  color: var(--fluen-ink);
}

.form-row__value {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 28px;
  height: 24px;
  padding: 0 8px;
  border-radius: 6px;
  background: var(--fluen-info-bg);
  color: var(--fluen-info);
  font-family: var(--fluen-font-mono);
  font-size: 0.85rem;
  font-weight: 600;
}

/* ── 滑块 ──────────────────────────────────────────────────────────── */
.slider {
  width: 100%;
  height: 4px;
  appearance: none;
  -webkit-appearance: none;
  background: var(--fluen-hairline);
  border-radius: 9999px;
  outline: none;
  cursor: pointer;
}

.slider:disabled {
  cursor: not-allowed;
  opacity: 0.6;
}

/* WebKit 滑块拇指 */
.slider::-webkit-slider-thumb {
  -webkit-appearance: none;
  appearance: none;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: var(--fluen-accent);
  border: 2px solid var(--fluen-canvas);
  cursor: pointer;
  transition: transform 0.15s ease, box-shadow 0.15s ease;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.2);
}

.slider::-webkit-slider-thumb:hover {
  transform: scale(1.1);
}

.slider:active::-webkit-slider-thumb {
  transform: scale(1.15);
}

/* Firefox 滑块拇指 */
.slider::-moz-range-thumb {
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: var(--fluen-accent);
  border: 2px solid var(--fluen-canvas);
  cursor: pointer;
  transition: transform 0.15s ease;
}

.slider::-moz-range-thumb:hover {
  transform: scale(1.1);
}

/* ── 滑块刻度标记 ────────────────────────────────────────────────────── */
.slider-marks {
  display: flex;
  justify-content: space-between;
  margin-top: 0.4rem;
  font-family: var(--fluen-font-mono);
  font-size: 0.7rem;
  color: var(--fluen-muted);
}

.form-hint {
  margin: 0.6rem 0 0;
  font-size: 0.72rem;
  color: var(--fluen-stone);
  line-height: 1.4;
}
</style>
