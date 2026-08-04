/**
 * LLM 配置 CRUD composable。
 *
 * 封装 Tauri invoke 调用，提供加载、保存、查询路径三个方法。
 * 在非 Tauri 环境（如纯浏览器开发）下优雅降级，返回默认空配置。
 *
 * @example
 * ```ts
 * const { loadConfig, saveConfig, getConfigPath } = useLlmConfig();
 * const config = await loadConfig();
 * await saveConfig(newConfig);
 * ```
 */

import { invoke } from '@tauri-apps/api/core';
import type { LlmConfig } from '../types/llm';

/** 默认空配置。 */
const DEFAULT_CONFIG: LlmConfig = {
  version: '1.0.0',
  active_provider_id: null,
  active_model_id: null,
  providers: [],
};

/**
 * 检测当前是否运行在 Tauri 环境中。
 * 通过判断 `window.__TAURI_INTERNALS__` 是否存在。
 */
function isTauriEnvironment(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export function useLlmConfig() {
  /**
   * 读取完整 LLM 配置。
   *
   * 文件不存在时后端返回默认空配置。
   * 在非 Tauri 环境下返回前端默认配置。
   */
  async function loadConfig(): Promise<LlmConfig> {
    if (!isTauriEnvironment()) {
      return { ...DEFAULT_CONFIG };
    }
    try {
      return await invoke<LlmConfig>('get_llm_config');
    } catch (err) {
      console.error('[useLlmConfig] 加载配置失败:', err);
      return { ...DEFAULT_CONFIG };
    }
  }

  /**
   * 保存完整 LLM 配置（原子写入）。
   *
   * 保存前由后端校验配置完整性，校验失败时抛出错误。
   * 在非 Tauri 环境下为空操作。
   */
  async function saveConfig(config: LlmConfig): Promise<void> {
    if (!isTauriEnvironment()) {
      console.warn('[useLlmConfig] 非 Tauri 环境，跳过保存');
      return;
    }
    await invoke('save_llm_config', { config });
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
      return await invoke<string>('get_llm_config_path');
    } catch (err) {
      console.error('[useLlmConfig] 获取配置路径失败:', err);
      return '';
    }
  }

  return {
    loadConfig,
    saveConfig,
    getConfigPath,
  };
}
