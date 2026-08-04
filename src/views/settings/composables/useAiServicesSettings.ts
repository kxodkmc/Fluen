/**
 * AI 服务设置分区 — 状态管理 composable。
 *
 * 封装 AI 服务配置（OCR / TTS / ASR）的加载、编辑、保存逻辑。
 * 当前 MVP 仅支持 PaddleOCR 提供商的配置管理，但接口设计预留扩展空间。
 *
 * 组件通过此 composable 获取响应式状态与操作方法，保持 UI 组件精简。
 */

import { ref, computed } from 'vue';
import { useAiServices } from '../../../composables/useAiServices';
import type {
  AiServicesConfig,
  AiServiceProvider,
  PaddleOcrConfig,
} from '../../../types/aiServices';

/* ── PaddleOCR 默认配置 ─────────────────────────────────────────────── */

/** PaddleOCR (AIStudio) 默认提供商配置。 */
const DEFAULT_PADDLEOCR_PROVIDER: Omit<AiServiceProvider, 'created_at' | 'updated_at'> = {
  id: 'paddleocr-aistudio',
  name: 'PaddleOCR (AIStudio)',
  category: 'ocr',
  deployment: 'api',
  api_base_url: 'https://paddleocr.aistudio-app.com/api/v2/ocr/jobs',
  api_key: null,
  auth_scheme: 'bearer',
  provider_config: {
    api_mode: 'job',
    options: {
      use_doc_orientation_classify: false,
      use_doc_unwarping: false,
      use_chart_recognition: false,
    },
  },
  models: [
    {
      id: 'PaddleOCR-VL-1.6',
      name: 'PaddleOCR VL 1.6',
      enabled: true,
    },
  ],
  active_model_id: 'PaddleOCR-VL-1.6',
  enabled: true,
};

/* ── 状态 ───────────────────────────────────────────────────────────── */

/** 加载的 AI 服务配置。 */
const config = ref<AiServicesConfig | null>(null);
/** 当前选中编辑的提供商 ID。 */
const selectedProviderId = ref<string | null>(null);
/** 是否正在加载。 */
const isLoading = ref(false);
/** 是否正在保存。 */
const isSaving = ref(false);
/** 错误信息。 */
const error = ref<string | null>(null);

