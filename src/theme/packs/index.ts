/**
 * 主题管理系统 — 内置主题包。
 */
import type { ThemePack } from '../types';
import lightPack from './light.json';
import darkPack from './dark.json';

/** 内置主题包列表。 */
export const BUILTIN_THEME_PACKS: ThemePack[] = [lightPack as ThemePack, darkPack as ThemePack];

/** 内置主题包按 id 索引。 */
export const BUILTIN_THEME_PACK_MAP: Record<string, ThemePack> = {
  [lightPack.meta.id]: lightPack as ThemePack,
  [darkPack.meta.id]: darkPack as ThemePack,
};

/** 内置 light 主题包 id。 */
export const LIGHT_THEME_ID = 'fluen-light';

/** 内置 dark 主题包 id。 */
export const DARK_THEME_ID = 'fluen-dark';
