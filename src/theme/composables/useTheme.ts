/**
 * 主题管理系统 — useTheme composable。
 *
 * 提供响应式的主题状态与操作方法：
 *   - currentThemeId / currentMode：跨组件共享的响应式状态（模块级单例）
 *   - setTheme(id) / setMode(mode) / toggleMode()：同步应用 DOM，异步持久化 AppConfig
 *   - initTheme()：从 AppConfig 初始化主题（应用启动时调用一次）
 *
 * 设计要点：
 *   - 模块级 ref 保证多次调用 useTheme() 共享同一响应式状态
 *   - AppConfig.theme 字段是模式（'light' | 'dark'），通过映射表转换为内置主题 id
 *   - DOM 应用同步执行（即时反馈），AppConfig 持久化异步执行（不阻塞 UI）
 */

import { ref, computed, type Ref, type ComputedRef } from 'vue';
import type { ThemePack, ThemeId, ThemeMode } from '../types';
import { themeRegistry } from '../registry';
import { applyThemeToDom, applyThemeById } from '../loader';
import { LIGHT_THEME_ID, DARK_THEME_ID } from '../packs';
import { useAppConfig } from '../../composables/useAppConfig';

/** 模式到内置主题 id 的映射。 */
const MODE_TO_THEME_ID: Record<ThemeMode, ThemeId> = {
  light: LIGHT_THEME_ID,
  dark: DARK_THEME_ID,
};

// 模块级单例响应式状态（跨组件共享）
const currentThemeId = ref<ThemeId>(LIGHT_THEME_ID);
const currentMode = ref<ThemeMode>('light');
const initialized = ref(false);

export interface UseThemeReturn {
  /** 当前主题包 id（响应式）。 */
  currentThemeId: Ref<ThemeId>;
  /** 当前主题模式（响应式，'light' | 'dark'）。 */
  currentMode: Ref<ThemeMode>;
  /** 可用主题列表（响应式）。 */
  themes: ComputedRef<ThemePack[]>;
  /** 当前主题包对象（响应式）。 */
  currentPack: ComputedRef<ThemePack | undefined>;
  /** 设置主题（按 id）。同步应用 DOM，异步持久化 AppConfig。 */
  setTheme: (id: ThemeId) => void;
  /** 设置主题模式。 */
  setMode: (mode: ThemeMode) => void;
  /** 在 light / dark 之间切换。 */
  toggleMode: () => void;
  /** 从 AppConfig 初始化主题（应用启动时调用一次）。 */
  initTheme: () => Promise<void>;
  /** 是否已初始化。 */
  initialized: Ref<boolean>;
}

export function useTheme(): UseThemeReturn {
  const themes = computed(() => themeRegistry.list());

  const currentPack = computed(() => themeRegistry.getById(currentThemeId.value));

  function setTheme(id: ThemeId): void {
    const pack = themeRegistry.getById(id);
    if (!pack) {
      throw new Error(`[theme] 主题未注册: ${id}`);
    }
    currentThemeId.value = id;
    currentMode.value = pack.meta.type;
    applyThemeToDom(pack);
    // 异步持久化到 AppConfig（不阻塞 UI）
    void persistMode(pack.meta.type);
  }

  function setMode(mode: ThemeMode): void {
    setTheme(MODE_TO_THEME_ID[mode]);
  }

  function toggleMode(): void {
    setMode(currentMode.value === 'light' ? 'dark' : 'light');
  }

  async function persistMode(mode: ThemeMode): Promise<void> {
    try {
      const { updateTheme } = useAppConfig();
      await updateTheme(mode);
    } catch (err) {
      console.error('[theme] 持久化主题失败:', err);
    }
  }

  async function initTheme(): Promise<void> {
    if (initialized.value) return;
    try {
      const { loadConfig } = useAppConfig();
      const config = await loadConfig();
      const mode = config.theme ?? 'light';
      const id = MODE_TO_THEME_ID[mode] ?? LIGHT_THEME_ID;
      currentThemeId.value = id;
      currentMode.value = mode;
      applyThemeById(id);
    } catch (err) {
      console.error('[theme] 初始化主题失败，使用默认 light:', err);
      applyThemeById(LIGHT_THEME_ID);
    } finally {
      initialized.value = true;
    }
  }

  return {
    currentThemeId,
    currentMode,
    themes,
    currentPack,
    setTheme,
    setMode,
    toggleMode,
    initTheme,
    initialized,
  };
}

