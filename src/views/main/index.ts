/**
 * 主界面模块 — 公共导出。
 *
 * 仅导出顶层视图组件和共享类型，内部面板组件与 composable
 * 保持模块私有，确保公共 API 最小化。
 */
export { default as MainView } from './MainView.vue';
export * from './types';
export * from './constants';
export { useMainLayout } from './composables/useMainLayout';
export type { UseMainLayoutReturn } from './composables/useMainLayout';
