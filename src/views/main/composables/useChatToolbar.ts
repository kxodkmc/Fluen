/**
 * 聊天底部工具栏状态管理 composable。
 *
 * 模块级单例，跨 Motis / 学术助手面板共享以下状态：
 *   - 当前模式（motis / assistant）
 *   - 附加文件列表
 *   - 思考强度
 *
 * 模型选择直接读取全局 LLM 配置并调用 setActive 持久化。
 * 工具栏组件仅负责展示与事件发射，所有业务逻辑集中于此。
 */

import { ref, computed, onMounted } from 'vue';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { useLlmSettings } from '../../settings/composables/useLlmSettings';
import type { RightPanelId } from '../types';

/** 思考强度选项。 */
export type ThinkingIntensity = 'default' | 'deep' | 'light';

/** 工具栏可用模型条目（跨提供商扁平化）。 */
export interface ToolbarModelOption {
  /** 全局唯一标识：provider_id::model_id */
  id: string;
  /** 显示名称。 */
  name: string;
  /** 提供商名称。 */
  providerName: string;
  /** 提供商 ID。 */
  providerId: string;
  /** 模型 ID。 */
  modelId: string;
}

const STORAGE_KEY = 'fluen.chat_toolbar';

/* ── 模块级状态（单例） ───────────────────────────────────────────────── */

/** 当前聊天模式。 */
const mode = ref<RightPanelId>('motis');
/** 附加文件路径列表。 */
const attachments = ref<string[]>([]);
/** 思考强度。 */
const thinkingIntensity = ref<ThinkingIntensity>('default');
/** 是否正在加载 LLM 配置。 */
const isLoadingModels = ref(false);

interface PersistedState {
  mode?: RightPanelId;
  thinkingIntensity?: ThinkingIntensity;
}

function loadPersistedState(): PersistedState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) return JSON.parse(raw) as PersistedState;
  } catch {
    // ignore
  }
  return {};
}

function savePersistedState(): void {
  try {
    const payload: PersistedState = {
      mode: mode.value,
      thinkingIntensity: thinkingIntensity.value,
    };
    localStorage.setItem(STORAGE_KEY, JSON.stringify(payload));
  } catch {
    // ignore
  }
}

export function useChatToolbar() {
  const llm = useLlmSettings();

  /* ── 初始化 ───────────────────────────────────────────────────────── */

  onMounted(() => {
    const persisted = loadPersistedState();
    if (persisted.mode) mode.value = persisted.mode;
    if (persisted.thinkingIntensity) thinkingIntensity.value = persisted.thinkingIntensity;
    void loadModels();
  });

  /** 加载 LLM 配置（用于模型下拉）。 */
  async function loadModels(): Promise<void> {
    isLoadingModels.value = true;
    try {
      await llm.load();
    } finally {
      isLoadingModels.value = false;
    }
  }

  /* ── 模型列表 ─────────────────────────────────────────────────────── */

  /** 将所有提供商的模型扁平化为下拉选项。 */
  const modelOptions = computed<ToolbarModelOption[]>(() => {
    const options: ToolbarModelOption[] = [];
    for (const provider of llm.providers.value) {
      if (!provider.enabled) continue;
      for (const model of provider.models) {
        if (!model.enabled) continue;
        options.push({
          id: `${provider.id}::${model.id}`,
          name: model.name,
          providerName: provider.name,
          providerId: provider.id,
          modelId: model.id,
        });
      }
    }
    return options;
  });

  /** 当前激活模型选项。 */
  const activeModelOption = computed<ToolbarModelOption | null>(() => {
    const providerId = llm.activeProviderId.value;
    const modelId = llm.activeModelId.value;
    if (!providerId || !modelId) return null;
    return (
      modelOptions.value.find((m) => m.providerId === providerId && m.modelId === modelId) ?? null
    );
  });

  /** 当前模型显示名称（用于工具栏按钮）。 */
  const activeModelLabel = computed<string>(() => {
    if (isLoadingModels.value) return '…';
    if (activeModelOption.value) return activeModelOption.value.name;
    if (modelOptions.value.length === 0) return '未配置';
    return '选择模型';
  });

  /** 切换激活模型。 */
  async function selectModel(option: ToolbarModelOption): Promise<void> {
    await llm.setActive(option.providerId, option.modelId);
  }

  /* ── 模式 ─────────────────────────────────────────────────────────── */

  function setMode(next: RightPanelId): void {
    if (mode.value === next) return;
    mode.value = next;
    savePersistedState();
  }

  /* ── 思考强度 ─────────────────────────────────────────────────────── */

  function setThinkingIntensity(next: ThinkingIntensity): void {
    if (thinkingIntensity.value === next) return;
    thinkingIntensity.value = next;
    savePersistedState();
  }

  /** 由 v-model 使用的 setter 包装。 */
  const modeModel = computed<RightPanelId>({
    get: () => mode.value,
    set: setMode,
  });

  const thinkingIntensityModel = computed<ThinkingIntensity>({
    get: () => thinkingIntensity.value,
    set: setThinkingIntensity,
  });

  /* ── 附件 ─────────────────────────────────────────────────────────── */

  /** 通过系统对话框选择文件并添加到附件列表。 */
  async function addAttachment(): Promise<void> {
    try {
      const selected = await openDialog({ multiple: true });
      if (!selected) return;
      const paths = Array.isArray(selected) ? selected : [selected];
      for (const path of paths) {
        if (path && !attachments.value.includes(path)) {
          attachments.value.push(path);
        }
      }
    } catch (err) {
      console.error('[useChatToolbar] 选择文件失败:', err);
    }
  }

  function removeAttachment(path: string): void {
    attachments.value = attachments.value.filter((p) => p !== path);
  }

  function clearAttachments(): void {
    attachments.value = [];
  }

  return {
    // 状态
    mode: modeModel,
    attachments,
    thinkingIntensity: thinkingIntensityModel,
    isLoadingModels,
    modelOptions,
    activeModelOption,
    activeModelLabel,

    // 方法
    loadModels,
    selectModel,
    setMode,
    setThinkingIntensity,
    addAttachment,
    removeAttachment,
    clearAttachments,
  };
}

export type UseChatToolbarReturn = ReturnType<typeof useChatToolbar>;
