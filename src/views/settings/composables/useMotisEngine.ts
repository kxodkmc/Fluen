/**
 * Motis 智慧驱动 — 状态管理 composable。
 *
 * 组合 `useLlmConfig` 与 `useMascotConfig`，为 Motis 提供独立的
 * LLM 服务商 / 模型选择与持久化逻辑。Motis 可使用与全局不同的
 * LLM 配置，未配置时回退到全局 LLM 设置。
 *
 * 暴露响应式状态与操作方法，组件仅负责 UI。
 */

import { ref, computed } from 'vue';
import { useLlmConfig } from '../../../composables/useLlmConfig';
import { useMascotConfig } from '../../../composables/useMascotConfig';
import type { LlmConfig, ProviderConfig, ModelConfig } from '../../../types/llm';
import type { MascotConfig } from '../../../types/mascot';

/* ── 模块级状态（单例，跨组件共享） ─────────────────────────────────── */

/** LLM 配置（用于列出可选 providers / models）。 */
const llmConfig = ref<LlmConfig | null>(null);
/** Motis 配置（用于读写 provider_id / model_id）。 */
const mascotConfig = ref<MascotConfig | null>(null);
/** 是否正在加载。 */
const isLoading = ref(false);
/** 是否正在保存。 */
const isSaving = ref(false);

export function useMotisEngine() {
  const { loadConfig: loadLlmConfig } = useLlmConfig();
  const { loadConfig: loadMascotConfig, saveConfig: saveMascotConfig } = useMascotConfig();

  /* ── 计算属性 ────────────────────────────────────────────────────── */

  /** 已配置的 LLM 服务商列表。 */
  const availableProviders = computed<ProviderConfig[]>(
    () => llmConfig.value?.providers ?? [],
  );

  /** 当前选中的服务商 ID（来自 MascotConfig）。 */
  const currentProviderId = computed<string | null>(
    () => mascotConfig.value?.provider_id ?? null,
  );

  /** 当前选中的服务商对象。 */
  const currentProvider = computed<ProviderConfig | null>(() =>
    availableProviders.value.find((p) => p.id === currentProviderId.value) ?? null,
  );

  /** 当前服务商下可选的模型列表。 */
  const availableModels = computed<ModelConfig[]>(
    () => currentProvider.value?.models ?? [],
  );

  /** 当前选中的模型 ID（来自 MascotConfig）。 */
  const currentModelId = computed<string | null>(
    () => mascotConfig.value?.model_id ?? null,
  );

  /** 是否没有任何已配置的 LLM 服务商。 */
  const isEmpty = computed<boolean>(() => availableProviders.value.length === 0);

  /* ── 加载 ────────────────────────────────────────────────────────── */

  /** 加载 LLM 配置与 Motis 配置。 */
  async function load(): Promise<void> {
    isLoading.value = true;
    try {
      const [llm, mascot] = await Promise.all([loadLlmConfig(), loadMascotConfig()]);
      llmConfig.value = llm;
      mascotConfig.value = mascot;
      // 清理脏数据：provider 或 model 已不存在于 LLM 配置中时回退到全局激活项
      const providerExists =
        !mascot.provider_id ||
        llm.providers.some((p) => p.id === mascot.provider_id);
      const modelExists =
        !providerExists ||
        !mascot.model_id ||
        llm.providers
          .find((p) => p.id === mascot.provider_id)
          ?.models.some((m) => m.id === mascot.model_id) === true;
      if (!providerExists || !modelExists) {
        mascotConfig.value = { ...mascot, provider_id: null, model_id: null };
        await saveMascotConfig(mascotConfig.value);
      }
    } finally {
      isLoading.value = false;
    }
  }

  /* ── 持久化 ──────────────────────────────────────────────────────── */

  /** 保存 Motis 配置。 */
  async function persist(): Promise<void> {
    if (!mascotConfig.value) return;
    isSaving.value = true;
    try {
      await saveMascotConfig(mascotConfig.value);
    } catch (err) {
      console.error('[useMotisEngine] 保存配置失败:', err);
    } finally {
      isSaving.value = false;
    }
  }

  /* ── 选择操作 ────────────────────────────────────────────────────── */

  /**
   * 选择服务商。
   * 切换服务商时清空已选模型（模型 ID 在不同服务商间不通用）。
   */
  async function selectProvider(providerId: string | null): Promise<void> {
    if (!mascotConfig.value) return;
    if (providerId === currentProviderId.value) return;
    mascotConfig.value = {
      ...mascotConfig.value,
      provider_id: providerId,
      model_id: null,
    };
    await persist();
  }

  /** 选择模型。 */
  async function selectModel(modelId: string | null): Promise<void> {
    if (!mascotConfig.value) return;
    if (modelId === currentModelId.value) return;
    mascotConfig.value = { ...mascotConfig.value, model_id: modelId };
    await persist();
  }

  return {
    // 状态
    isLoading,
    isSaving,
    // 计算属性
    availableProviders,
    availableModels,
    currentProviderId,
    currentModelId,
    isEmpty,
    // 方法
    load,
    selectProvider,
    selectModel,
  };
}
