<script setup lang="ts">
/**
 * OnboardingStepper — 使用 Stepper 预设组件包裹 onboarding 专有配置
 *（按钮文案），并将三个步骤组件作为默认插槽子项组合。
 *
 * 强调色从 useTheme 实时读取，确保主题切换时 Stepper 指示器与按钮同步。
 * 数据通过 props 从父级向下传递；步骤级变更通过事件双向绑定，
 * 使父级 composable 保持唯一数据源。
 */
import { computed } from 'vue';
import { Stepper } from '../../../presets';
import { useI18n } from '../../../i18n';
import { useTheme } from '../../../theme';
import type { Language, OnboardingData, ThemeMode } from '../types';
import type { ProviderConfig } from '../../../types/llm';
import StepAppearance from '../steps/StepAppearance.vue';
import StepLlmConfig from '../steps/StepLlmConfig.vue';
import StepTBD from '../steps/StepTBD.vue';

defineProps<{
  data: OnboardingData;
}>();

const emit = defineEmits<{
  (e: 'step-change', step: number): void;
  (e: 'complete'): void;
  (e: 'update:theme', value: ThemeMode): void;
  (e: 'update:language', value: Language): void;
  (e: 'update:llmProvider', value: ProviderConfig | null): void;
  (e: 'update:activeModelId', value: string | null): void;
}>();

const { t } = useI18n();
const { currentPack } = useTheme();
const accentColor = computed(() => currentPack.value?.colors.accent.default ?? '#3b82f6');
const accentColorHover = computed(() => currentPack.value?.colors.accent.hover ?? '#1d4ed8');
</script>

<template>
  <Stepper
    :initial-step="1"
    :accent-color="accentColor"
    :accent-color-hover="accentColorHover"
    :back-button-text="t('onboarding.stepper.back')"
    :next-button-text="t('onboarding.stepper.next')"
    :complete-button-text="t('onboarding.stepper.complete')"
    @step-change="emit('step-change', $event)"
    @final-step-completed="emit('complete')"
  >
    <!-- Step 1: 主题模式与语言 -->
    <StepAppearance
      :theme="data.theme"
      :language="data.language"
      @update:theme="emit('update:theme', $event)"
      @update:language="emit('update:language', $event)"
    />

    <!-- Step 2: LLM 服务配置 -->
    <StepLlmConfig
      :provider="data.llmProvider"
      :active-model-id="data.activeModelId"
      @update:provider="emit('update:llmProvider', $event)"
      @update:active-model-id="emit('update:activeModelId', $event)"
    />

    <!-- Step 3: 暂定占位 -->
    <StepTBD />
  </Stepper>
</template>
