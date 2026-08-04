/**
 * 主题管理系统 — 主题包 Schema 校验。
 *
 * 在加载主题包时执行校验，拒绝非法主题包。
 * 零第三方依赖，手写校验逻辑，支持常见 CSS 颜色格式：
 *   #hex / rgb() / rgba() / hsl() / hsla() / oklch() / 命名颜色
 */

import { TOKEN_TO_VAR } from './tokens';

// ============ 颜色值格式校验 ============

/** 16 进制颜色正则：支持 #rgb / #rgba / #rrggbb / #rrggbbaa */
const HEX_COLOR_RE = /^#([0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;

/** rgb()/rgba() 函数正则（捕获内部参数）。 */
const RGB_RE = /^rgba?\(([^)]+)\)$/i;

/** hsl()/hsla() 函数正则（捕获内部参数）。 */
const HSL_RE = /^hsla?\(([^)]+)\)$/i;

/** oklch() 函数正则（捕获内部参数）。 */
const OKLCH_RE = /^oklch\(([^)]+)\)$/i;

/**
 * 常见命名颜色集合（CSS 基本命名色）。
 * 至少包含 20 个常见命名，覆盖白/黑/红/绿/蓝等。
 */
const NAMED_COLORS = new Set([
  'white', 'black', 'red', 'green', 'blue', 'yellow', 'orange', 'purple',
  'pink', 'brown', 'gray', 'grey', 'cyan', 'magenta', 'transparent',
  'silver', 'gold', 'navy', 'maroon', 'olive', 'teal', 'lime', 'aqua',
  'fuchsia', 'indigo', 'ivory', 'khaki', 'lavender', 'linen', 'moccasin',
]);

/** 判断字符串是否为百分比（0%-100%）。 */
function isValidPercent(p: string): boolean {
  if (!p.endsWith('%')) return false;
  const n = Number(p.slice(0, -1));
  return !Number.isNaN(n) && n >= 0 && n <= 100;
}

/** 判断字符串是否为 alpha 值（0-1 或 0%-100%）。 */
function isValidAlpha(a: string): boolean {
  if (a.endsWith('%')) {
    const n = Number(a.slice(0, -1));
    return !Number.isNaN(n) && n >= 0 && n <= 100;
  }
  const n = Number(a);
  return !Number.isNaN(n) && n >= 0 && n <= 1;
}

/** 判断字符串是否为色相角度（0-360，支持 deg/turn/rad/grad 后缀）。 */
function isValidHue(h: string): boolean {
  let raw = h;
  if (raw.endsWith('deg') || raw.endsWith('turn') || raw.endsWith('rad') || raw.endsWith('grad')) {
    raw = raw.replace(/(deg|turn|rad|grad)$/, '');
  }
  const n = Number(raw);
  return !Number.isNaN(n) && n >= 0 && n <= 360;
}

/** 判断字符串是否为 rgb 分量（0-255 或 0%-100%）。 */
function isValidRgbComponent(p: string): boolean {
  if (p.endsWith('%')) {
    const n = Number(p.slice(0, -1));
    return !Number.isNaN(n) && n >= 0 && n <= 100;
  }
  const n = Number(p);
  return !Number.isNaN(n) && n >= 0 && n <= 255;
}

/** 判断字符串是否为 oklch 亮度（0%-100% 或 0-1）。 */
function isValidOklchLightness(l: string): boolean {
  if (l.endsWith('%')) {
    const n = Number(l.slice(0, -1));
    return !Number.isNaN(n) && n >= 0 && n <= 100;
  }
  const n = Number(l);
  return !Number.isNaN(n) && n >= 0 && n <= 1;
}

/** 判断字符串是否为 oklch 色度（0%-100% 或非负数值）。 */
function isValidOklchChroma(c: string): boolean {
  if (c.endsWith('%')) {
    const n = Number(c.slice(0, -1));
    return !Number.isNaN(n) && n >= 0 && n <= 100;
  }
  const n = Number(c);
  return !Number.isNaN(n) && n >= 0;
}

/**
 * 拆分颜色函数参数为「主体分量」与「可选 alpha」。
 * 支持两种语法：
 *   - 经典逗号分隔：r, g, b, a
 *   - 现代斜杠分隔：r g b / a
 * @returns [主体分量数组, alpha 或 null]
 */
function splitColorParams(params: string): [string[], string | null] {
  const trimmed = params.trim();
  if (trimmed.includes('/')) {
    const [mainPart, alphaPart] = trimmed.split('/');
    return [mainPart.trim().split(/[\s,]+/).filter(Boolean), alphaPart.trim()];
  }
  return [trimmed.split(/[\s,]+/).filter(Boolean), null];
}

/** 校验 rgb()/rgba() 参数。 */
function isValidRgbParams(params: string): boolean {
  const [parts, alpha] = splitColorParams(params);
  if (parts.length !== 3) return false;
  if (!parts.every(isValidRgbComponent)) return false;
  return alpha === null || isValidAlpha(alpha);
}

/** 校验 hsl()/hsla() 参数。 */
function isValidHslParams(params: string): boolean {
  const [parts, alpha] = splitColorParams(params);
  if (parts.length !== 3) return false;
  if (!isValidHue(parts[0])) return false;
  if (!isValidPercent(parts[1]) || !isValidPercent(parts[2])) return false;
  return alpha === null || isValidAlpha(alpha);
}