export function useAiServicesSettings() {
  const { loadConfig, saveConfig } = useAiServices();

  /* ── 计算属性 ─────────────────────────────────────────────────────── */

  /** 所有提供商列表。 */
  const providers = computed<AiServiceProvider[]>(() => config.value?.providers ?? []);

  /** OCR 类型的提供商列表。 */
  const ocrProviders = computed(() =>
    providers.value.filter((p) => p.category === 'ocr'),
  );

  /** 当前激活的 OCR 提供商 ID。 */
  const activeOcrProviderId = computed(() =>
    config.value?.active_providers?.ocr ?? null,
  );

  /** 当前选中的提供商对象。 */
  const selectedProvider = computed<AiServiceProvider | null>(() =>
    providers.value.find((p) => p.id === selectedProviderId.value) ?? null,
  );

  /** 获取提供商的 PaddleOCR 专属配置。 */
  function getPaddleOcrConfig(provider: AiServiceProvider): PaddleOcrConfig {
    if (!provider.provider_config) {
      return { api_mode: 'job', options: {
        use_doc_orientation_classify: false,
        use_doc_unwarping: false,
        use_chart_recognition: false,
      }};
    }
    return provider.provider_config as unknown as PaddleOcrConfig;
  }

  /* ── 加载 ────────────────────────────────────────────────────────── */

  async function load(): Promise<void> {
    isLoading.value = true;
    error.value = null;
    try {
      config.value = await loadConfig();
      // 如果没有 OCR 提供商，自动创建默认的 PaddleOCR 提供商
      if (ocrProviders.value.length === 0) {
        ensureDefaultOcrProvider();
      }
      // 默认选中第一个 OCR 提供商
      if (!selectedProviderId.value && ocrProviders.value.length > 0) {
        selectedProviderId.value = ocrProviders.value[0].id;
      }
    } finally {
      isLoading.value = false;
    }
  }

  /** 确保 config 中存在默认的 PaddleOCR 提供商（不自动保存）。 */
  function ensureDefaultOcrProvider(): void {
    if (!config.value) return;
    const now = new Date().toISOString();
    const provider: AiServiceProvider = {
      ...DEFAULT_PADDLEOCR_PROVIDER,
      created_at: now,
      updated_at: now,
    };
    config.value.providers.push(provider);
    // 自动设为活跃 OCR 提供商
    if (!config.value.active_providers.ocr) {
      config.value.active_providers.ocr = provider.id;
    }
  }

  /* ── 编辑 ────────────────────────────────────────────────────────── */

  /** 选中提供商进行编辑。 */
  function selectProvider(providerId: string): void {
    selectedProviderId.value =
      selectedProviderId.value === providerId ? null : providerId;
  }

  /** 更新提供商的 API Key。 */
  async function updateApiKey(providerId: string, apiKey: string): Promise<void> {
    if (!config.value) return;
    const provider = config.value.providers.find((p) => p.id === providerId);
    if (provider) {
      provider.api_key = apiKey || null;
      provider.updated_at = new Date().toISOString();
      await persist();
    }
  }

  /** 更新提供商的 Base URL。 */
  async function updateBaseUrl(providerId: string, baseUrl: string): Promise<void> {
    if (!config.value) return;
    const provider = config.value.providers.find((p) => p.id === providerId);
    if (provider) {
      provider.api_base_url = baseUrl || null;
      provider.updated_at = new Date().toISOString();
      await persist();
    }
  }

  /** 更新 PaddleOCR API 模式。 */
  async function updateApiMode(
    providerId: string,
    apiMode: 'job' | 'sync',
  ): Promise<void> {
    if (!config.value) return;
    const provider = config.value.providers.find((p) => p.id === providerId);
    if (!provider || !provider.provider_config) return;

    const paddleConfig = getPaddleOcrConfig(provider);
    paddleConfig.api_mode = apiMode;
    provider.provider_config = { ...paddleConfig } as unknown as Record<string, unknown>;
    provider.updated_at = new Date().toISOString();
    await persist();
  }

  /** 更新 PaddleOCR 识别选项。 */
  async function updateOcrOptions(
    providerId: string,
    options: Partial<PaddleOcrConfig['options']>,
  ): Promise<void> {
    if (!config.value) return;
    const provider = config.value.providers.find((p) => p.id === providerId);
    if (!provider || !provider.provider_config) return;

    const paddleConfig = getPaddleOcrConfig(provider);
    paddleConfig.options = { ...paddleConfig.options, ...options };
    provider.provider_config = { ...paddleConfig } as unknown as Record<string, unknown>;
    provider.updated_at = new Date().toISOString();
    await persist();
  }

  /** 设置激活的 OCR 提供商。 */
  async function setActiveOcrProvider(providerId: string): Promise<void> {
    if (!config.value) return;
    config.value.active_providers.ocr = providerId;
    await persist();
  }

  /** 设置激活的模型。 */
  async function setActiveModel(providerId: string, modelId: string): Promise<void> {
    if (!config.value) return;
    const provider = config.value.providers.find((p) => p.id === providerId);
    if (provider) {
      provider.active_model_id = modelId;
      provider.updated_at = new Date().toISOString();
      await persist();
    }
  }

  /* ── 持久化 ──────────────────────────────────────────────────────── */

  async function persist(): Promise<void> {
    if (!config.value) return;
    isSaving.value = true;
    error.value = null;
    try {
      await saveConfig(config.value);
    } catch (err) {
      const msg = typeof err === 'string' ? err : String(err);
      error.value = msg;
      console.error('[useAiServicesSettings] 保存配置失败:', err);
    } finally {
      isSaving.value = false;
    }
  }

  return {
    // 状态
    config,
    providers,
    ocrProviders,
    activeOcrProviderId,
    selectedProvider,
    selectedProviderId,
    isLoading,
    isSaving,
    error,
    // 方法
    load,
    selectProvider,
    updateApiKey,
    updateBaseUrl,
    updateApiMode,
    updateOcrOptions,
    setActiveOcrProvider,
    setActiveModel,
    getPaddleOcrConfig,
  };
}
