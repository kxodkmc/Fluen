/**
 * 主界面模块 — 静态配置与常量。
 *
 * 活动栏图标、默认面板尺寸、菜单项等静态数据集中于此，
 * 组件保持声明式，易于扩展 — 只需在此添加条目即可。
 */

import type { ActivityItem } from './types';

/* ── 默认面板尺寸 ─────────────────────────────────────────────────────── */
export const DEFAULT_PANEL_SIZES = {
  functionPanel: 260,
  aiPanel: 360,
} as const;

/** 活动栏固定宽度（px），同时为功能区折叠后的剩余宽度。 */
export const ACTIVITY_BAR_WIDTH = 48;

/** 面板最小/最大宽度约束（px）。 */
export const PANEL_CONSTRAINTS = {
  functionPanel: { min: 200, max: 480 },
  aiPanel: { min: 280, max: 560 },
} as const;

/** 标题栏高度（px）。 */
export const TITLE_BAR_HEIGHT = 40;

/* ── 活动栏项目 ───────────────────────────────────────────────────────── */
export const ACTIVITY_ITEMS: ActivityItem[] = [
  {
    id: 'outline',
    icon: 'M4 6h16M4 12h12M4 18h8',
  },
  {
    id: 'references',
    icon: 'M4 19.5A2.5 2.5 0 0 1 6.5 17H20M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2Z',
  },
];

/* ── 标题栏菜单 ───────────────────────────────────────────────────────── */
export const TITLE_BAR_MENU_IDS = ['files', 'edit', 'view', 'help'] as const;

/** 文件菜单子项 ID。 */
export const FILE_MENU_ITEM_IDS = [
  'newArticle',
  'openArticle',
  'recentArticles',
] as const;

/** 应用标题（显示于标题栏左侧 logo 旁）。 */
export const APP_TITLE = 'Fluen';

/** 标题栏响应式断点（px）。 */
export const TITLE_BAR_BREAKPOINTS = {
  /** 菜单文字隐藏，仅显示 logo 与图标按钮 */
  menuCollapse: 760,
  /** 搜索栏收缩为图标按钮 */
  searchCollapse: 560,
} as const;

/* ── Motis 状态气泡文案池 ────────────────────────────────────────────── */
/*
 * 依据 MascotConfig.professional_expression 切换风格：
 *   false → 拟人化文案（随机选取一条 i18n key，由调用方 t() 翻译）
 *   true  → 专业化文案兜底（i18n key）
 *
 * 实际展示文案存储于 i18n 资源（main.motisPanel.playfulThinking.* /
 * playfulTool.* / professionalTool / professionalThinking），
 * 此处仅维护 key 列表以支持随机选取。
 */

/** 工具调用时的拟人化文案 i18n key 列表（professional_expression=false 时随机选取）。 */
export const PLAYFUL_TOOL_MESSAGE_KEYS: string[] = [
  'main.motisPanel.playfulTool.tool1',
  'main.motisPanel.playfulTool.tool2',
  'main.motisPanel.playfulTool.tool3',
  'main.motisPanel.playfulTool.tool4',
  'main.motisPanel.playfulTool.tool5',
  'main.motisPanel.playfulTool.tool6',
];

/** 思考时的拟人化文案 i18n key 列表（professional_expression=false 时随机选取）。 */
export const PLAYFUL_THINKING_MESSAGE_KEYS: string[] = [
  'main.motisPanel.playfulThinking.thinking1',
  'main.motisPanel.playfulThinking.thinking2',
  'main.motisPanel.playfulThinking.thinking3',
  'main.motisPanel.playfulThinking.thinking4',
  'main.motisPanel.playfulThinking.thinking5',
];

/** 工具调用时的专业化文案 i18n key（professional_expression=true 时使用）。 */
export const PROFESSIONAL_TOOL_MESSAGE_KEY = 'main.motisPanel.professionalTool';

/** 思考时的专业化文案 i18n key（professional_expression=true 时使用）。 */
export const PROFESSIONAL_THINKING_MESSAGE_KEY = 'main.motisPanel.professionalThinking';

/**
 * 从 key 列表中随机选取一条。
 *
 * @param keys i18n key 数组
 * @returns 随机选取的 key；池为空时返回空字符串
 */
export function getRandomMessageKey(keys: string[]): string {
  if (keys.length === 0) return '';
  return keys[Math.floor(Math.random() * keys.length)];
}
