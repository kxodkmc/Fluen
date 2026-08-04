/**
 * Onboarding 模块 — 中央状态管理 composable。
 *
 * 封装 onboarding 流程的全部响应式状态（当前阶段、步骤索引、
 * 用户数据）及变更方法。组件通过返回的响应式对象只读消费状态，
 * 通过 action 回调触发状态转换。
 */

import { reactive } from 'vue';
import type {
  Language,
  OnboardingData,
  OnboardingPhase,
  OnboardingState,
  ThemeMode,
} from '../types';
import { DEFAULT_ONBOARDING_DATA } from '../constants';
import type { ProviderConfig } from '../../../types/llm';

export function useOnboardingState() {
  const state = reactive<OnboardingState>({
    phase: 'welcome' as OnboardingPhase,
    currentStep: 1,
    data: { ...DEFAULT_ONBOARDING_DATA },
  });

  /* ── 阶段转换 ──────────────────────────────────────────────────────── */
  const startStepper = (): void => {
    state.phase = 'stepper';
  };

  const completeStepper = (): void => {
    state.phase = 'complete';
  };

  const reset = (): void => {
    state.phase = 'welcome';
    state.currentStep = 1;
    state.data = { ...DEFAULT_ONBOARDING_DATA };
  };

  /* ── 数据变更 ──────────────────────────────────────────────────────── */
  const updateData = (patch: Partial<OnboardingData>): void => {
    Object.assign(state.data, patch);
  };

  const setTheme = (theme: ThemeMode): void => {
    state.data.theme = theme;
  };

  const setLanguage = (language: Language): void => {
    state.data.language = language;
  };

  const setLlmProvider = (provider: ProviderConfig | null): void => {
    state.data.llmProvider = provider;
  };

  const setActiveModelId = (modelId: string | null): void => {
    state.data.activeModelId = modelId;
  };

  const setStep = (step: number): void => {
    state.currentStep = step;
  };

  return {
    state,
    startStepper,
    completeStepper,
    reset,
    updateData,
    setTheme,
    setLanguage,
    setLlmProvider,
    setActiveModelId,
    setStep,
  };
}

export type UseOnboardingStateReturn = ReturnType<typeof useOnboardingState>;
