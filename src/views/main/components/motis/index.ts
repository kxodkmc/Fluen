/**
 * Motis 模块 — 公共 API。
 *
 * 导出 MotisPanel 容器及其子组件，以及 useMotisChat 注入键。
 *
 * 使用方式：
 * ```ts
 * import { MotisPanel, MOTIS_CHAT_KEY } from '@/views/main/components/motis';
 * ```
 *
 * 组件树：
 *   MotisPanel
 *   ├── MotisChatHeader
 *   ├── MotisChatMessageList
 *   │   ├── MotisThinkingIndicator
 *   │   └── MotisToolCallBubble
 *   └── MotisChatInput
 */

export { default as MotisPanel } from './MotisPanel.vue';
export { default as MotisChatHeader } from './MotisChatHeader.vue';
export { default as MotisChatMessageList } from './MotisChatMessageList.vue';
export { default as MotisChatInput } from './MotisChatInput.vue';
export { default as MotisThinkingIndicator } from './MotisThinkingIndicator.vue';
export { default as MotisToolCallBubble } from './MotisToolCallBubble.vue';
export { MOTIS_CHAT_KEY } from './symbols';
