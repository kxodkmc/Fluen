/**
 * 主题管理系统 — 主题注册表。
 *
 * 提供主题包的注册、查询、列举、注销能力：
 *   - 内置主题包（fluen-light / fluen-dark）在模块加载时自动注册，受保护不可注销
 *   - 支持未来用户自定义主题包动态注册
 *   - 模块级单例 `themeRegistry` 供全局使用，亦可通过 `createThemeRegistry()` 创建独立实例
 */

import type { ThemePack, ThemeId, ThemeMode } from './types';
import { BUILTIN_THEME_PACKS, LIGHT_THEME_ID, DARK_THEME_ID } from './packs';

/**
 * 内置主题包 id 集合（受保护，不可注销或覆盖）。
 * 用于在 unregister / clear 时跳过内置包。
 */
export const BUILTIN_THEME_IDS: ReadonlySet<string> = new Set<string>([
  LIGHT_THEME_ID,
  DARK_THEME_ID,
]);

/**
 * 主题注册表。
 *
 * 以 Map<ThemeId, ThemePack> 维护已注册主题包，保证按 id 的 O(1) 查询。
 * 构造时自动注册所有内置主题包。
 */
export class ThemeRegistry {
  /** 已注册主题包映射（按 id 索引）。 */
  private readonly packs = new Map<ThemeId, ThemePack>();

  constructor() {
    // 自动注册内置主题包
    for (const pack of BUILTIN_THEME_PACKS) {
      this.register(pack);
    }
  }

  /**
   * 注册主题包。
   * 若 id 已存在抛出 Error；内置主题包 id 同样不可被覆盖。
   * @param pack 待注册的主题包
   */
  register(pack: ThemePack): void {
    const id = pack.meta.id;
    if (this.packs.has(id)) {
      throw new Error(`[theme] 主题包 id 已存在: ${id}`);
    }
    this.packs.set(id, pack);
  }

  /**
   * 按 id 查询主题包。
   * @returns 命中则返回 ThemePack，否则返回 undefined
   */
  getById(id: ThemeId): ThemePack | undefined {
    return this.packs.get(id);
  }

  /**
   * 判断指定 id 是否已注册。
   */
  has(id: ThemeId): boolean {
    return this.packs.has(id);
  }

  /**
   * 返回所有已注册主题包列表。
   * 返回新数组，外部修改不影响内部状态。
   */
  list(): ThemePack[] {
    return Array.from(this.packs.values());
  }

  /**
   * 按主题模式（light / dark）过滤已注册主题包。
   */
  listByMode(mode: ThemeMode): ThemePack[] {
    return this.list().filter((pack) => pack.meta.type === mode);
  }

  /**
   * 注销主题包。
   * - 内置主题包不可注销，抛出 Error
   * - 其余 id 注销成功返回 true，id 不存在返回 false
   */
  unregister(id: ThemeId): boolean {
    if (BUILTIN_THEME_IDS.has(id)) {
      throw new Error(`[theme] 内置主题包不可注销: ${id}`);
    }
    return this.packs.delete(id);
  }

  /**
   * 清空所有用户主题包，保留内置主题包。
   */
  clear(): void {
    for (const id of this.packs.keys()) {
      if (!BUILTIN_THEME_IDS.has(id)) {
        this.packs.delete(id);
      }
    }
  }
}

/**
 * 创建独立主题注册表实例（用于测试或独立场景）。
 * 新实例会自动注册内置主题包。
 */
export function createThemeRegistry(): ThemeRegistry {
  return new ThemeRegistry();
}

/** 全局主题注册表单例。 */
export const themeRegistry: ThemeRegistry = new ThemeRegistry();
