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

/** 日志配置（对应 Rust `LogConfig`，serde 默认值在后端定义）。 */
export interface LogConfig {
  /** 全局日志级别：trace/debug/info/warn/error。 */
  level: string;
  /** 是否同时输出到 stdout 控制台。 */
  console_enabled: boolean;
  /** 自定义日志存放目录，为 null 时使用默认缓存目录。 */
  log_dir: string | null;
  /** 单个日志文件最大条目数（512-8192，默认 4096）。 */
  max_entries_per_file: number;
  /** 累计日志文件数上限（1-8192，默认 64）。 */
  max_file_count: number;
}

/** App 配置顶层结构，对应 `app_config.json`。 */
export interface AppConfig {
  version: string;
  theme: ThemeMode;
  language: Language;
  onboarding_completed: boolean;
  /** 日志系统配置。 */
  logging: LogConfig;
  /** 最近打开项目显示数量（1-8，默认 4）。 */
  recent_projects_count: number;
}
