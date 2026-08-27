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
    // bi-book（Bootstrap Icons，16×16 填充网格）
    id: 'references',
    icon: 'M1 2.828c.885-.37 2.154-.769 3.388-.893 1.33-.134 2.458.063 3.112.752v9.746c-.935-.53-2.12-.603-3.213-.493-1.18.12-2.37.461-3.287.811V2.828zm7.5-.141c.654-.689 1.782-.886 3.112-.752 1.234.124 2.503.523 3.388.893v9.923c-.918-.35-2.107-.692-3.287-.81-1.094-.111-2.278-.039-3.213.492V2.687zM8 1.783C7.015.936 5.587.81 4.287.94c-1.514.153-3.042.672-3.994 1.105A.5.5 0 0 0 0 2.5v11a.5.5 0 0 0 .707.455c.882-.4 2.303-.881 3.68-1.02 1.409-.142 2.59.087 3.223.877a.5.5 0 0 0 .78 0c.633-.79 1.814-1.019 3.222-.877 1.378.139 2.8.62 3.681 1.02A.5.5 0 0 0 16 13.5v-11a.5.5 0 0 0-.293-.455c-.952-.433-2.48-.952-3.994-1.105C10.413.809 8.985.936 8 1.783z',
    viewBox: '0 0 16 16',
    filled: true,
  },
  {
    // bi-collection（Bootstrap Icons，16×16 填充网格）
    id: 'knowledge',
    icon: 'M2.5 3.5a.5.5 0 0 1 0-1h11a.5.5 0 0 1 0 1h-11zm2-2a.5.5 0 0 1 0-1h7a.5.5 0 0 1 0 1h-7zM0 13a1.5 1.5 0 0 0 1.5 1.5h13A1.5 1.5 0 0 0 16 13V6a1.5 1.5 0 0 0-1.5-1.5h-13A1.5 1.5 0 0 0 0 6v7zm1.5.5A.5.5 0 0 1 1 13V6a.5.5 0 0 1 .5-.5h13a.5.5 0 0 1 .5.5v7a.5.5 0 0 1-.5.5h-13z',
    viewBox: '0 0 16 16',
    filled: true,
  },
  {
    // bi-bar-chart（Bootstrap Icons，16×16 填充网格）
    id: 'data',
    icon: 'M4 11H2v3h2v-3zm5-4H7v7h2V7zm5-5v12h-2V2h2zm-2-1a1 1 0 0 0-1 1v12a1 1 0 0 0 1 1h2a1 1 0 0 0 1-1V2a1 1 0 0 0-1-1h-2zM6 7a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v7a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V7zm-5 4a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v3a1 1 0 0 1-1 1H2a1 1 0 0 1-1-1v-3z',
    viewBox: '0 0 16 16',
    filled: true,
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

/** 帮助菜单子项 ID。 */
export const HELP_MENU_ITEM_IDS = ['about', 'openLogsDir'] as const;

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