/** 校验 oklch() 参数。 */
function isValidOklchParams(params: string): boolean {
  const [parts, alpha] = splitColorParams(params);
  if (parts.length !== 3) return false;
  if (!isValidOklchLightness(parts[0])) return false;
  if (!isValidOklchChroma(parts[1])) return false;
  if (!isValidHue(parts[2])) return false;
  return alpha === null || isValidAlpha(alpha);
}

/**
 * 判断字符串是否为合法 CSS 颜色值。
 * 支持 #hex、rgb/rgba、hsl/hsla、oklch、命名颜色。
 * @param value 待校验值
 */
export function isValidColor(value: unknown): boolean {
  if (typeof value !== 'string') return false;
  const v = value.trim().toLowerCase();
  if (!v) return false;
  if (HEX_COLOR_RE.test(v)) return true;
  if (NAMED_COLORS.has(v)) return true;

  const rgbMatch = RGB_RE.exec(v);
  if (rgbMatch) return isValidRgbParams(rgbMatch[1]);

  const hslMatch = HSL_RE.exec(v);
  if (hslMatch) return isValidHslParams(hslMatch[1]);

  const oklchMatch = OKLCH_RE.exec(v);
  if (oklchMatch) return isValidOklchParams(oklchMatch[1]);

  return false;
}

// ============ 必需字段定义 ============

/**
 * 所有必需的颜色 token 路径列表。
 * 派生自 tokens.ts 的 TOKEN_TO_VAR 表，确保与 CSS 变量映射保持一致。
 */
export const REQUIRED_COLOR_PATHS: readonly string[] = Object.keys(TOKEN_TO_VAR);

/** 按分组聚合的必需 token 键映射（用于校验时遍历）。 */
const REQUIRED_KEYS_BY_GROUP: Record<string, string[]> = (() => {
  const map: Record<string, string[]> = {};
  for (const path of REQUIRED_COLOR_PATHS) {
    const [group, key] = path.split('.');
    if (!map[group]) map[group] = [];
    map[group].push(key);
  }
  return map;
})();

/**
 * 获取所有必需的 token 路径列表。
 * @returns token 路径数组（如 ['brand.primary', 'surface.canvas', ...]）
 */
export function getRequiredColorPaths(): string[] {
  return [...REQUIRED_COLOR_PATHS];
}

// ============ 主题包校验 ============

/** 判断值是否为纯对象（非 null、非数组）。 */
function isPlainObject(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

/** 将值格式化为错误信息中可展示的字符串。 */
function formatValue(value: unknown): string {
  if (typeof value === 'string') return value;
  return JSON.stringify(value);
}

/**
 * 校验主题包结构合法性。
 * 在加载主题包时执行，拒绝非法主题包。
 * @param pack 待校验的主题包（类型未知）
 * @returns { valid: 是否合法, errors: 中文错误信息数组 }
 */
export function validateThemePack(pack: unknown): { valid: boolean; errors: string[] } {
  const errors: string[] = [];

  // 校验 pack 是对象
  if (!isPlainObject(pack)) {
    errors.push('主题包必须是一个对象');
    return { valid: false, errors };
  }

  // 校验 meta
  if (!isPlainObject(pack.meta)) {
    errors.push('meta 必须是一个对象');
  } else {
    const meta = pack.meta;
    if (typeof meta.id !== 'string' || meta.id.trim() === '') {
      errors.push('meta.id 必须是非空字符串');
    }
    if (typeof meta.name !== 'string' || meta.name.trim() === '') {
      errors.push('meta.name 必须是非空字符串');
    }
    if (typeof meta.version !== 'string') {
      errors.push('meta.version 必须是字符串');
    }
    if (meta.type !== 'light' && meta.type !== 'dark') {
      errors.push('meta.type 必须是 "light" 或 "dark"');
    }
  }

  // 校验 colors
  if (!isPlainObject(pack.colors)) {
    errors.push('colors 必须是一个对象');
  } else {
    const colors = pack.colors;
    for (const [group, keys] of Object.entries(REQUIRED_KEYS_BY_GROUP)) {
      if (!isPlainObject(colors[group])) {
        errors.push(`colors.${group} 必须是一个对象`);
        continue;
      }
      const groupColors = colors[group];
      for (const key of keys) {
        const path = `${group}.${key}`;
        if (!(key in groupColors)) {
          errors.push(`colors.${path} 缺失必需的颜色字段`);
          continue;
        }
        const value = groupColors[key];
        if (!isValidColor(value)) {
          errors.push(`colors.${path} 不是合法的颜色值: ${formatValue(value)}`);
        }
      }
    }
  }

  // 可选校验：resources 若存在必须为对象
  if (pack.resources !== undefined && !isPlainObject(pack.resources)) {
    errors.push('resources 若存在必须为对象');
  }

  // 可选校验：extends 若存在必须为字符串
  if (pack.extends !== undefined && typeof pack.extends !== 'string') {
    errors.push('extends 若存在必须为字符串');
  }

  return { valid: errors.length === 0, errors };
}
