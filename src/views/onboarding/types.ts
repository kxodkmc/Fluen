/**
 * Onboarding 模块 — 类型定义。
 *
 * 所有 onboarding 流程中的共享类型集中于此，
 * 供组件、composable 和步骤视图从单一来源导入。
 */

import type { ProviderConfig } from '../../types/llm';
import type { Language } from '../../i18n/types';

/** Re-export Language 供 onboarding 模块内组件/composable 统一从 types 导入。 */
export type { Language };

/** Onboarding 顶层阶段。 */
export type OnboardingPhase = 'welcome' | 'stepper' | 'complete';

/** 主题模式。 */
export type ThemeMode = 'light' | 'dark';

/** 下拉列表选项。 */
export interface DropdownOption {
  value: string;
  label: string;
  /** SVG path 内容（可选，用于提供商图标）。 */
  icon?: string;
}

/** 主题模式选项卡片。 */
export interface ThemeOption {
  key: ThemeMode;
}

/** LLM 提供商预设。 */
export interface ProviderPreset {
  /** 预设标识。 */
  id: string;
  /** SVG 图标 path（预留，暂时为空）。 */
  icon: string;
  /** 是否为预设（预设仅需填写 API Key）。 */
  isPreset: boolean;
  /** OpenAI 风格 base URL。 */
  openaiBaseUrl: string;
  /** 默认 API 风格。 */
  defaultStyle: 'OpenAI' | 'Anthropic';
  /** 预设模型列表。 */
  models: ProviderModelPreset[];
}

/** 预设模型。 */
export interface ProviderModelPreset {
  id: string;
  name: string;
  contextWindow: number;
  /** 最大输出 token 数。 */
  maxOutputTokens: number;
  thinking: boolean;
  /** 支持图片理解。 */
  vision: boolean;
  /** 支持音频理解。 */
  audio: boolean;
  /** 支持视频理解。 */
  video: boolean;
}

/** Onboarding 流程中收集的用户数据。 */
export interface OnboardingData {
  theme: ThemeMode;
  language: Language;
  llmProvider: ProviderConfig | null;
  /** 用户在步骤 2 中选中的模型 ID（对应 LlmConfig.active_model_id）。 */
  activeModelId: string | null;
}

/** `useOnboardingState` 返回的响应式状态结构。 */
export interface OnboardingState {
  phase: OnboardingPhase;
  currentStep: number;
  data: OnboardingData;
}
