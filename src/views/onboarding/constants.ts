/**
 * Onboarding 模块 — 静态配置与常量。
 *
 * 所有静态数据（步骤元数据、可选项、品牌色等）集中于此，
 * 步骤组件保持声明式，易于扩展 — 只需在此添加条目并在对应步骤中引用。
 */

import type { ProviderPreset, ThemeOption } from './types';
import { DEEPSEEK_ICON, XIAOMIMIMO_ICON, MINIMAX_ICON, OLLAMA_ICON, STEPFUN_ICON } from './providerIcons';

/* ── 主题模式选项 ────────────────────────────────────────────────────── */
export const THEME_OPTIONS: ThemeOption[] = [
  { key: 'light' },
  { key: 'dark' },
];

/* ── LLM 提供商预设 ──────────────────────────────────────────────────── */

/** DeepSeek 预设模型：上下文 1M，均支持思考，纯文本。 */
const DEEPSEEK_MODELS = [
  {
    id: 'deepseek-v4-flash',
    name: 'DeepSeek-V4-Flash',
    contextWindow: 1_000_000,
    maxOutputTokens: 8_192,
    thinking: true,
    vision: false,
    audio: false,
    video: false,
  },
  {
    id: 'deepseek-v4-pro',
    name: 'DeepSeek-V4-Pro',
    contextWindow: 1_000_000,
    maxOutputTokens: 8_192,
    thinking: true,
    vision: false,
    audio: false,
    video: false,
  },
];

/** Minimax 预设模型：上下文 1M，支持思考。 */
const MINIMAX_MODELS = [
  {
    id: 'MiniMax-Text-01',
    name: 'MiniMax-Text-01',
    contextWindow: 1_000_000,
    maxOutputTokens: 131_072,
    thinking: true,
    vision: false,
    audio: false,
    video: false,
  },
  {
    id: 'abab6.5s-chat',
    name: 'ABAB 6.5s Chat',
    contextWindow: 245_760,
    maxOutputTokens: 8_192,
    thinking: false,
    vision: false,
    audio: false,
    video: false,
  },
];

/** Ollama 预设模型：本地运行，上下文取决于配置。 */
const OLLAMA_MODELS = [
  {
    id: 'llama3.2',
    name: 'Llama 3.2',
    contextWindow: 128_000,
    maxOutputTokens: 8_192,
    thinking: false,
    vision: false,
    audio: false,
    video: false,
  },
  {
    id: 'qwen2.5',
    name: 'Qwen 2.5',
    contextWindow: 128_000,
    maxOutputTokens: 8_192,
    thinking: false,
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
    contextWindow: 1_000_000,
    maxOutputTokens: 131_072,
    thinking: true,
    vision: false,
    audio: false,
    video: false,
  },
  {
    id: 'mimo-v2.5',
    name: 'MiMo-V2.5',
    contextWindow: 1_000_000,
    maxOutputTokens: 131_072,
    thinking: true,
    vision: true,
    audio: true,
    video: false,
  },
];

/** StepFun 预设模型：多模态推理旗舰，256K 上下文，支持图片/视频输入与三档推理强度。 */
const STEPFUN_MODELS = [
  {
    id: 'step-3.7-flash',
    name: 'Step 3.7 Flash',
    contextWindow: 256_000,
    maxOutputTokens: 8_192,
    thinking: true,
    vision: true,
    audio: false,
    video: true,
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
    id: 'xiaomimimo',
    icon: XIAOMIMIMO_ICON,
    isPreset: true,
    openaiBaseUrl: 'https://api.xiaomimimo.com/v1',
    defaultStyle: 'OpenAI',
    models: XIAOMI_MIMO_MODELS,
  },
  {
    id: 'minimax',
    icon: MINIMAX_ICON,
    isPreset: true,
    openaiBaseUrl: 'https://api.minimax.chat/v1',
    defaultStyle: 'OpenAI',
    models: MINIMAX_MODELS,
  },
  {
    id: 'ollama',
    icon: OLLAMA_ICON,
    isPreset: true,
    openaiBaseUrl: 'http://localhost:11434/v1',
    defaultStyle: 'OpenAI',
    models: OLLAMA_MODELS,
  },
  {
    id: 'stepfun',
    icon: STEPFUN_ICON,
    isPreset: true,
    openaiBaseUrl: 'https://api.stepfun.com/v1',
    defaultStyle: 'OpenAI',
    models: STEPFUN_MODELS,
  },
  {
    id: 'stepfun-plan',
    icon: STEPFUN_ICON,
    isPreset: true,
    openaiBaseUrl: 'https://api.stepfun.com/step_plan/v1',
    defaultStyle: 'OpenAI',
    models: STEPFUN_MODELS,
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
