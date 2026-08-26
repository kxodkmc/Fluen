<script setup lang="ts">
/**
 * MotisPanel — Motis 对话面板容器。
 *
 * 编排结构：
 *   ┌──────────────────────────────┐
 *   │ MotisChatHeader              │  头部（头像 + 名称 + 心情 + 关闭）
 *   ├──────────────────────────────┤
 *   │ MotisChatMessageList         │  对话区（消息列表 + 思考指示器 + 工具气泡）
 *   ├──────────────────────────────┤
 *   │ <slot name="extension" />    │  预留扩展区（未来记忆功能、自动任务等）
 *   ├──────────────────────────────┤
 *   │ MotisChatInput               │  输入区（文本框 + 发送/停止）
 *   └──────────────────────────────┘
 *
 * 通过 inject 获取 useMotisChat 实例（由 MainView provide），
 * 确保整个组件树共享同一份对话状态。
 *
 * @emits close - 关闭按钮点击时触发（由 RightPanel 收起面板）
 */
import { computed, inject, watch } from 'vue';
import { useMotisChat } from '../../composables/useMotisChat';
import { useChatToolbar } from '../../composables/useChatToolbar';
import { useMainLayout, MAIN_LAYOUT_KEY } from '../../composables/useMainLayout';
import { MOTIS_CHAT_KEY } from './symbols';
import MotisChatHeader from './MotisChatHeader.vue';
import MotisChatMessageList from './MotisChatMessageList.vue';
import MotisChatInput from './MotisChatInput.vue';
import ChatInputToolbar from '../ChatInputToolbar.vue';
import ApprovalDialog from '../ApprovalDialog.vue';

defineEmits<{
  (e: 'close'): void;
}>();

/* ── 注入 useMotisChat 实例 ──────────────────────────────────────────── */
// 优先使用 MainView 提供的共享实例；未提供时创建独立实例（兼容独立使用场景）
const motisChat = inject(MOTIS_CHAT_KEY, () => useMotisChat(), true);

/* ── 注入布局状态（用于底部工具栏切换模式） ─────────────────────────── */
const layout = inject(MAIN_LAYOUT_KEY, () => useMainLayout(), true);

/* ── 聊天工具栏状态（模块级单例，跨面板共享） ───────────────────────── */
const toolbar = useChatToolbar();

/* ── 解构状态与方法（顶层绑定 → 模板中自动解包 ref） ────────────────── */
const { messages, isGenerating, pendingApprovals, draftMessage, send, cancel, resolveApproval } =
  motisChat;

/** 草稿消息双向绑定（v-model 需要 computed 包装 ref）。 */
const draft = computed({
  get: () => draftMessage.value,
  set: (val: string) => {
    draftMessage.value = val;
  },
});

/** 发送当前草稿消息。 */
function handleSend(): void {
  send(draftMessage.value);
  draftMessage.value = '';
}

/** 工具栏模式切换时同步到布局状态。 */
watch(
  () => toolbar.mode.value,
  (next) => {
    layout.setActiveRightPanel(next);
  },
);

/** 外部切换面板时（如标题栏 Mascot 点击）同步回工具栏。 */
watch(
  () => layout.activeRightPanel.value,
  (next) => {
    if (next && next !== toolbar.mode.value) {
      toolbar.mode.value = next;
    }
  },
  { immediate: true },
);
</script>

<template>
  <div class="motis-panel">
    <!-- 头部 -->
    <MotisChatHeader @close="$emit('close')" />

    <!-- 对话区 -->
    <MotisChatMessageList :messages="messages" :is-generating="isGenerating" />

    <!-- 预留扩展区（未来记忆功能、自动任务等） -->
    <slot name="extension" />

    <!-- 工具写操作确认弹窗（浮动层，共享组件） -->
    <ApprovalDialog
      :approvals="pendingApprovals"
      @resolve="(id, approved) => void resolveApproval(id, approved)"
    />

    <!-- 输入区 -->
    <MotisChatInput
      v-model="draft"
      :is-generating="isGenerating"
      @send="handleSend"
      @cancel="cancel"
    />

    <!-- 底部工具栏 -->
    <ChatInputToolbar
      :mode="toolbar.mode.value"
      :thinking-intensity="toolbar.thinkingIntensity.value"
      :model-label="toolbar.activeModelLabel.value"
      :models="toolbar.modelOptions.value"
      :can-send="draft.trim().length > 0 && !isGenerating"
      :is-generating="isGenerating"
      @update:mode="toolbar.mode.value = $event"
      @update:thinking-intensity="toolbar.thinkingIntensity.value = $event"
      @add-file="toolbar.addAttachment"
      @select-model="toolbar.selectModel"
      @send="handleSend"
      @stop="cancel"
    />
  </div>
</template>

<style scoped>
.motis-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--fluen-surface);
  overflow: hidden;
  position: relative;
}
</style>
