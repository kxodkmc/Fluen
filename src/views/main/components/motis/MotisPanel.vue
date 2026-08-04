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
import { computed, inject } from 'vue';
import { useMotisChat } from '../../composables/useMotisChat';
import { MOTIS_CHAT_KEY } from './symbols';
import MotisChatHeader from './MotisChatHeader.vue';
import MotisChatMessageList from './MotisChatMessageList.vue';
import MotisChatInput from './MotisChatInput.vue';

defineEmits<{
  (e: 'close'): void;
}>();

/* ── 注入 useMotisChat 实例 ──────────────────────────────────────────── */
// 优先使用 MainView 提供的共享实例；未提供时创建独立实例（兼容独立使用场景）
const motisChat = inject(MOTIS_CHAT_KEY, () => useMotisChat(), true);

/* ── 解构状态与方法（顶层绑定 → 模板中自动解包 ref） ────────────────── */
const { messages, isGenerating, draftMessage, send, cancel } = motisChat;

/** 草稿消息双向绑定（v-model 需要 computed 包装 ref）。 */
const draft = computed({
  get: () => draftMessage.value,
  set: (val: string) => {
    draftMessage.value = val;
  },
});

/** 发送当前草稿消息（MotisChatInput 的 send 事件无 payload，需主动传入草稿内容）。 */
function handleSend(): void {
  send(draftMessage.value);
  // 发送后清空草稿
  draftMessage.value = '';
}
</script>

<template>
  <div class="motis-panel">
    <!-- 头部 -->
    <MotisChatHeader @close="$emit('close')" />

    <!-- 对话区 -->
    <MotisChatMessageList :messages="messages" :is-generating="isGenerating" />

    <!-- 预留扩展区（未来记忆功能、自动任务等） -->
    <slot name="extension" />

    <!-- 输入区 -->
    <MotisChatInput
      v-model="draft"
      :is-generating="isGenerating"
      @send="handleSend"
      @cancel="cancel"
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
}
</style>
