/**
 * LLM 设置分区 — 状态管理 composable。
 *
 * 封装 LLM 配置的加载、添加/编辑/删除提供商、设置激活项及持久化逻辑。
 * 组件通过此 composable 获取响应式状态与操作方法，保持 UI 组件精简。
 */

import { ref, computed } from 'vue';
import { useLlmConfig } from '../../../composables/useLlmConfig';
import { useI18n } from '../../../i18n';
import type { LlmConfig, ProviderConfig, ModelConfig, SceneModels, SceneModelRef, EmbeddingConfig } from '../../../types/llm';

/** 场景化模型槽位 key（与后端 `SceneModels` 字段一一对应）。 */
export type SceneModelKey = 'knowledge_build';

/* ── 预设数据（与 onboarding 共享结构） ─────────────────────────────── */

interface PresetModel {
  id: string;
  name: string;
  contextWindow: number;
  maxOutputTokens: number;
  thinking: boolean;
  vision: boolean;
  audio: boolean;
}

interface ProviderPreset {
  id: string;
  icon: string;
  isPreset: boolean;
  openaiBaseUrl: string;
  defaultStyle: 'OpenAI' | 'Anthropic';
  models: PresetModel[];
}

// 导入 onboarding 的预设常量以复用
import {
  PROVIDER_PRESETS,
} from '../../onboarding/constants';
import type { ProviderPreset as OnboardingPreset } from '../../onboarding/types';

/** 将 onboarding 的预设转换为本模块使用的结构。 */
const PRESETS: ProviderPreset[] = PROVIDER_PRESETS.map((p: OnboardingPreset) => ({
  id: p.id,
  icon: p.icon,
  isPreset: p.isPreset,
  openaiBaseUrl: p.openaiBaseUrl,
  defaultStyle: p.defaultStyle,
  models: p.models,
}));

/* ── 状态 ───────────────────────────────────────────────────────────── */

/** 加载的 LLM 配置。 */
const config = ref<LlmConfig | null>(null);
/** 当前选中编辑的提供商 ID。 */
const selectedProviderId = ref<string | null>(null);
/** 是否处于「添加提供商」模式。 */
const isAdding = ref(false);
/** 添加模式下选中的预设 ID。 */
const selectedPresetId = ref<string>('');
/** 是否正在保存。 */
const isSaving = ref(false);
/** 是否正在加载。 */
const isLoading = ref(false);

/** 生成 16 位 UUID4（前端临时 ID，后端可重新分配）。 */
function generateId(): string {
  return crypto.randomUUID().replace(/-/g, '').slice(0, 16);
}

/** 从预设模型构建能力标志。 */
function buildCapabilities(model: Pick<PresetModel, 'thinking' | 'vision' | 'audio'>) {
  return {
    thinking: model.thinking,
    vision: model.vision,
    audio: model.audio,
    video: false,
    tool_calling: true,
    streaming: true,
  };
}

/** 将名称转换为 slug。 */
function slugify(text: string): string {
  return text
    .toLowerCase()
    .trim()
    .replace(/[^\w\s-]/g, '')
    .replace(/[\s_-]+/g, '-')
    .replace(/^-+|-+$/g, '');
}

/* ── Composable ────────────────────────────────────────────────────── */

