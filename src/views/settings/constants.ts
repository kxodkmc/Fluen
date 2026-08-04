/**
 * 设置模块 — 静态配置与分区注册表。
 *
 * 所有设置分区集中注册于此，SettingsView 根据此列表
 * 渲染侧边栏导航与对应内容面板。
 *
 * 扩展方式：
 *   1. 在 `sections/` 下创建新的 Section 组件
 *   2. 在此数组中添加一条注册项
 *   3. 在 i18n locale 文件中添加对应翻译
 */

import type { SettingsSection } from './types';
import AppearanceSection from './sections/AppearanceSection.vue';
import LanguageSection from './sections/LanguageSection.vue';
import LlmConfigSection from './sections/LlmConfigSection.vue';
import MotisSection from './sections/MotisSection.vue';
import AiServicesSection from './sections/AiServicesSection.vue';

/** 设置分区注册表（有序）。 */
export const SETTINGS_SECTIONS: SettingsSection[] = [
  {
    id: 'appearance',
    labelKey: 'settings.sections.appearance',
    icon: 'M12 2v2m0 16v2M4.93 4.93l1.41 1.41m11.32 11.32l1.41 1.41M2 12h2m16 0h2M4.93 19.07l1.41-1.41m11.32-11.32l1.41-1.41M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z',
    component: AppearanceSection,
  },
  {
    id: 'language',
    labelKey: 'settings.sections.language',
    icon: 'M4 5h7M9 3v2c0 4.418-2.239 8-5 8M5 9c0 2.144 2.239 4 5 4m6-8h7m-5-2v2c0 4.418 2.239 8 5 8m-5-6c0 2.144 2.239 4 5 4M4 19l4-9 4 9M5.5 16h5',
    component: LanguageSection,
  },
  {
    id: 'llmConfig',
    labelKey: 'settings.sections.llmConfig',
    icon: 'M12 2a3 3 0 0 0-3 3v1H7a3 3 0 0 0-3 3v1H3v2h1v1a3 3 0 0 0 3 3h2v1a3 3 0 0 0 6 0v-1h2a3 3 0 0 0 3-3v-1h1v-2h-1V9a3 3 0 0 0-3-3h-2V5a3 3 0 0 0-3-3z',
    component: LlmConfigSection,
  },
  {
    id: 'motis',
    labelKey: 'settings.sections.motis',
    icon: 'M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-3.5 6a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3zm7 0a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3zM12 17.5c-2.33 0-4.31-1.46-5.11-3.5h10.22c-.8 2.04-2.78 3.5-5.11 3.5z',
    component: MotisSection,
  },
  {
    id: 'aiServices',
    labelKey: 'settings.sections.aiServices',
    icon: 'M3 7V5a2 2 0 0 1 2-2h2M17 3h2a2 2 0 0 1 2 2v2M21 17v2a2 2 0 0 1-2 2h-2M7 21H5a2 2 0 0 1-2-2v-2M7 12h10',
    component: AiServicesSection,
  },
];

/** 默认激活的分区 ID。 */
export const DEFAULT_SETTINGS_SECTION = 'appearance';
