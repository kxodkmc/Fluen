/**
 * 宠物助手运行时数据 CRUD composable。
 *
 * 封装 Tauri invoke 调用，提供加载、保存、查询路径三个基础方法，
 * 以及心情 / 好感度联动业务逻辑。
 * 在非 Tauri 环境下优雅降级，返回默认数据或使用 localStorage。
 *
 * @example
 * ```ts
 * const { loadData, saveData, updateMood, updateAffinity, syncMoodFromAffinity } = useMascotData();
 * await updateMood('happy');
 * const newAffinity = await updateAffinity(10);
 * ```
 */

import { invoke } from '@tauri-apps/api/core';
import type { MascotData, Mood } from '../types/mascot';

/** 默认数据。 */
const DEFAULT_DATA: MascotData = {
  version: '1.0.0',
  mood: 'neutral',
  affinity: 0,
  mood_updated_at: null,
  affinity_updated_at: null,
};

/**
 * 检测当前是否运行在 Tauri 环境中。
 * 通过判断 `window.__TAURI_INTERNALS__` 是否存在。
 */
function isTauriEnvironment(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** 将好感度钳制到 0-100 范围内。 */
function clampAffinity(value: number): number {
  return Math.max(0, Math.min(100, value));
}

/** 根据好感度推导心情：≥70 → happy，30-69 → neutral，<30 → sad。 */
function deriveMood(affinity: number): Mood {
  if (affinity >= 70) return 'happy';
  if (affinity >= 30) return 'neutral';
  return 'sad';
}

export function useMascotData() {
  /**
   * 读取宠物助手运行时数据。
   *
   * 文件不存在时后端返回默认数据。
   * 在非 Tauri 环境下从 localStorage 读取或返回默认数据。
   */
  async function loadData(): Promise<MascotData> {
    if (!isTauriEnvironment()) {
      return loadFromLocalStorage();
    }
    try {
      return await invoke<MascotData>('get_mascot_data');
    } catch (err) {
      console.error('[useMascotData] 加载数据失败:', err);
      return { ...DEFAULT_DATA };
    }
  }

  /**
   * 保存宠物助手运行时数据（原子写入）。
   *
   * 在非 Tauri 环境下保存到 localStorage。
   */
  async function saveData(data: MascotData): Promise<void> {
    if (!isTauriEnvironment()) {
      saveToLocalStorage(data);
      return;
    }
    await invoke('save_mascot_data', { data });
  }

  /**
   * 获取数据文件的完整路径（供前端展示）。
   * 在非 Tauri 环境下返回空字符串。
   */
  async function getDataPath(): Promise<string> {
    if (!isTauriEnvironment()) {
      return '';
    }
    try {
      return await invoke<string>('get_mascot_data_path');
    } catch (err) {
      console.error('[useMascotData] 获取数据路径失败:', err);
      return '';
    }
  }

  /**
   * 仅更新心情（读取当前数据 → 更新 mood + mood_updated_at → 保存）。
   */
  async function updateMood(mood: Mood): Promise<void> {
    const data = await loadData();
    await saveData({
      ...data,
      mood,
      mood_updated_at: new Date().toISOString(),
    });
  }

  /**
   * 增减好感度（读取当前数据 → 钳制 0-100 → 更新 affinity + affinity_updated_at → 保存）。
   *
   * @returns 更新后的好感度值
   */
  async function updateAffinity(delta: number): Promise<number> {
    const data = await loadData();
    const next = clampAffinity(data.affinity + delta);
    await saveData({
      ...data,
      affinity: next,
      affinity_updated_at: new Date().toISOString(),
    });
    return next;
  }

  /**
   * 根据好感度同步心情（读取当前数据 → 推导 mood → 更新 mood + mood_updated_at → 保存）。
   *
   * @returns 推导出的心情
   */
  async function syncMoodFromAffinity(): Promise<Mood> {
    const data = await loadData();
    const mood = deriveMood(data.affinity);
    await saveData({
      ...data,
      mood,
      mood_updated_at: new Date().toISOString(),
    });
    return mood;
  }

  return {
    loadData,
    saveData,
    getDataPath,
    updateMood,
    updateAffinity,
    syncMoodFromAffinity,
  };
}

// ---------------------------------------------------------------------------
// localStorage 降级实现（非 Tauri 环境）
// ---------------------------------------------------------------------------

const LS_KEY = 'fluen.mascot_data';

function loadFromLocalStorage(): MascotData {
  try {
    const raw = localStorage.getItem(LS_KEY);
    if (raw) {
      return { ...DEFAULT_DATA, ...JSON.parse(raw) };
    }
  } catch {
    // ignore parse errors
  }
  return { ...DEFAULT_DATA };
}

function saveToLocalStorage(data: MascotData): void {
  try {
    localStorage.setItem(LS_KEY, JSON.stringify(data));
  } catch {
    // ignore quota errors
  }
}
