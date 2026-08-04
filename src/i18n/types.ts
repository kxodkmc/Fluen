/**
 * 国际化模块 — 类型定义。
 *
 * 定义语言类型、支持的 locale 元数据以及消息 Schema。
 * MessageSchema 通过 typeof import 从 zh-CN 翻译资源精确推断。
 */

/** 界面语言（与 src/types/app.ts 的 Language 保持一致）。 */
export type Language = 'zh-CN' | 'en' | 'es';

/** 受支持的 locale 元数据（label 为原生写法，不翻译）。 */
export interface SupportedLocale {
  /** 语言 key。 */
  key: Language;
  /** 原生写法的语言名称（如「简体中文」「English」「Español」）。 */
  label: string;
}

/** 消息 Schema 类型（从 zh-CN 翻译资源精确推断）。 */
export type MessageSchema = typeof import('./locales/zh-CN').default;
