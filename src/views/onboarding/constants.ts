/**
 * Onboarding 模块 — 静态配置与常量。
 *
 * 所有静态数据（步骤元数据、可选项、品牌色等）集中于此，
 * 步骤组件保持声明式，易于扩展 — 只需在此添加条目并在对应步骤中引用。
 */

import type { ProviderPreset, ThemeOption } from './types';
import { DEEPSEEK_ICON, XIAOMIMIMO_ICON, KIMI_ICON, AGNES_ICON, OPENROUTER_ICON, OPENAI_ICON } from './providerIcons';

/* ── 主题模式选项 ────────────────────────────────────────────────────── */
export const THEME_OPTIONS: ThemeOption[] = [
  { key: 'light' },
  { key: 'dark' },
];

/* ── LLM 提供商预设 ──────────────────────────────────────────────────── */

/**
 * 预设内容与后端 referee 模块（`referee-ai/src/provider/*`）保持一致：
 * 仅收录 referee 原生适配的提供商（openai / deepseek / xiaomi / moonshot / agnes / openrouter），
 * 其余 OpenAI 兼容服务通过「OpenAI 兼容」自定义入口添加。
 * 模型规格（上下文窗口 / 最大输出 / 多模态能力）以 referee 各适配器为准。
 * OpenAI / OpenRouter 为聚合或通用兼容网关，模型由用户自行填写（`models` 为空）。
 */

/** DeepSeek 预设模型：上下文 1M，最大输出 384K，均支持思考，纯文本。 */
const DEEPSEEK_MODELS = [
  {
    id: 'deepseek-v4-flash',
    name: 'DeepSeek-V4-Flash',
    contextWindow: 1_048_576,
    maxOutputTokens: 393_216,
    thinking: true,
    vision: false,
    audio: false,
    video: false,
  },
  {
    id: 'deepseek-v4-pro',
    name: 'DeepSeek-V4-Pro',
    contextWindow: 1_048_576,
    maxOutputTokens: 393_216,
    thinking: true,
    vision: false,
    audio: false,
    video: false,
  },
];

/** Xiaomi MiMo 预设模型：上下文 1M，最大输出 128K，均支持深度思考。 */
const XIAOMI_MIMO_MODELS = [
  {
    id: 'mimo-v2.5-pro',
    name: 'MiMo-V2.5-Pro',
    contextWindow: 1_048_576,
    maxOutputTokens: 131_072,
    thinking: true,
    vision: false,
    audio: false,
    video: false,
  },
  {
    id: 'mimo-v2.5',
    name: 'MiMo-V2.5',
    contextWindow: 1_048_576,
    maxOutputTokens: 131_072,
    thinking: true,
    vision: true,
    audio: true,
    video: true,
  },
];

/** Moonshot Kimi 预设模型：上下文 1M，最大输出 1M，常驻思考，支持图片。 */
const KIMI_MODELS = [
  {
    id: 'kimi-k3',
    name: 'Kimi K3',
    contextWindow: 1_048_576,
    maxOutputTokens: 1_048_576,
    thinking: true,
    vision: true,
    audio: false,
    video: false,
  },
];

/** Agnes 预设模型：上下文 512K，最大输出 64K，支持思考与图片。 */
const AGNES_MODELS = [
  {
    id: 'agnes-2.5-flash',
    name: 'Agnes 2.5 Flash',
    contextWindow: 524_288,
    maxOutputTokens: 65_536,
    thinking: true,
    vision: true,
    audio: false,
    video: false,
  },
];

export const PROVIDER_PRESETS: ProviderPreset[] = [
  {
    id: 'deepseek',
    icon: DEEPSEEK_ICON,
    isPreset: true,
    openaiBaseUrl: 'https://api.deepseek.com',
    defaultStyle: 'OpenAI',
    models: DEEPSEEK_MODELS,
  },
  {
    id: 'xiaomi',
    icon: XIAOMIMIMO_ICON,
    isPreset: true,
    openaiBaseUrl: 'https://api.xiaomimimo.com/v1',
    defaultStyle: 'OpenAI',
    models: XIAOMI_MIMO_MODELS,
  },
  {
    id: 'moonshot',
    icon: KIMI_ICON,
    isPreset: true,
    openaiBaseUrl: 'https://api.moonshot.cn/v1',
    defaultStyle: 'OpenAI',
    models: KIMI_MODELS,
  },
  {
    id: 'agnes',
    icon: AGNES_ICON,
    isPreset: true,
    openaiBaseUrl: 'https://apihub.agnes-ai.com/v1',
    defaultStyle: 'OpenAI',
    models: AGNES_MODELS,
  },
  {
    id: 'openai',
    icon: OPENAI_ICON,
    isPreset: true,
    openaiBaseUrl: 'https://api.openai.com/v1',
    defaultStyle: 'OpenAI',
    models: [], // 模型由用户自行填写
  },
  {
    id: 'openrouter',
    icon: OPENROUTER_ICON,
    isPreset: true,
    openaiBaseUrl: 'https://openrouter.ai/api/v1',
    defaultStyle: 'OpenAI',
    models: [], // 聚合网关，模型由用户自行填写
  },
  {
    id: 'openai-compatible',
    icon: '',
    isPreset: false,
    openaiBaseUrl: '',
    defaultStyle: 'OpenAI',
    models: [],
  },
];

/* ── 默认 onboarding 数据 ────────────────────────────────────────────── */
export const DEFAULT_ONBOARDING_DATA = {
  theme: 'light' as const,
  language: 'zh-CN' as const,
  llmProvider: null,
  activeModelId: null,
};
