/**
 * 主题管理系统 — 主题加载器。
 *
 * 负责：
 *   1. 从 JSON 加载主题包（解析 + schema 校验）
 *   2. 处理 extends 继承逻辑（继承父主题作为基底再合并覆盖 token）
 *   3. 将主题包 colors 扁平化为 CSS 变量并注入到 :root 的 inline style
 *   4. 设置 :root 的 data-theme 属性
 *   5. 提供清理函数移除已注入的变量
 */

import type { ThemePack, ThemeColors, ThemeId, ThemeResources } from './types';
import { validateThemePack } from './schema';
import { flattenColors, getAllVarNames } from './tokens';
import { themeRegistry } from './registry';

/** 最大继承深度（防止循环继承）。 */
const MAX_EXTENDS_DEPTH = 5;

/**
 * 从未知数据加载主题包。
 * 接收 unknown（通常是 JSON.parse 的结果），先执行 schema 校验，再解析 extends 继承链。
 * @param raw 待加载的原始数据
 * @returns 解析完成的 ThemePack（无 extends 字段）
 * @throws 校验失败或父主题未注册时抛出 Error
 */
export function loadThemePack(raw: unknown): ThemePack {
  const result = validateThemePack(raw);
  if (!result.valid) {
    const msg = result.errors.map((e) => `  - ${e}`).join('\n');
    throw new Error(`[theme] 主题包校验失败:\n${msg}`);
  }
  const pack = raw as ThemePack;
  return resolveExtends(pack);
}

/**
 * 解析主题包的 extends 继承链。
 * - 无 extends 字段时原样返回
 * - 有 extends 时，从 registry 查询父主题并深度合并
 * - 递归处理父主题的 extends（链式继承），最大深度 MAX_EXTENDS_DEPTH
 * @param pack 待解析的主题包
 * @param depth 当前递归深度（内部使用）
 * @returns 合并后的完整 ThemePack（不再有 extends 字段）
 * @throws 父主题未注册或循环继承时抛出 Error
 */
function resolveExtends(pack: ThemePack, depth = 0): ThemePack {
  if (!pack.extends) return pack;
  if (depth >= MAX_EXTENDS_DEPTH) {
    throw new Error(`[theme] 继承深度超过 ${MAX_EXTENDS_DEPTH} 层，疑似循环继承`);
  }
  const parentId = pack.extends;
  const parent = themeRegistry.getById(parentId);
  if (!parent) {
    throw new Error(`[theme] 父主题未注册: ${parentId}`);
  }
  // 递归解析父主题的继承链
  const resolvedParent = resolveExtends(parent, depth + 1);
  // 合并 meta：子 meta 覆盖父 meta（自然保留子 meta.id 与子 meta.type）
  // 合并 colors：父主题作为基底，子主题覆盖具体 token
  // 合并 resources：子覆盖父
  return {
    meta: { ...resolvedParent.meta, ...pack.meta },
    colors: deepMergeColors(resolvedParent.colors, pack.colors),
    resources: mergeResources(resolvedParent.resources, pack.resources),
  };
}

/**
 * 深度合并两个 ThemeColors 对象（按颜色组合并）。
 * 子主题的同名 token 覆盖父主题。
 * @param base 父主题颜色
 * @param override 子主题颜色（覆盖）
 */
function deepMergeColors(base: ThemeColors, override: ThemeColors): ThemeColors {
  // 颜色组接口无索引签名，先转 unknown 再断言为通用键值结构，便于合并
  const baseRecord = base as unknown as Record<string, Record<string, string>>;
  const overrideRecord = override as unknown as Record<string, Record<string, string>>;
  const merged: Record<string, Record<string, string>> = {};
  // 父主题作为基底
  for (const group of Object.keys(baseRecord)) {
    merged[group] = { ...baseRecord[group] };
  }
  // 子主题覆盖具体 token
  for (const group of Object.keys(overrideRecord)) {
    const overrideGroup = overrideRecord[group];
    if (overrideGroup) {
      merged[group] = { ...(merged[group] || {}), ...overrideGroup };
    }
  }
  return merged as unknown as ThemeColors;
}

/**
 * 合并 resources 字段（子覆盖父，未定义时取另一边）。
 * @param base 父主题 resources
 * @param override 子主题 resources
 * @returns 合并后的 resources，若两者都为 undefined 则返回 undefined
 */
function mergeResources(
  base?: ThemeResources,
  override?: ThemeResources,
): ThemeResources | undefined {
  if (!base && !override) return undefined;
  if (!base) return override;
  if (!override) return base;
  return {
    images: { ...base.images, ...override.images },
    fonts: { ...base.fonts, ...override.fonts },
  };
}

/**
 * 将主题包 colors 扁平化为 CSS 变量并注入到 :root。
 * 调用前会先清理旧变量，避免残留。
 * 同时设置 documentElement 的 data-theme 属性为主题 id。
 * @param pack 主题包
 */
export function applyThemeToDom(pack: ThemePack): void {
  clearThemeFromDom();
  const vars = flattenColors(pack.colors);
  const rootStyle = document.documentElement.style;
  Object.entries(vars).forEach(([name, value]) => {
    rootStyle.setProperty(name, value);
  });
  document.documentElement.dataset.theme = pack.meta.id;
}

/**
 * 清理 :root 上已注入的主题 CSS 变量与 data-theme 属性。
 */
export function clearThemeFromDom(): void {
  const rootStyle = document.documentElement.style;
  getAllVarNames().forEach((name) => {
    rootStyle.removeProperty(name);
  });
  delete document.documentElement.dataset.theme;
}

/**
 * 按 id 应用已注册的主题包到 DOM。
 * @param id 主题 id
 * @returns 应用的主题包
 * @throws 主题未注册时抛出 Error
 */
export function applyThemeById(id: ThemeId): ThemePack {
  const pack = themeRegistry.getById(id);
  if (!pack) {
    throw new Error(`[theme] 主题未注册: ${id}`);
  }
  applyThemeToDom(pack);
  return pack;
}
