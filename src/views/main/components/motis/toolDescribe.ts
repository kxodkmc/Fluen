/**
 * Motis 工具调用展示辅助 — 将工具调用消息映射为「动词 + 对象」的简洁描述。
 *
 * 与组件解耦的纯函数模块：
 *   - describeToolCall：单条工具消息 → 类别 / 动词 i18n key / 对象文本
 *   - summarizeToolDisplays：一组工具描述 → 按类别计数的摘要（供分组头部展示）
 *
 * 对象文本优先取自固定名词（i18n key，如论文全文/大纲/章节），
 * 其次取原始文本（文件路径末段、检索词），未知工具回退 generic。
 */
import type { ChatMessage } from '../../types';

/** 工具类别 — 决定时间线行图标与汇总计数文案。 */
export type ToolCategory =
  | 'read'
  | 'write'
  | 'paper'
  | 'search'
  | 'manuscript'
  | 'delegate'
  | 'generic';

/** 单条工具调用的展示描述。 */
export interface ToolDisplay {
  category: ToolCategory;
  /** 时间线行动词标签的 i18n key（如「已读取」/「Read」）。 */
  verbKey: string;
  /** 分组摘要计数文案的 i18n key（含 {count} 占位符）。 */
  countKey: string;
  /** 对象原始文本（文件名、检索词等），与 objectKey 二选一。 */
  objectText?: string;
  /** 对象为固定名词时的 i18n key（优先于 objectText 展示）。 */
  objectKey?: string;
}

/** 各类别的动词标签与摘要计数 i18n key（集中维护便于扩展新类别）。 */
const CATEGORY_LABELS: Record<ToolCategory, { verbKey: string; countKey: string }> = {
  read: {
    verbKey: 'main.motisPanel.activity.verbRead',
    countKey: 'main.motisPanel.activity.countRead',
  },
  write: {
    verbKey: 'main.motisPanel.activity.verbWrite',
    countKey: 'main.motisPanel.activity.countWrite',
  },
  paper: {
    verbKey: 'main.motisPanel.activity.verbPaper',
    countKey: 'main.motisPanel.activity.countPaper',
  },
  search: {
    verbKey: 'main.motisPanel.activity.verbSearch',
    countKey: 'main.motisPanel.activity.countSearch',
  },
  manuscript: {
    verbKey: 'main.motisPanel.activity.verbManuscript',
    countKey: 'main.motisPanel.activity.countManuscript',
  },
  delegate: {
    verbKey: 'main.motisPanel.activity.verbDelegate',
    countKey: 'main.motisPanel.activity.countDelegate',
  },
  generic: {
    verbKey: 'main.motisPanel.activity.verbCall',
    countKey: 'main.motisPanel.activity.countGeneric',
  },
};

/** paper_outline 的固定名词对象 i18n key（论文大纲）。 */
const PAPER_OUTLINE_OBJECT_KEY = 'main.motisPanel.activity.paperOutline';

/** agent_id → 智能体显示名 i18n key（settings.motis.agents.*）。 */
const AGENT_NAME_KEYS: Record<string, string> = {
  academic_writer: 'settings.motis.agents.academicWriter',
  knowledge_builder: 'settings.motis.agents.knowledgeBuilder',
  data_analyst: 'settings.motis.agents.dataAnalyst',
};

/** 提取路径末段（兼容 / 与 \ 分隔符）。 */
function pathBasename(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/** 从工具输入中读取字符串字段（非对象输入返回 undefined）。 */
function inputStr(input: unknown, key: string): string | undefined {
  if (typeof input !== 'object' || input === null) return undefined;
  const value = (input as Record<string, unknown>)[key];
  return typeof value === 'string' ? value : undefined;
}

/** 按 category 取动词/计数 key 组成展示描述的基础字段。 */
function makeDisplay(category: ToolCategory, extra?: Partial<ToolDisplay>): ToolDisplay {
  return { category, ...CATEGORY_LABELS[category], ...extra };
}

/**
 * 将单条工具调用消息映射为展示描述。
 *
 * 已知工具（project_read / project_write / project_edit /
 * paper_outline / paper_section / literature_search / manuscript /
 * delegate_agent）给出精确的动词与对象；其余回退 generic。
 */
export function describeToolCall(msg: ChatMessage): ToolDisplay {
  switch (msg.toolName) {
    case 'project_read':
    case 'project_write':
    case 'project_edit': {
      const isWrite = msg.toolName !== 'project_read';
      const path = inputStr(msg.toolInput, 'path');
      return makeDisplay(isWrite ? 'write' : 'read', {
        objectText: path ? pathBasename(path) : undefined,
      });
    }
    case 'paper_outline':
      // 「已读取大纲」固定名词对象
      return makeDisplay('paper', { objectKey: PAPER_OUTLINE_OBJECT_KEY });
    case 'paper_section': {
      // 对象展示章节标题引用（如 "# 引言"）
      const heading = inputStr(msg.toolInput, 'heading')?.trim();
      return makeDisplay('paper', { objectText: heading || undefined });
    }
    case 'literature_search': {
      const query = inputStr(msg.toolInput, 'query')?.trim();
      return makeDisplay('search', { objectText: query || undefined });
    }
    case 'manuscript':
      // 「已更新正文」动词已含对象语义，无需附加对象
      return makeDisplay('manuscript');
    case 'delegate_agent': {
      // 对象展示目标智能体显示名（如「已委派 学术撰写助手」）
      const agentId = inputStr(msg.toolInput, 'agent_id');
      return makeDisplay('delegate', {
        objectKey: agentId ? AGENT_NAME_KEYS[agentId] : undefined,
      });
    }
    default:
      return makeDisplay('generic', { objectText: msg.toolName });
  }
}

/** 摘要单项 — 某类别计数的 i18n key 与次数。 */
export interface ToolSummaryPart {
  countKey: string;
  count: number;
}

/**
 * 将一组工具描述按计数 key 聚合为摘要项（保持首次出现的顺序）。
 *
 * @example
 * summarizeToolDisplays([read, search, read])
 * // → [{ countKey: '…countRead', count: 2 }, { countKey: '…countSearch', count: 1 }]
 */
export function summarizeToolDisplays(
  displays: readonly ToolDisplay[],
): ToolSummaryPart[] {
  const order: string[] = [];
  const counts = new Map<string, number>();
  for (const display of displays) {
    const key = display.countKey;
    if (!counts.has(key)) order.push(key);
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return order.map((countKey) => ({ countKey, count: counts.get(countKey)! }));
}
