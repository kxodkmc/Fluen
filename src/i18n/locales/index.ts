/**
 * 翻译资源汇总入口。
 *
 * 导出受支持的 locale 列表、默认语言以及合并后的 messages 对象。
 */

import type { SupportedLocale } from '../types';
import zhCN from './zh-CN';
import en from './en';
import es from './es';

/** 受支持的 locale 列表（label 为原生写法，不翻译）。 */
export const SUPPORTED_LOCALES: SupportedLocale[] = [
  { key: 'zh-CN', label: '简体中文' },
  { key: 'en', label: 'English' },
  { key: 'es', label: 'Español' },
];

/** 默认语言。 */
export const DEFAULT_LOCALE = 'zh-CN' as const;

/** 合并后的 messages 对象（供 createI18n 使用）。 */
export const messages = {
  'zh-CN': zhCN,
  en,
  es,
};
