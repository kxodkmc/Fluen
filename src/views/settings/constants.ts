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
import GeneralSection from './sections/GeneralSection.vue';
import AppearanceSection from './sections/AppearanceSection.vue';
import LanguageSection from './sections/LanguageSection.vue';
import LlmConfigSection from './sections/LlmConfigSection.vue';
import MotisSection from './sections/MotisSection.vue';
import LiteratureSection from './sections/LiteratureSection.vue';
import OcrServiceSection from './sections/OcrServiceSection.vue';
import AiServicesSection from './sections/AiServicesSection.vue';
import LoggingSection from './sections/LoggingSection.vue';

/** 设置分区注册表（有序）。 */
export const SETTINGS_SECTIONS: SettingsSection[] = [
  {
    id: 'general',
    labelKey: 'settings.sections.general',
    icon: 'M3 6l9-3 9 3M3 6l9 3 9-3M3 6v12l9 3 9-3V6M12 9v12',
    component: GeneralSection,
  },
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
    id: 'aiServices',
    labelKey: 'settings.sections.aiServices',
    icon: 'M3 7V5a2 2 0 0 1 2-2h2M17 3h2a2 2 0 0 1 2 2v2M21 17v2a2 2 0 0 1-2 2h-2M7 21H5a2 2 0 0 1-2-2v-2M7 12h10',
    children: [
      {
        id: 'literature',
        labelKey: 'settings.sections.literature',
        icon: 'M4 19.5A2.5 2.5 0 0 1 6.5 17H20V2H6.5A2.5 2.5 0 0 0 4 4.5v15z M4 19.5v0A2.5 2.5 0 0 0 6.5 22H20v-5',
        component: LiteratureSection,
      },
      {
        id: 'ocrService',
        labelKey: 'settings.sections.ocrService',
        icon: 'M3 7a2 2 0 0 1 2-2h2l2-2h6l2 2h2a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z M12 16a4 4 0 1 0 0-8 4 4 0 0 0 0 8z M15 9h.01',
        component: OcrServiceSection,
      },
      {
        id: 'motis',
        labelKey: 'settings.sections.motis',
        icon: 'M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2z',
        component: MotisSection,
      },
    ],
  },
  {
    id: 'oldAiServices',
    labelKey: 'settings.sections.oldAiServices',
    icon: 'M3 7V5a2 2 0 0 1 2-2h2M17 3h2a2 2 0 0 1 2 2v2M21 17v2a2 2 0 0 1-2 2h-2M7 21H5a2 2 0 0 1-2-2v-2M7 12h10',
    component: AiServicesSection,
  },
  {
    id: 'logging',
    labelKey: 'settings.sections.logging',
    icon: 'M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z M14 2v6h6 M16 13H8 M16 17H8 M10 9H8',
    component: LoggingSection,
  },
];

/** 默认激活的分区 ID。 */
export const DEFAULT_SETTINGS_SECTION = 'general';
