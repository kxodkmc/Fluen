/**
 * 模型设置组件 — 共享类型与辅助函数。
 *
 * 汇聚供应商展示判定（预设 / 自定义、启用状态）、品牌图标解析、
 * 上下文窗口格式化等纯函数，供侧栏与面板组件复用。
 */
import { PROVIDER_PRESETS } from '../../../onboarding/constants';
import { PROVIDER_ICONS } from '../../../onboarding/providerIcons';
import type { ProviderConfig, ModelConfig } from '../../../../types/llm';

/** 常用上下文窗口档位（token 数 → 展示文案）。 */
export const CONTEXT_OPTIONS: { value: number | null; label: string }[] = [
  { value: null, label: '—' },
  { value: 131_072, label: '128K' },
  { value: 204_800, label: '200K' },
  { value: 524_288, label: '512K' },
  { value: 1_048_576, label: '1M' },
  { value: 2_097_152, label: '2M' },
];

/** 预设供应商的 API Key 获取链接（无链接的预设不展示入口）。 */
export const API_KEY_URLS: Record<string, string> = {
  deepseek: 'https://platform.deepseek.com/api_keys',
  openai: 'https://platform.openai.com/api-keys',
  moonshot: 'https://platform.moonshot.cn/console/api-keys',
  openrouter: 'https://openrouter.ai/settings/keys',
  xiaomi: 'https://xiaomimimo.com',
  agnes: 'https://apihub.agnes-ai.com',
};

/** 判断提供商是否来自预设。 */
export function isPresetProvider(provider: ProviderConfig): boolean {
  return PROVIDER_PRESETS.some((p) => p.isPreset && p.id === provider.id);
}

/** 解析提供商品牌图标（SVG path 片段，自定义提供商无图标返回空串）。 */
export function providerIcon(provider: ProviderConfig): string {
  return PROVIDER_ICONS[provider.id] ?? '';
}

/** 提供商是否处于可用状态（启用且已配置 API Key）。 */
export function providerEnabled(provider: ProviderConfig): boolean {
  return provider.enabled && !!provider.api_key;
}

/** 将 token 数格式化为紧凑展示（1_048_576 → "1M"）。 */
export function formatContext(tokens: number | null): string {
  if (!tokens) return '';
  if (tokens >= 1_000_000) return `${Math.round(tokens / 1_000_000)}M`;
  if (tokens >= 1_000) return `${Math.round(tokens / 1_000)}K`;
  return String(tokens);
}

/** 模型编辑草稿 — 编辑器表单的中间态。 */
export interface ModelDraft {
  id: string;
  name: string;
  contextWindow: number | null;
  thinking: boolean;
  vision: boolean;
}

/** ModelConfig → 编辑草稿。 */
export function draftFromModel(model: ModelConfig): ModelDraft {
  return {
    id: model.id,
    name: model.name,
    contextWindow: model.context_window,
    thinking: model.capabilities.thinking,
    vision: model.capabilities.vision,
  };
}

/** 编辑草稿 → ModelConfig（新模型默认启用流式与工具调用）。 */
export function modelFromDraft(draft: ModelDraft): ModelConfig {
  return {
    id: draft.id.trim(),
    name: draft.name.trim() || draft.id.trim(),
    capabilities: {
      thinking: draft.thinking,
      vision: draft.vision,
      audio: false,
      video: false,
      tool_calling: true,
      streaming: true,
    },
    max_output_tokens: null,
    context_window: draft.contextWindow,
    description: null,
    enabled: true,
  };
}
