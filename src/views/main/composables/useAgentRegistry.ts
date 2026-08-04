/**
 * 智能体注册表 composable。
 *
 * 管理可通过搜索栏 `@` 前缀快捷调用的智能体列表。
 * 每个智能体对应一个右侧面板（RightPanelId），用户输入
 * `@<名称> <消息>` 即可展开对应面板并预填消息。
 *
 * # 扩展方式
 *
 * 在 `agents` 计算属性中添加新的 `AgentEntry` 即可。
 * 新智能体的 triggerName 应使用 i18n 翻译后的名称（专有名词除外），
 * 以确保用户在各语言下都能用本地化名称召唤。
 *
 * @example
 * ```ts
 * const { agents } = useAgentRegistry();
 * // 输入 @M → 匹配 Motis
 * const matched = agents.value.filter(a =>
 *   a.triggerName.toLowerCase().startsWith('m')
 * );
 * ```
 */

import { computed } from 'vue';
import { useI18n } from '../../../i18n';
import type { RightPanelId } from '../types';

/** 智能体条目。 */
export interface AgentEntry {
  /** 智能体 ID（对应 RightPanelId）。 */
  id: RightPanelId;
  /** 显示名称（已国际化）。 */
  label: string;
  /** 触发前缀名称（`@` 后输入的匹配文本）。 */
  triggerName: string;
  /** 图标 SVG path（24x24 viewBox）。 */
  icon: string;
  /** 简短描述（已国际化，可选）。 */
  description?: string;
}

export function useAgentRegistry() {
  const { t } = useI18n();

  /** Motis 图标（笑脸）。 */
  const MOTIS_ICON =
    'M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-3.5 6a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3zm7 0a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3zM12 17.5c-2.33 0-4.31-1.46-5.11-3.5h10.22c-.8 2.04-2.78 3.5-5.11 3.5z';

  /**
   * 已注册的智能体列表。
   *
   * 未来扩展点：在此数组中添加新的 AgentEntry 即可。
   * 新智能体的 triggerName 应使用 i18n 翻译后的名称。
   */
  const agents = computed<AgentEntry[]>(() => [
    {
      id: 'motis',
      label: t('main.rightPanel.motis'),
      // Motis 为专有名词，各语言下统一为 'Motis'
      triggerName: 'Motis',
      icon: MOTIS_ICON,
      description: t('main.agents.motisDesc'),
    },
    // ── 扩展示例 ──────────────────────────────────────────
    // {
    //   id: 'assistant',
    //   label: t('main.rightPanel.assistant'),
    //   triggerName: t('main.rightPanel.assistant'),
    //   icon: '...',
    //   description: t('main.agents.assistantDesc'),
    // },
  ]);

  return { agents };
}
