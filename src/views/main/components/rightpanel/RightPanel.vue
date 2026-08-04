<script setup lang="ts">
/**
 * RightPanel — 右侧面板容器。
 *
 * 顶部 pill-tab 切换器在 Motis 对话与学术助手之间切换，
 * 内容区使用 v-show 保留各面板状态（不销毁组件）。
 *
 * 设计参考 DESIGN.md 的 pill-tab / pill-tab-active 样式：
 *   - 未选中：canvas 背景、steel 文字、hairline 边框、rounded-full
 *   - 选中：primary 背景、on-primary 文字、rounded-full
 *
 * @prop activeRightPanel - 当前激活的面板 ID
 * @emits select-panel - 切换面板（传入面板 ID）
 * @emits close - 关闭右侧面板
 */
import { useI18n } from '../../../../i18n';
import type { RightPanelId } from '../../types';
import { MotisPanel } from '../motis';
import AIPanel from '../AIPanel.vue';

defineProps<{
  /** 当前激活的右侧面板。 */
  activeRightPanel: RightPanelId | null;
}>();

defineEmits<{
  (e: 'select-panel', panel: RightPanelId): void;
  (e: 'close'): void;
}>();

const { t } = useI18n();
</script>

<template>
  <div class="right-panel">
    <!-- ── 顶部面板切换器 ─────────────────────────────────────────── -->
    <div class="right-panel__tabs">
      <button
        class="right-panel__tab"
        :class="{ 'right-panel__tab--active': activeRightPanel === 'motis' }"
        @click="$emit('select-panel', 'motis')"
      >
        {{ t('main.rightPanel.motis') }}
      </button>
      <button
        class="right-panel__tab"
        :class="{ 'right-panel__tab--active': activeRightPanel === 'assistant' }"
        @click="$emit('select-panel', 'assistant')"
      >
        {{ t('main.rightPanel.assistant') }}
      </button>
    </div>

    <!-- ── 内容区（v-show 保留各面板状态） ──────────────────────── -->
    <div class="right-panel__content">
      <!-- Motis 对话面板 -->
      <div v-show="activeRightPanel === 'motis'" class="right-panel__pane">
        <MotisPanel @close="$emit('close')" />
      </div>

      <!-- 学术助手面板（AIPanel 内部自管理状态） -->
      <div v-show="activeRightPanel === 'assistant'" class="right-panel__pane">
        <AIPanel :messages="[]" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.right-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--fluen-surface);
  overflow: hidden;
}

/* ── 面板切换器 ─────────────────────────────────────────────────────── */
.right-panel__tabs {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 12px;
  flex-shrink: 0;
  border-bottom: 1px solid var(--fluen-hairline);
  background: var(--fluen-surface);
}

/* pill-tab 样式（参考 DESIGN.md） */
.right-panel__tab {
  padding: 4px 14px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 9999px; /* rounded.full */
  background: var(--fluen-canvas);
  color: var(--fluen-steel);
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease, border-color 0.15s ease;
}

.right-panel__tab:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

/* pill-tab-active 样式 */
.right-panel__tab--active {
  background: var(--fluen-primary);
  color: var(--fluen-on-primary);
  border-color: var(--fluen-primary);
}

.right-panel__tab--active:hover {
  background: var(--fluen-primary);
  color: var(--fluen-on-primary);
}

/* ── 内容区 ─────────────────────────────────────────────────────────── */
.right-panel__content {
  flex: 1;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.right-panel__pane {
  flex: 1;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
</style>
