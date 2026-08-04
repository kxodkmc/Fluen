/**
 * 设置模块 — 公共导出。
 *
 * 仅导出顶层视图组件和共享类型，
 * 内部分区组件与 composable 保持模块私有，确保公共 API 最小化。
 */
export { default as SettingsView } from './SettingsView.vue';
export * from './types';
export * from './constants';
