<script setup lang="ts">
/**
 * RightPanel — 右侧面板容器。
 *
 * 内容区使用 v-show 保留各面板状态（不销毁组件）。
 * 面板切换由底部 ChatInputToolbar 统一控制。
 *
 * @prop activeRightPanel - 当前激活的面板 ID
 * @emits close - 关闭右侧面板
 */
import type { RightPanelId } from '../../types';
import { MotisPanel } from '../motis';
import AIPanel from '../AIPanel.vue';

defineProps<{
  /** 当前激活的右侧面板。 */
  activeRightPanel: RightPanelId | null;
}>();

defineEmits<{
  (e: 'close'): void;
}>();
</script>

<template>
  <div class="right-panel">
    <!-- ── 内容区（v-show 保留各面板状态） ──────────────────────── -->
    <div class="right-panel__content">
      <!-- Motis 对话面板 -->
      <div v-show="activeRightPanel === 'motis'" class="right-panel__pane">
        <MotisPanel @close="$emit('close')" />
      </div>

      <!-- 学术助手面板（AIPanel 内部自管理状态） -->
      <div v-show="activeRightPanel === 'assistant'" class="right-panel__pane">
        <AIPanel />
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
