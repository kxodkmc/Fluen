/**
 * AI 服务配置 CRUD composable。
 *
 * 封装 Tauri invoke 调用，提供加载、保存、查询路径三个方法。
 * 在非 Tauri 环境（如纯浏览器开发）下优雅降级，返回默认空配置。
 *
 * @example
 * ```ts
 * const { loadConfig, saveConfig, getConfigPath } = useAiServices();
 * const config = await loadConfig();
 * await saveConfig(newConfig);
 * ```
 */

import { invoke } from '@tauri-apps/api/core';
import type { AiServicesConfig } from '../types/aiServices';

/** 默认空配置。 */
const DEFAULT_CONFIG: AiServicesConfig = {
  version: '1.0.0',
  active_providers: {},
  providers: [],
  default_reference_import_mode: 'ocr',
};

/**
 * 检测当前是否运行在 Tauri 环境中。
 */
function isTauriEnvironment(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export function useAiServices() {
  /**
   * 读取完整 AI 服务配置。
   *
   * 文件不存在时后端返回默认空配置。
   */
  async function loadConfig(): Promise<AiServicesConfig> {
    if (!isTauriEnvironment()) {
      return { ...DEFAULT_CONFIG };
    }
    try {
      return await invoke<AiServicesConfig>('get_ai_services_config');
    } catch (err) {
      console.error('[useAiServices] 加载配置失败:', err);
      return { ...DEFAULT_CONFIG };
    }
  }

  /**
   * 保存完整 AI 服务配置（原子写入）。
   *
   * 保存前由后端校验配置完整性，校验失败时抛出错误。
   */
  async function saveConfig(config: AiServicesConfig): Promise<void> {
    if (!isTauriEnvironment()) {
      console.warn('[useAiServices] 非 Tauri 环境，跳过保存');
      return;
    }
    await invoke('save_ai_services_config', { config });
  }

  /**
   * 获取配置文件的完整路径（供前端展示）。
   */
  async function getConfigPath(): Promise<string> {
    if (!isTauriEnvironment()) {
      return '';
    }
    try {
      return await invoke<string>('get_ai_services_config_path');
    } catch (err) {
      console.error('[useAiServices] 获取配置路径失败:', err);
      return '';
    }
  }

  return {
    loadConfig,
    saveConfig,
    getConfigPath,
  };
}
