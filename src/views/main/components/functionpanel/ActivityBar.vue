<script setup lang="ts">
/**
 * ActivityBar — VSCode 风格的活动栏。
 *
 * 始终可见的最左侧窄条（48px），承载视图切换图标。
 * 外观在折叠/展开态下保持一致，确保交互稳定不突兀。
 *
 * 点击行为由父组件（FunctionPanel）通过 `select-activity` 事件决定：
 *   - 点击未激活项 → 激活并展开侧边栏
 *   - 点击已激活项 → 切换侧边栏展开/收起
 *
 * 视觉规则：
 *   - 激活且侧边栏展开 → 图标高亮 + 左侧 indicator 条
 *   - 激活但侧边栏收起 → 图标回到默认色，indicator 消失（提示视图已隐藏）
 *
 * 通过 `position: 'bottom'` 可将条目置于底部槽位（账户/设置等），便于扩展。
 */
import { computed } from 'vue';
import { ACTIVITY_ITEMS } from '../../constants';
import { useI18n } from '../../../../i18n';

const props = defineProps<{
  /** 当前激活的活动栏项 id */
  activeActivity: string;
  /** 侧边栏是否收起（收起时激活项不显示 indicator） */
  collapsed?: boolean;
}>();

defineEmits<{
  (e: 'select-activity', id: string): void;
}>();

const { t } = useI18n();

/** 顶部主视图切换项（默认）。 */
const topItems = computed(() =>
  ACTIVITY_ITEMS.filter((item) => item.position !== 'bottom'),
);

/** 底部辅助入口项（账户/设置等）。 */
const bottomItems = computed(() =>
  ACTIVITY_ITEMS.filter((item) => item.position === 'bottom'),
);

/** 某项是否处于"激活且侧边栏展开"状态（用于显示 indicator 与高亮）。 */
function isActiveAndShown(id: string): boolean {
  return id === props.activeActivity && !props.collapsed;
}
</script>

<template>
  <nav class="activity-bar" aria-label="activity bar">
    <div class="activity-bar__group activity-bar__group--top">
      <button
        v-for="item in topItems"
        :key="item.id"
        class="activity-bar__item"
        :class="{ 'activity-bar__item--active': isActiveAndShown(item.id) }"
        :title="t('main.activity.' + item.id)"
        :aria-pressed="item.id === activeActivity"
        @click="$emit('select-activity', item.id)"
      >
        <svg
          viewBox="0 0 24 24"
          width="22"
          height="22"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path :d="item.icon" />
        </svg>
      </button>
    </div>

    <div
      v-if="bottomItems.length"
      class="activity-bar__group activity-bar__group--bottom"
    >
      <button
        v-for="item in bottomItems"
        :key="item.id"
        class="activity-bar__item"
        :class="{ 'activity-bar__item--active': isActiveAndShown(item.id) }"
        :title="t('main.activity.' + item.id)"
        :aria-pressed="item.id === activeActivity"
        @click="$emit('select-activity', item.id)"
      >
        <svg
          viewBox="0 0 24 24"
          width="22"
          height="22"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path :d="item.icon" />
        </svg>
      </button>
    </div>
  </nav>
</template>

<style scoped>
.activity-bar {
  display: flex;
  flex-direction: column;
  width: 48px;
  height: 100%;
  flex-shrink: 0;
  background: var(--fluen-surface-deep);
  border-right: 1px solid var(--fluen-hairline);
  user-select: none;
}

/* ── 分组：顶部主视图 / 底部辅助 ───────────────────────────────────── */
.activity-bar__group {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  padding: 8px 0;
}

.activity-bar__group--top {
  flex: 1;
}

.activity-bar__group--bottom {
  flex-shrink: 0;
  border-top: 1px solid var(--fluen-hairline);
}

/* ── 按钮 ───────────────────────────────────────────────────────────── */
.activity-bar__item {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 48px;
  height: 48px;
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
  transition: color 0.15s ease, background 0.15s ease;
}

.activity-bar__item:hover {
  color: var(--fluen-ink);
  background: var(--fluen-hover);
}

.activity-bar__item--active {
  color: var(--fluen-ink);
}

/* 左侧 indicator 条（仅激活且展开时显示） */
.activity-bar__item--active::before {
  content: '';
  position: absolute;
  left: 0;
  top: 50%;
  transform: translateY(-50%);
  width: 2px;
  height: 24px;
  background: var(--fluen-accent);
  border-radius: 0 2px 2px 0;
}
</style>
