/**
 * 主题管理系统 — Token 映射与扁平化工具。
 *
 * 将主题包 colors 对象扁平化为 CSS 变量键值对。
 * 命名规则：colors.brand.coral → --fluen-brand-coral
 *          colors.surface.canvas → --fluen-canvas（保留语义化短名）
 *          colors.text.ink → --fluen-ink
 *          colors.accent.default → --fluen-accent
 */

import type { ThemeColors } from './types';

/** CSS 变量前缀。 */
export const CSS_VAR_PREFIX = '--fluen';

/**
 * token 路径到 CSS 变量名的映射表。
 * 显式声明而非自动转换，确保命名稳定可预测。
 * 键格式：'group.key'，值格式：'variable-name'（不含前缀）。
 */
export const TOKEN_TO_VAR: Record<string, string> = {
  // brand
  'brand.primary': 'primary',
  'brand.onPrimary': 'on-primary',
  'brand.primarySoft': 'primary-soft',
  'brand.coral': 'brand-coral',
  'brand.magenta': 'brand-magenta',
  'brand.blue': 'brand-blue',
  'brand.blueMid': 'brand-blue-mid',
  'brand.blueDeep': 'brand-blue-deep',
  'brand.blue700': 'brand-blue-700',
  'brand.cyan': 'brand-cyan',
  'brand.blue200': 'brand-blue-200',
  'brand.purple': 'brand-purple',
  // surface
  'surface.canvas': 'canvas',
  'surface.surface': 'surface',
  'surface.surfaceDeep': 'surface-deep',
  'surface.surfaceSoft': 'surface-soft',
  // border
  'border.hairline': 'hairline',
  'border.hairlineSoft': 'hairline-soft',
  // text
  'text.ink': 'ink',
  'text.inkStrong': 'ink-strong',
  'text.charcoal': 'charcoal',
  'text.slate': 'slate',
  'text.steel': 'steel',
  'text.stone': 'stone',
  'text.muted': 'muted',
  'text.onPrimary': 'on-primary',
  'text.onAccent': 'on-accent',
  'text.onDark': 'on-dark',
  // accent
  'accent.default': 'accent',
  'accent.hover': 'accent-hover',
  'accent.pressed': 'accent-pressed',
  // state
  'state.hover': 'hover',
  'state.successBg': 'success-bg',
  'state.successText': 'success-text',
  'state.error': 'error',
  'state.errorBg': 'error-bg',
  'state.warning': 'warning',
  'state.warningBg': 'warning-bg',
  'state.info': 'info',
  'state.infoBg': 'info-bg',
  // shadow
  'shadow.dock': 'shadow-dock',
  'shadow.card': 'shadow-card',
  'shadow.modal': 'shadow-modal',
  'shadow.atmospheric': 'shadow-atmospheric',
};

/**
 * 将 token 路径转换为完整 CSS 变量名。
 * @param path token 路径，如 'brand.coral'
 * @returns 完整 CSS 变量名，如 '--fluen-brand-coral'
 */
export function tokenPathToVarName(path: string): string {
  const suffix = TOKEN_TO_VAR[path];
  if (!suffix) {
    throw new Error(`[theme] 未知的 token 路径: ${path}`);
  }
  return `${CSS_VAR_PREFIX}-${suffix}`;
}

/**
 * 将 ThemeColors 对象扁平化为 CSS 变量键值对。
 * @param colors 主题颜色集合
 * @returns 键为完整 CSS 变量名（如 '--fluen-brand-coral'），值为颜色字符串
 */
export function flattenColors(colors: ThemeColors): Record<string, string> {
  const result: Record<string, string> = {};

  (Object.keys(colors) as Array<keyof ThemeColors>).forEach((group) => {
    // 先转 unknown 再断言：颜色组接口无索引签名，直接断言会触发 TS2352
    const groupColors = colors[group] as unknown as Record<string, string>;
    Object.keys(groupColors).forEach((key) => {
      const path = `${group}.${key}`;
      const varName = tokenPathToVarName(path);
      result[varName] = groupColors[key];
    });
  });

  return result;
}

/**
 * 获取所有已注册的 CSS 变量名列表（用于清理 DOM 上的主题变量）。
 * @returns CSS 变量名数组
 */
export function getAllVarNames(): string[] {
  return Object.values(TOKEN_TO_VAR).map((suffix) => `${CSS_VAR_PREFIX}-${suffix}`);
}
