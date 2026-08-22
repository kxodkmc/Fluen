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
 *   │   ├── MotisActivityGroup（连续思考/工具调用的可折叠时间线）
 *   │   └── MotisThinkingIndicator（尾部思考指示器）
 *   └── MotisChatInput
 */

export { default as MotisPanel } from './MotisPanel.vue';
export { default as MotisChatHeader } from './MotisChatHeader.vue';
export { default as MotisChatMessageList } from './MotisChatMessageList.vue';
export { default as MotisChatInput } from './MotisChatInput.vue';
export { default as MotisActivityGroup } from './MotisActivityGroup.vue';
export { default as MotisThinkingIndicator } from './MotisThinkingIndicator.vue';
export { MOTIS_CHAT_KEY } from './symbols';
