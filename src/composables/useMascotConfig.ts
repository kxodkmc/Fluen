/**
 * 宠物助手配置 CRUD composable。
 *
 * 封装 Tauri invoke 调用，提供加载、保存、查询路径三个方法。
 * 后端内置内存缓存，读取性能优于每次磁盘 I/O。
 * 在非 Tauri 环境下优雅降级，返回默认配置或使用 localStorage。
 *
 * @example
 * ```ts
 * const { loadConfig, saveConfig, getConfigPath } = useMascotConfig();
 * const config = await loadConfig();
 * await saveConfig(newConfig);
 * ```
 */

import { invoke } from '@tauri-apps/api/core';
import type { MascotConfig } from '../types/mascot';

/** 默认配置。 */
const DEFAULT_CONFIG: MascotConfig = {
  version: '1.0.0',
  name: 'Motis',
  enabled: true,
  provider_id: null,
  model_id: null,
  mcp_enabled: false,
  skills_enabled: false,
  function_calling_enabled: false,
  personality: 'cheerful',
  show_thinking_content: false,
  professional_expression: false,
};

/**
 * 检测当前是否运行在 Tauri 环境中。
 * 通过判断 `window.__TAURI_INTERNALS__` 是否存在。
 */
function isTauriEnvironment(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export function useMascotConfig() {
  /**
   * 读取宠物助手配置（后端优先从内存缓存返回）。
   *
   * 文件不存在时后端返回默认配置。
   * 在非 Tauri 环境下从 localStorage 读取或返回默认配置。
   */
  async function loadConfig(): Promise<MascotConfig> {
    if (!isTauriEnvironment()) {
      return loadFromLocalStorage();
    }
    try {
      return await invoke<MascotConfig>('get_mascot_config');
    } catch (err) {
      console.error('[useMascotConfig] 加载配置失败:', err);
      return { ...DEFAULT_CONFIG };
    }
  }

  /**
   * 保存宠物助手配置（原子写入 + 更新后端缓存）。
   *
   * 在非 Tauri 环境下保存到 localStorage。
   */
  async function saveConfig(config: MascotConfig): Promise<void> {
    if (!isTauriEnvironment()) {
      saveToLocalStorage(config);
      return;
    }
    await invoke('save_mascot_config', { config });
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
      return await invoke<string>('get_mascot_config_path');
    } catch (err) {
      console.error('[useMascotConfig] 获取配置路径失败:', err);
      return '';
    }
  }

  return {
    loadConfig,
    saveConfig,
    getConfigPath,
  };
}

// ---------------------------------------------------------------------------
// localStorage 降级实现（非 Tauri 环境）
// ---------------------------------------------------------------------------

const LS_KEY = 'fluen.mascot_config';

function loadFromLocalStorage(): MascotConfig {
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

function saveToLocalStorage(config: MascotConfig): void {
  try {
    localStorage.setItem(LS_KEY, JSON.stringify(config));
  } catch {
    // ignore quota errors
  }
}
