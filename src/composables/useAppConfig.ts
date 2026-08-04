/**
 * App 配置 CRUD composable。
 *
 * 封装 Tauri invoke 调用，提供加载、保存、查询路径三个方法。
 * 后端内置内存缓存，读取性能优于每次磁盘 I/O。
 * 在非 Tauri 环境下优雅降级，返回默认配置或使用 localStorage。
 *
 * @example
 * ```ts
 * const { loadConfig, saveConfig, getConfigPath } = useAppConfig();
 * const config = await loadConfig();
 * await saveConfig(newConfig);
 * ```
 */

import { invoke } from '@tauri-apps/api/core';
import type { AppConfig, Language, ThemeMode } from '../types/app';
import { i18nInstance } from '../i18n';

/** 默认配置。 */
const DEFAULT_CONFIG: AppConfig = {
  version: '1.0.0',
  theme: 'light',
  language: 'zh-CN',
  onboarding_completed: false,
};

/**
 * 检测当前是否运行在 Tauri 环境中。
 * 通过判断 `window.__TAURI_INTERNALS__` 是否存在。
 */
function isTauriEnvironment(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export function useAppConfig() {
  /**
   * 读取 App 配置（后端优先从内存缓存返回）。
   *
   * 文件不存在时后端返回默认配置。
   * 在非 Tauri 环境下从 localStorage 读取或返回默认配置。
   */
  async function loadConfig(): Promise<AppConfig> {
    if (!isTauriEnvironment()) {
      return loadFromLocalStorage();
    }
    try {
      return await invoke<AppConfig>('get_app_config');
    } catch (err) {
      console.error('[useAppConfig] 加载配置失败:', err);
      return { ...DEFAULT_CONFIG };
    }
  }

  /**
   * 保存 App 配置（原子写入 + 更新后端缓存）。
   *
   * 在非 Tauri 环境下保存到 localStorage。
   */
  async function saveConfig(config: AppConfig): Promise<void> {
    if (!isTauriEnvironment()) {
      saveToLocalStorage(config);
      return;
    }
    await invoke('save_app_config', { config });
  }

  /**
   * 获取配置文件的完整路径（供前端展示）。
   * 在非 Tauri 环境下返回空字符串。
   */
  async function getConfigPath(): Promise<string> {
    if (!isTauriEnvironment()) {
      return '';
    }
    try {
      return await invoke<string>('get_app_config_path');
    } catch (err) {
      console.error('[useAppConfig] 获取配置路径失败:', err);
      return '';
    }
  }

  /**
   * 仅更新主题（读取当前配置 → 合并 → 保存）。
   */
  async function updateTheme(theme: ThemeMode): Promise<void> {
    const config = await loadConfig();
    await saveConfig({ ...config, theme });
  }

  /**
   * 仅更新语言（读取当前配置 → 合并 → 保存）。
   *
   * 保存配置后同步 i18n 实例（直接引用 i18nInstance 避免循环依赖）。
   */
  async function updateLanguage(language: Language): Promise<void> {
    const config = await loadConfig();
    await saveConfig({ ...config, language });
    i18nInstance.global.locale.value = language;
  }

  /**
   * 标记 onboarding 已完成。
   */
  async function markOnboardingCompleted(): Promise<void> {
    const config = await loadConfig();
    await saveConfig({ ...config, onboarding_completed: true });
  }

  return {
    loadConfig,
    saveConfig,
    getConfigPath,
    updateTheme,
    updateLanguage,
    markOnboardingCompleted,
  };
}

// ---------------------------------------------------------------------------
// localStorage 降级实现（非 Tauri 环境）
// ---------------------------------------------------------------------------

const LS_KEY = 'fluen.app_config';

function loadFromLocalStorage(): AppConfig {
  try {
    const raw = localStorage.getItem(LS_KEY);
    if (raw) {
      return { ...DEFAULT_CONFIG, ...JSON.parse(raw) };
    }
  } catch {
    // ignore parse errors
  }
  return { ...DEFAULT_CONFIG };
}

function saveToLocalStorage(config: AppConfig): void {
  try {
    localStorage.setItem(LS_KEY, JSON.stringify(config));
  } catch {
    // ignore quota errors
  }
}
