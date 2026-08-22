/**
 * Motis 设置分区 — 状态管理 composable。
 *
 * 封装 Motis 配置与运行时数据的加载、编辑、持久化逻辑。
 * 组件通过此 composable 获取响应式状态与操作方法，保持 UI 组件精简。
 */

import { ref, computed } from 'vue';
import { useLogger } from '../../../composables/useLogger';
import { useMascotConfig } from '../../../composables/useMascotConfig';
import { useMascotData } from '../../../composables/useMascotData';
import type { MascotConfig, MascotData, Mood } from '../../../types/mascot';

/* ── 人格预设 ─────────────────────────────────────────────────────────── */

/** Motis 人格选项。 */
export interface PersonalityOption {
  id: string;
  /** i18n key 后缀，完整路径为 `settings.motis.personalityOptions.${suffix}`。 */
  labelKey: string;
}

/** 内置人格选项列表。 */
const PERSONALITIES: PersonalityOption[] = [
  { id: 'cheerful', labelKey: 'cheerful' },
  { id: 'calm', labelKey: 'calm' },
  { id: 'curious', labelKey: 'curious' },
  { id: 'professional', labelKey: 'professional' },
];

/* ── 子智能体定义 ──────────────────────────────────────────────────────── */

/** Motis 可调度的子智能体选项。 */
export interface AgentOption {
  id: string;
  /** i18n key 后缀，完整路径为 `settings.motis.agents.${suffix}`。 */
  labelKey: string;
  /** 职责描述 i18n key 后缀。 */
  descKey: string;
}

/** 内置子智能体选项列表。 */
const AGENTS: AgentOption[] = [
  { id: 'academic_writer', labelKey: 'academicWriter', descKey: 'academicWriterDesc' },
  { id: 'knowledge_builder', labelKey: 'knowledgeBuilder', descKey: 'knowledgeBuilderDesc' },
  { id: 'data_analyst', labelKey: 'dataAnalyst', descKey: 'dataAnalystDesc' },
];

/* ── 状态 ───────────────────────────────────────────────────────────── */

/** Motis 配置（null = 尚未加载）。 */
const config = ref<MascotConfig | null>(null);
/** Motis 运行时数据（null = 尚未加载）。 */
const data = ref<MascotData | null>(null);
/** 是否正在加载。 */
const isLoading = ref(false);
/** 是否正在保存。 */
const isSaving = ref(false);

/* ── Composable ────────────────────────────────────────────────────── */

