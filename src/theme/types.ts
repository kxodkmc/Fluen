/**
 * 主题管理系统 — 类型定义。
 *
 * 主题包数据模型对齐 DESIGN.md 调色板结构，支持未来个性化主题包扩展。
 */

/** 主题模式（light / dark）。 */
export type ThemeMode = 'light' | 'dark';

/** 主题 ID（唯一标识，如 'fluen-light'、'fluen-dark'）。 */
export type ThemeId = string;

/** 主题元数据。 */
export interface ThemePackMeta {
  id: ThemeId;
  /** 显示名称（如"浅色"、"深色"）。 */
  name: string;
  /** 语义化版本号（如 '1.0.0'）。 */
  version: string;
  /** 作者。 */
  author?: string;
  /** 描述。 */
  description?: string;
  /** 主题模式。 */
  type: ThemeMode;
  /** 预览信息（用于主题选择器展示）。 */
  preview?: {
    /** 预览背景色（CSS 颜色值）。 */
    background: string;
    /** 预览强调色（CSS 颜色值）。 */
    accent: string;
    /** 预览文字色（CSS 颜色值，可选）。 */
    text?: string;
  };
}

/** 品牌色组（对应 DESIGN.md brand-* 系列）。 */
export interface ThemeBrandColors {
  /** 主品牌色，黑色，CTA 主色（对应 colors.primary）。 */
  primary: string;
  /** 主品牌色上的文字色（对应 colors.on-primary）。 */
  onPrimary: string;
  /** 主品牌色柔和版本（对应 colors.primary-soft）。 */
  primarySoft: string;
  /** 品牌珊瑚红（M2.7 标识色）。 */
  coral: string;
  /** 品牌洋红（Music 标识色）。 */
  magenta: string;
  /** 品牌蓝（Hailuo 标识色，主蓝）。 */
  blue: string;
  /** 品牌蓝中调。 */
  blueMid: string;
  /** 品牌蓝深调（表单激活、链接强调）。 */
  blueDeep: string;
  /** 品牌蓝 700（文档标签）。 */
  blue700: string;
  /** 品牌青色（氛围色）。 */
  cyan: string;
  /** 品牌蓝 200（代码徽章背景）。 */
  blue200: string;
  /** 品牌紫色（Speech 标识色）。 */
  purple: string;
}

/** 表面色组（对应 DESIGN.md canvas / surface / hairline 系列）。 */
export interface ThemeSurfaceColors {
  /** 主画布背景（对应 colors.canvas）。 */
  canvas: string;
  /** 表面背景（对应 colors.surface）。 */
  surface: string;
  /** 深层表面（项目原有 token）。 */
  surfaceDeep: string;
  /** 柔和表面（对应 colors.surface-soft）。 */
  surfaceSoft: string;
}

/** 边框色组（对应 DESIGN.md hairline 系列）。 */
export interface ThemeBorderColors {
  /** 主边框/分隔线（对应 colors.hairline）。 */
  hairline: string;
  /** 柔和边框（对应 colors.hairline-soft）。 */
  hairlineSoft: string;
}

/** 文字色组（对应 DESIGN.md ink/charcoal/slate/steel/stone/muted 系列）。 */
export interface ThemeTextColors {
  /** 主文字色（对应 colors.ink）。 */
  ink: string;
  /** 强调文字色（对应 colors.ink-strong，纯黑）。 */
  inkStrong: string;
  /** 正文色（对应 colors.charcoal）。 */
  charcoal: string;
  /** 次级文字色（对应 colors.slate）。 */
  slate: string;
  /** 三级文字色（对应 colors.steel）。 */
  steel: string;
  /** 弱化文字色（对应 colors.stone）。 */
  stone: string;
  /** 静音文字色（对应 colors.muted）。 */
  muted: string;
  /** 主品牌色上的文字色（与 brand.onPrimary 相同，独立暴露便于消费）。 */
  onPrimary: string;
  /** 强调色上的文字色。 */
  onAccent: string;
  /** 深色背景上的文字色。 */
  onDark: string;
}

/** 强调色组（应用主交互色，可独立于 brand.blue）。 */
export interface ThemeAccentColors {
  /** 默认强调色。 */
  default: string;
  /** hover 态强调色。 */
  hover: string;
  /** pressed 态强调色。 */
  pressed: string;
}

/** 交互态与语义色组。 */
export interface ThemeStateColors {
  /** 通用 hover 背景色（半透明覆盖）。 */
  hover: string;
  /** 成功背景色（对应 colors.success-bg）。 */
  successBg: string;
  /** 成功文字色（对应 colors.success-text）。 */
  successText: string;
  /** 错误色（用于错误边框、错误图标）。 */
  error: string;
  /** 错误背景色。 */
  errorBg: string;
  /** 警告色。 */
  warning: string;
  /** 警告背景色。 */
  warningBg: string;
  /** 信息色。 */
  info: string;
  /** 信息背景色。 */
  infoBg: string;
}

/** 阴影色组（对应 DESIGN.md Elevation 系统）。 */
export interface ThemeShadowColors {
  /** Dock 阴影（浮动的圆角矩形）。 */
  dock: string;
  /** 卡片阴影（subtle 级别）。 */
  card: string;
  /** 模态阴影（modal 级别）。 */
  modal: string;
  /** 大气阴影（atmospheric 级别，用于特色卡片）。 */
  atmospheric: string;
}

/** 完整颜色集合。 */
export interface ThemeColors {
  brand: ThemeBrandColors;
  surface: ThemeSurfaceColors;
  border: ThemeBorderColors;
  text: ThemeTextColors;
  accent: ThemeAccentColors;
  state: ThemeStateColors;
  shadow: ThemeShadowColors;
}

/** 资源引用（未来个性化主题包使用，本期内置主题包可空）。 */
export interface ThemeResources {
  /** 图片资源路径映射。 */
  images?: Record<string, string>;
  /** 字体资源路径映射。 */
  fonts?: Record<string, string>;
}

/** 主题包完整结构。 */
export interface ThemePack {
  meta: ThemePackMeta;
  colors: ThemeColors;
  /** 资源引用（可选）。 */
  resources?: ThemeResources;
  /** 继承的父主题 ID（可选，用于个性化主题包继承内置 light / dark）。 */
  extends?: ThemeId;
}
