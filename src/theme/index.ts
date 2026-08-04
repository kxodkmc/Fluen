/**
 * 主题管理系统 — 公共 API。
 *
 * 模块结构：
 *   - types.ts      类型定义
 *   - tokens.ts     token 到 CSS 变量的映射与扁平化工具
 *   - schema.ts     主题包 JSON Schema 校验
 *   - registry.ts   主题注册表
 *   - loader.ts     主题加载器（解析、注入 DOM）
 *   - packs/        内置主题包（light / dark）
 *   - provider/     ThemeProvider 根组件
 *   - composables/  useTheme composable
 *   - styles/       主题样式入口（CSS 变量 fallback）
 *
 * 使用方式：
 *   import { useTheme, ThemeProvider } from '@/theme';
 */

export * from './types';
export * from './tokens';
// 以下模块将在后续任务中实现，届时取消注释
export * from './schema';
export * from './registry';
export * from './loader';
export { default as ThemeProvider } from './provider/ThemeProvider.vue';
export { useTheme } from './composables/useTheme';
export type { UseThemeReturn } from './composables/useTheme';
