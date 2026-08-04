<script setup lang="ts">
/**
 * OnboardingView — onboarding 顶层视图。
 *
 * 在 DarkVeil 背景容器内编排三个阶段（welcome → stepper → complete）。
 * 所有响应式状态由 `useOnboardingState` composable 管理；
 * 本组件负责将状态连接到当前阶段的子组件。
 *
 * 阶段转换：
 *   welcome  →  stepper   （用户点击"开始配置"）
 *   stepper  →  complete  （用户完成所有步骤，触发 LLM 配置与 App 配置保存）
 *   complete →  （进入应用 / 重置）
 *
 * 挂载时从后端加载已有 App 配置与 LLM 配置，回填到 onboarding 状态中。
 */
import { onMounted } from 'vue';
import { AnimatePresence, Motion } from 'motion-v';
import OnboardingBackground from './components/OnboardingBackground.vue';
import WelcomeScreen from './components/WelcomeScreen.vue';
import OnboardingStepper from './components/OnboardingStepper.vue';
import OnboardingComplete from './components/OnboardingComplete.vue';
import { useOnboardingState } from './composables/useOnboardingState';
import { useLlmConfig } from '../../composables/useLlmConfig';
import { useAppConfig } from '../../composables/useAppConfig';
import { useI18n } from '../../i18n';
import type { Language, ThemeMode } from './types';
import type { LlmConfig, ProviderConfig } from '../../types/llm';
import type { AppConfig } from '../../types/app';

const emit = defineEmits<{
  (e: 'enter'): void;
}>();

const { t } = useI18n();

const {
  state,
  startStepper,
  completeStepper,
  reset,
  setTheme,
  setLanguage,
  setLlmProvider,
  setActiveModelId,
  setStep,
} = useOnboardingState();

const { loadConfig: loadLlmConfig, saveConfig: saveLlmConfig } = useLlmConfig();
const { loadConfig: loadAppConfig, saveConfig: saveAppConfig } = useAppConfig();

/* ── 挂载时加载已有配置 ──────────────────────────────────────────────── */
onMounted(async () => {
  /* 加载 App 配置（主题、语言） */
  const appConfig = await loadAppConfig();
  setTheme(appConfig.theme);
  setLanguage(appConfig.language);

  /* 加载 LLM 配置 */
  const llmConfig = await loadLlmConfig();
  if (llmConfig.providers.length > 0) {
    const provider = llmConfig.providers[0];
    setLlmProvider(provider);
    setActiveModelId(llmConfig.active_model_id);
  }
});

/* ── 事件处理 ────────────────────────────────────────────────────────── */
const handleThemeUpdate = (theme: ThemeMode): void => {
  setTheme(theme);
};

const handleLanguageUpdate = (language: Language): void => {
  setLanguage(language);
};

const handleLlmProviderUpdate = (provider: ProviderConfig | null): void => {
  setLlmProvider(provider);
};

const handleActiveModelIdUpdate = (modelId: string | null): void => {
  setActiveModelId(modelId);
};

const handleStepChange = (step: number): void => {
  setStep(step);
};

const handleComplete = async (): Promise<void> => {
  /* 1. 构建 LlmConfig 并保存 */
  const provider = state.data.llmProvider;
  if (provider) {
    const llmConfig: LlmConfig = {
      version: '1.0.0',
      active_provider_id: provider.id,
      active_model_id: state.data.activeModelId,
      providers: [provider],
    };
    try {
      await saveLlmConfig(llmConfig);
    } catch (err) {
      console.error('[OnboardingView] 保存 LLM 配置失败:', err);
    }
  }

  /* 2. 构建 AppConfig 并保存（主题、语言、onboarding 标记） */
  const appConfig: AppConfig = {
    version: '1.0.0',
    theme: state.data.theme,
    language: state.data.language,
    onboarding_completed: true,
  };
  try {
    await saveAppConfig(appConfig);
  } catch (err) {
    console.error('[OnboardingView] 保存 App 配置失败:', err);
  }

  completeStepper();
};

const handleEnter = (): void => {
  emit('enter');
};

const handleReset = (): void => {
  reset();
};
</script>

<template>
  <OnboardingBackground>
    <AnimatePresence mode="wait">
      <!-- Phase 1: Welcome -->
      <WelcomeScreen
        v-if="state.phase === 'welcome'"
        key="welcome"
        @start="startStepper"
      />

      <!-- Phase 2: Stepper -->
      <Motion
        v-else-if="state.phase === 'stepper'"
        key="stepper"
        as="div"
        class="ob-stepper-wrap"
        :initial="{ opacity: 0, y: 20 }"
        :animate="{ opacity: 1, y: 0 }"
        :exit="{ opacity: 0, y: -20 }"
        :transition="{ duration: 0.4, ease: 'easeOut' }"
      >
        <OnboardingStepper
          :data="state.data"
          @step-change="handleStepChange"
          @complete="handleComplete"
          @update:theme="handleThemeUpdate"
          @update:language="handleLanguageUpdate"
          @update:llm-provider="handleLlmProviderUpdate"
          @update:active-model-id="handleActiveModelIdUpdate"
        />
      </Motion>

      <!-- Phase 3: Complete -->
      <OnboardingComplete
        v-else
        key="complete"
        :data="state.data"
        @enter="handleEnter"
      />
    </AnimatePresence>

    <!-- Reset link (visible during / after stepper) -->
    <button
      v-if="state.phase !== 'welcome'"
      class="ob-reset"
      @click="handleReset"
    >
      {{ t('onboarding.actions.reset') }}
    </button>
  </OnboardingBackground>
</template>

<style scoped>
.ob-stepper-wrap {
  width: 100%;
  max-width: 32rem;
  display: flex;
  justify-content: center;
}

.ob-reset {
  position: fixed;
  bottom: 1.5rem;
  left: 50%;
  transform: translateX(-50%);
  padding: 6px 16px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 9999px;
  background: transparent;
  color: var(--fluen-stone);
  font-family: var(--fluen-font-sans);
  font-size: 0.75rem;
  cursor: pointer;
  transition: all 0.2s ease;
  z-index: 10;
}

.ob-reset:hover {
  border-color: var(--fluen-stone);
  color: var(--fluen-charcoal);
  background: var(--fluen-hover);
}
</style>
