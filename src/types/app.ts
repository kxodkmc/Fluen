/**
 * App 配置的前端类型定义。
 *
 * 与 Rust 后端 `app_config::model` 模块一一对应。
 *
 * @see src-tauri/src/app_config/model.rs
 */

import type { Language } from '../i18n/types';

/** 主题模式（对应 Rust `ThemeMode`，serde `rename_all = "lowercase"`）。 */
export type ThemeMode = 'light' | 'dark';

/**
 * 界面语言（对应 Rust `Language`，serde `rename = "zh-CN" / "en" / "es"`）。
 *
 * 类型定义统一来自 `src/i18n/types`，保持单一数据源。
 */
export type { Language } from '../i18n/types';

/** App 配置顶层结构，对应 `app_config.json`。 */
export interface AppConfig {
  version: string;
  theme: ThemeMode;
  language: Language;
  onboarding_completed: boolean;
}
