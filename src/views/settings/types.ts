/**
 * 设置模块 — 类型定义。
 *
 * 所有设置页面中的共享类型集中于此，
 * 供 SettingsView、sections 和 constants 统一导入。
 */

import type { Component, InjectionKey } from 'vue';

/** 设置分区定义。 */
export interface SettingsSection {
  /** 唯一标识（用于导航跳转与 i18n key 匹配）。 */
  id: string;
  /** i18n 翻译 key 前缀（如 `settings.sections.appearance`）。 */
  labelKey: string;
  /** SVG path 内容（用于侧边栏图标）。 */
  icon: string;
  /** 该分区的 Vue 组件（叶子分区必有；分组不需要）。 */
  component?: Component;
  /** 子分区列表（存在时该分区作为可折叠分组渲染）。 */
  children?: SettingsSection[];
}

/** 设置页面导航目标（从 TitleBar 下拉菜单触发）。 */
export type SettingsSectionId =
  | 'general'
  | 'appearance'
  | 'language'
  | 'llmConfig'
  | 'motis'
  | 'literature'
  | 'ocrService'
  | 'oldAiServices'
  | 'logging';

/**
 * 分区导航注入 key。
 * SettingsView 提供此函数，子分区组件可注入后跳转到其他分区。
 */
export const SETTINGS_NAVIGATE_KEY: InjectionKey<(id: SettingsSectionId) => void> = Symbol('settings-navigate');