export function useMotisSettings() {
  const { loadConfig, saveConfig } = useMascotConfig();
  const { loadData, syncMoodFromAffinity } = useMascotData();
  /** 统一前端日志（桥接到后端同一日志文件）。 */
  const log = useLogger('motis-settings');

  /** 人格选项列表。 */
  const personalities = PERSONALITIES;

  /** 是否已启用 Motis。 */
  const enabled = computed(() => config.value?.enabled ?? false);

  /** Motis 名称。 */
  const name = computed(() => config.value?.name ?? 'Motis');

  /** 当前人格。 */
  const personality = computed(() => config.value?.personality ?? 'cheerful');

  /** 是否启用 MCP。 */
  const mcpEnabled = computed(() => config.value?.mcp_enabled ?? false);

  /** 是否启用 Skills。 */
  const skillsEnabled = computed(() => config.value?.skills_enabled ?? false);

  /** 是否启用函数调用。 */
  const functionCallingEnabled = computed(() => config.value?.function_calling_enabled ?? false);

  /** 是否展示详细思考内容。 */
  const showThinkingContent = computed(() => config.value?.show_thinking_content ?? false);

  /** 是否使用专业化表述。 */
  const professionalExpression = computed(() => config.value?.professional_expression ?? false);

  /** 已启用的子智能体 ID 列表（空列表 = 全部可用）。 */
  const enabledAgents = computed<string[]>(() => config.value?.enabled_agents ?? []);

  /** 子智能体选项列表。 */
  const agents = AGENTS;

  /** 判断指定子智能体是否启用。空列表 = 全部可用。 */
  function isAgentEnabled(agentId: string): boolean {
    const list = enabledAgents.value;
    return list.length === 0 || list.includes(agentId);
  }

  /** 当前好感度。 */
  const affinity = computed(() => data.value?.affinity ?? 0);

  /** 当前心情。 */
  const mood = computed<Mood>(() => data.value?.mood ?? 'neutral');

  /* ── 加载 ────────────────────────────────────────────────────────── */

  async function load(): Promise<void> {
    isLoading.value = true;
    try {
      config.value = await loadConfig();
      data.value = await loadData();
    } finally {
      isLoading.value = false;
    }
  }

  /* ── 配置更新 ────────────────────────────────────────────────────── */

  /** 更新配置字段并持久化。 */
  async function updateConfig(patch: Partial<MascotConfig>): Promise<void> {
    if (!config.value) return;
    config.value = { ...config.value, ...patch };
    isSaving.value = true;
    try {
      await saveConfig(config.value);
      log.debug('保存 Motis 配置', patch);
    } catch (err) {
      log.error('保存配置失败', err);
    } finally {
      isSaving.value = false;
    }
  }

  /** 切换启用状态。 */
  async function toggleEnabled(value: boolean): Promise<void> {
    await updateConfig({ enabled: value });
  }

  /** 更新名称。 */
  async function updateName(value: string): Promise<void> {
    await updateConfig({ name: value });
  }

  /** 更新人格。 */
  async function updatePersonality(value: string): Promise<void> {
    await updateConfig({ personality: value });
  }

  /** 切换 MCP。 */
  async function toggleMcp(value: boolean): Promise<void> {
    await updateConfig({ mcp_enabled: value });
  }

  /** 切换 Skills。 */
  async function toggleSkills(value: boolean): Promise<void> {
    await updateConfig({ skills_enabled: value });
  }

  /** 切换函数调用。 */
  async function toggleFunctionCalling(value: boolean): Promise<void> {
    log.info('切换工具调用总开关', { enabled: value });
    await updateConfig({ function_calling_enabled: value });
  }

  /** 切换展示详细思考内容。 */
  async function toggleShowThinking(value: boolean): Promise<void> {
    await updateConfig({ show_thinking_content: value });
  }

  /** 切换专业化表述。 */
  async function toggleProfessionalExpression(value: boolean): Promise<void> {
    await updateConfig({ professional_expression: value });
  }

  /** 切换子智能体启用状态。
   *
   * 空列表语义为「全部可用」——当用户首次操作时，
   * 将当前全部 Agent ID 写入列表作为初始状态，再执行增删。
   *
   * **联动总开关**：子智能体依赖「函数调用」工具能力——任一子智能体
   * 开启则自动打开 `function_calling_enabled`，全部关闭则随之关闭，
   * 避免出现「开了子智能体却调不到工具」的困惑。
   */
  async function toggleAgent(agentId: string, enabled: boolean): Promise<void> {
    let list = config.value?.enabled_agents ?? [];
    // 空列表 = 全部可用，首次操作时初始化为全部 ID
    if (list.length === 0) {
      list = AGENTS.map((a) => a.id);
    }
    if (enabled) {
      if (!list.includes(agentId)) {
        list = [...list, agentId];
      }
    } else {
      list = list.filter((id) => id !== agentId);
    }
    await updateConfig({
      enabled_agents: list,
      function_calling_enabled: list.length > 0,
    });
    log.info('切换子智能体', {
      agentId,
      enabled,
      enabledList: list,
      masterAuto: list.length > 0,
    });
  }

  /* ── 数据更新 ────────────────────────────────────────────────────── */

  /** 根据好感度同步心情。 */
  async function syncMood(): Promise<void> {
    if (!data.value) return;
    const newMood = await syncMoodFromAffinity();
    data.value = { ...data.value, mood: newMood };
  }

  return {
    // 状态
    config,
    data,
    isLoading,
    isSaving,
    // 计算属性
    enabled,
    name,
    personality,
    mcpEnabled,
    skillsEnabled,
    functionCallingEnabled,
    showThinkingContent,
    professionalExpression,
    enabledAgents,
    agents,
    affinity,
    mood,
    personalities,
    // 方法
    load,
    toggleEnabled,
    updateName,
    updatePersonality,
    toggleMcp,
    toggleSkills,
    toggleFunctionCalling,
    toggleShowThinking,
    toggleProfessionalExpression,
    isAgentEnabled,
    toggleAgent,
    syncMood,
  };
}