export function useLlmSettings() {
  const { t } = useI18n();
  const { loadConfig, saveConfig } = useLlmConfig();

  /** 已配置的提供商列表。 */
  const providers = computed(() => config.value?.providers ?? []);

  /** 当前选中的提供商对象。 */
  const selectedProvider = computed(() =>
    providers.value.find((p) => p.id === selectedProviderId.value) ?? null,
  );

  /** 激活的提供商 ID。 */
  const activeProviderId = computed(() => config.value?.active_provider_id ?? null);

  /** 激活的模型 ID。 */
  const activeModelId = computed(() => config.value?.active_model_id ?? null);

  /** 预设列表（供添加时选择）。 */
  const presets = PRESETS;

  /** 当前选中的预设对象。 */
  const selectedPreset = computed(() =>
    PRESETS.find((p) => p.id === selectedPresetId.value) ?? null,
  );

  /** 获取预设的显示名称。 */
  function getPresetLabel(presetId: string): string {
    return t(`onboarding.steps.llmConfig.providerPresets.${presetId === 'openai-compatible' ? 'openaiCompatible' : presetId}`);
  }

  /* ── 加载 ────────────────────────────────────────────────────────── */

  async function load(): Promise<void> {
    isLoading.value = true;
    try {
      config.value = await loadConfig();
    } finally {
      isLoading.value = false;
    }
  }

  /* ── 添加提供商 ──────────────────────────────────────────────────── */

  /** 开始添加提供商流程。 */
  function startAdding(): void {
    isAdding.value = true;
    selectedPresetId.value = '';
    selectedProviderId.value = null;
  }

  /** 取消添加。 */
  function cancelAdding(): void {
    isAdding.value = false;
    selectedPresetId.value = '';
  }

  /** 从预设构建 ProviderConfig。 */
  function buildProviderFromPreset(preset: ProviderPreset, apiKey: string): ProviderConfig {
    const models: ModelConfig[] = preset.models.map((m) => ({
      id: m.id,
      name: m.name,
      capabilities: buildCapabilities(m),
      max_output_tokens: m.maxOutputTokens,
      context_window: m.contextWindow,
      description: null,
      enabled: true,
    }));

    return {
      id: preset.id,
      name: getPresetLabel(preset.id),
      provider_type: 'custom',
      openai_base_url: preset.openaiBaseUrl,
      anthropic_base_url: null,
      api_key: apiKey || null,
      default_style: preset.defaultStyle,
      extra_headers: {},
      enabled: true,
      models,
      created_at: null,
      updated_at: null,
    };
  }

  /** 添加预设提供商。 */
  async function addPresetProvider(preset: ProviderPreset, apiKey: string, modelId: string): Promise<void> {
    if (!config.value) return;
    const provider = buildProviderFromPreset(preset, apiKey);
    // 替换同 ID 的已有提供商
    config.value.providers = config.value.providers.filter((p) => p.id !== provider.id);
    config.value.providers.push(provider);
    // 如果没有激活提供商，自动设为激活
    if (!config.value.active_provider_id) {
      config.value.active_provider_id = provider.id;
    }
    if (!config.value.active_model_id && modelId) {
      config.value.active_model_id = modelId;
    }
    isAdding.value = false;
    selectedPresetId.value = '';
    await persist();
  }

  /** 添加自定义提供商。 */
  async function addCustomProvider(params: {
    name: string;
    baseUrl: string;
    apiKey: string;
    modelId: string;
    modelName: string;
  }): Promise<void> {
    if (!config.value) return;
    const id = slugify(params.name) || `custom-${generateId()}`;
    const provider: ProviderConfig = {
      id,
      name: params.name || 'Custom Provider',
      provider_type: 'custom',
      openai_base_url: params.baseUrl || null,
      anthropic_base_url: null,
      api_key: params.apiKey || null,
      default_style: 'OpenAI',
      extra_headers: {},
      enabled: true,
      models: params.modelId
        ? [{
            id: params.modelId,
            name: params.modelName || params.modelId,
            capabilities: buildCapabilities({ thinking: false, vision: false, audio: false }),
            max_output_tokens: null,
            context_window: null,
            description: null,
            enabled: true,
          }]
        : [],
      created_at: null,
      updated_at: null,
    };
    config.value.providers.push(provider);
    if (!config.value.active_provider_id) {
      config.value.active_provider_id = id;
    }
    if (!config.value.active_model_id && params.modelId) {
      config.value.active_model_id = params.modelId;
    }
    isAdding.value = false;
    selectedPresetId.value = '';
    await persist();
  }

  /* ── 编辑提供商 ──────────────────────────────────────────────────── */

  /** 更新提供商的 API Key。 */
  async function updateApiKey(providerId: string, apiKey: string): Promise<void> {
    if (!config.value) return;
    const provider = config.value.providers.find((p) => p.id === providerId);
    if (provider) {
      provider.api_key = apiKey || null;
      await persist();
    }
  }

  /** 设置激活的提供商和模型。 */
  async function setActive(providerId: string, modelId: string | null): Promise<void> {
    if (!config.value) return;
    config.value.active_provider_id = providerId;
    config.value.active_model_id = modelId;
    await persist();
  }

  /* ── 场景化模型槽位 ──────────────────────────────────────────────── */

  /** 场景化模型配置集合（响应式）。 */
  const sceneModels = computed<SceneModels | null>(
    () => config.value?.scene_models ?? null,
  );

  /**
   * 设置某场景的专用模型。
   * 若该场景原无配置，会自动创建 `scene_models` 容器。
   */
  async function setSceneModel(scene: SceneModelKey, ref: SceneModelRef): Promise<void> {
    if (!config.value) return;
    if (!config.value.scene_models) {
      config.value.scene_models = {};
    }
    config.value.scene_models[scene] = ref;
    await persist();
  }

  /** 清除某场景的专用模型（回退到全局激活项）。 */
  async function clearSceneModel(scene: SceneModelKey): Promise<void> {
    if (!config.value?.scene_models) return;
    if (config.value.scene_models[scene] == null) return;
    config.value.scene_models[scene] = null;
    // 容器为空时整体清空，避免持久化空对象
    const kb = config.value.scene_models.knowledge_build;
    if (!kb) {
      config.value.scene_models = null;
    }
    await persist();
  }

  /* ── Embedding 配置 ─────────────────────────────────────────────── */

  /** Embedding 配置（响应式，始终非 null，缺失时返回默认值）。 */
  const embeddingConfig = computed<EmbeddingConfig>(
    () => config.value?.embedding ?? { enabled: true, model_ref: null },
  );

  /** 设置 Embedding 启用状态。 */
  async function setEmbeddingEnabled(enabled: boolean): Promise<void> {
    if (!config.value) return;
    if (!config.value.embedding) {
      config.value.embedding = { enabled, model_ref: null };
    } else {
      config.value.embedding.enabled = enabled;
    }
    await persist();
  }

  /**
   * 设置 Embedding 自定义模型引用。
   * 传 `null` 表示使用内置免费提供商。
   */
  async function setEmbeddingModel(ref: SceneModelRef | null): Promise<void> {
    if (!config.value) return;
    if (!config.value.embedding) {
      config.value.embedding = { enabled: true, model_ref: ref };
    } else {
      config.value.embedding.model_ref = ref;
    }
    await persist();
  }

  /** 删除提供商。 */
  async function removeProvider(providerId: string): Promise<void> {
    if (!config.value) return;
    config.value.providers = config.value.providers.filter((p) => p.id !== providerId);
    if (config.value.active_provider_id === providerId) {
      config.value.active_provider_id = config.value.providers[0]?.id ?? null;
      config.value.active_model_id = null;
    }
    if (selectedProviderId.value === providerId) {
      selectedProviderId.value = null;
    }
    await persist();
  }

  /** 选中提供商进行编辑。 */
  function selectProvider(providerId: string): void {
    selectedProviderId.value = selectedProviderId.value === providerId ? null : providerId;
    isAdding.value = false;
  }

  /* ── 持久化 ──────────────────────────────────────────────────────── */

  async function persist(): Promise<void> {
    if (!config.value) return;
    isSaving.value = true;
    try {
      await saveConfig(config.value);
    } catch (err) {
      console.error('[useLlmSettings] 保存配置失败:', err);
    } finally {
      isSaving.value = false;
    }
  }

  return {
    // 状态
    config,
    providers,
    selectedProvider,
    selectedProviderId,
    activeProviderId,
    activeModelId,
    sceneModels,
    embeddingConfig,
    isAdding,
    isSaving,
    isLoading,
    selectedPresetId,
    selectedPreset,
    presets,
    // 方法
    load,
    startAdding,
    cancelAdding,
    addPresetProvider,
    addCustomProvider,
    updateApiKey,
    setActive,
    setSceneModel,
    clearSceneModel,
    setEmbeddingEnabled,
    setEmbeddingModel,
    removeProvider,
    selectProvider,
    getPresetLabel,
  };
}
