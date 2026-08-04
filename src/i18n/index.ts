/**
 * 国际化模块 — 公共 API。
 *
 * 模块结构：
 *   - types.ts         类型定义（Language / SupportedLocale / MessageSchema）
 *   - locales/         翻译资源（zh-CN / en）与汇总入口
 *   - composables/     useI18n composable（包装 vue-i18n）
 *
 * 使用方式：
 *   import { useI18n, SUPPORTED_LOCALES, DEFAULT_LOCALE } from '@/i18n';
 *   const { t, locale, setLocale } = useI18n();
 *
 * 在 main.ts 中作为 Vue 插件注册：
 *   import { i18nPlugin } from '@/i18n';
 *   app.use(i18nPlugin);
 */

import { createI18n } from 'vue-i18n';
import { messages, DEFAULT_LOCALE } from './locales';

/**
 * vue-i18n 实例。
 *
 * 单独导出以便 useAppConfig 等模块直接引用，避免通过 composables 产生循环依赖。
 */
export const i18nInstance = createI18n({
  legacy: false,
  locale: DEFAULT_LOCALE,
  fallbackLocale: DEFAULT_LOCALE,
  messages,
  missingWarn: false,
  fallbackWarn: false,
});

/**
 * Vue 插件（即 vue-i18n 实例本身，已实现 install 方法）。
 *
 * 在 main.ts 中 `app.use(i18nPlugin)` 注册。
 */
export const i18nPlugin = i18nInstance;

export { useI18n } from './composables';
export { SUPPORTED_LOCALES, DEFAULT_LOCALE } from './locales';
export type { Language, SupportedLocale, MessageSchema } from './types';
