/**
 * 主界面模块 — 共享类型定义。
 *
 * 布局面板标识、活动栏项目、内容标签页等类型集中于此，
 * 供 composable 和各面板组件引用，保持类型一致性。
 */

/** 布局面板标识（从左到右三段）。 */
export type PanelId = 'function' | 'content' | 'ai';

/**
 * 编辑器视图模式。
 * - `split`：双栏（左 MD 源码 + 右 HTML 预览）
 * - `source`：仅显示 MD 源码
 * - `preview`：仅显示 HTML 预览
 */
export type EditorLayoutMode = 'split' | 'source' | 'preview';

/** 活动栏项目 — 左侧功能区中的导航条目。 */
export interface ActivityItem {
  /** 唯一标识 */
  id: string;
  /** SVG path data（24×24 viewBox） */
  icon: string;
  /**
   * 槽位：top=顶部主视图切换（默认），bottom=底部辅助入口（账户/设置等）。
   * 预留字段，便于后续扩展，无需指定时按 top 处理。
   */
  position?: 'top' | 'bottom';
}

/** 内容标签页 — 中间内容区中打开的文件或编辑器。 */
export interface ContentTab {
  /** 唯一标识 */
  id: string;
  /** 标签页标题 */
  title: string;
  /** 标签页类型 */
  type: 'file' | 'editor' | 'welcome' | 'reference' | 'wiki';
  /** SVG path data（可选，标签页图标） */
  icon?: string;
  /** 是否已修改（显示圆点指示） */
  dirty?: boolean;
  /** 当 type='reference' 时，关联的文献 ID。 */
  referenceId?: string;
  /** 当 type='wiki' 时，关联的知识库条目 ID。 */
  wikiId?: string;
}

/** AI 面板会话角色。 */
export type ChatRole = 'user' | 'assistant';

/** 消息类型 — 区分普通文本、思考过程、工具调用与状态提示。 */
export type ChatMessageKind = 'text' | 'thinking' | 'tool_call' | 'status';

/** 子智能体单次内部工具调用的活动记录。 */
export interface AgentActivity {
  /** 子智能体会话内的工具调用 ID（start/result 事件关联键）。 */
  id: string;
  /** 工具名称。 */
  name: string;
  /** 工具调用参数。 */
  input?: unknown;
  /** 工具结果（结束时填充；后端已对超长内容截断）。 */
  result?: unknown;
  /** 是否成功（结束时填充）。 */
  ok?: boolean;
  /** 执行耗时（毫秒，结束时填充）。 */
  durationMs?: number;
  /** 是否仍在执行。 */
  running: boolean;
}

/** 子智能体委派运行记录（挂在 delegate_agent 工具调用消息上）。 */
export interface AgentRun {
  /** 目标子智能体 ID（如 `essay_writing`）。 */
  agentId: string;
  /** 派发的任务描述。 */
  task: string;
  /** 运行状态。 */
  status: 'running' | 'done' | 'failed';
  /** 发起时间戳（ms）。 */
  startedAt: number;
  /** 总耗时（毫秒，结束时填充）。 */
  durationMs?: number;
  /** token 用量（成功结束时填充）。 */
  tokensUsed?: number;
  /** 失败原因（失败时填充）。 */
  error?: string;
  /**
   * 子智能体思考过程累积文本（`motis:agent-thought` 增量追加；
   * 依 MascotConfig.show_thinking_content 决定是否收集）。
   */
  thought?: string;
  /** 子智能体实时输出累积文本（`motis:agent-text` 增量追加）。 */
  text?: string;
  /**
   * 内部工具调用活动列表（按时间追加）。元素属性随事件更新、
   * 数组引用整体替换（只读数组契约，兼容 readonly 消息源）。
   */
  activities: readonly AgentActivity[];
}

/** AI 面板单条消息。 */
export interface ChatMessage {
  id: string;
  role: ChatRole;
  content: string;
  timestamp: number;
  /** 消息类型（text=正文文本，thinking=思考过程，tool_call=工具调用，status=状态提示）。 */
  kind: ChatMessageKind;
  /** 工具名称（kind='tool_call' 时使用）。 */
  toolName?: string;
  /** 工具调用输入参数（kind='tool_call' 时携带，用于前端展示）。 */
  toolInput?: unknown;
  /** 工具调用执行结果（kind='tool_call' 时由 motis:tool-result 填充）。 */
  toolResult?: unknown;
  /** 子智能体委派运行记录（toolName='delegate_agent' 时由 agent-* 事件填充）。 */
  agentRun?: AgentRun;
  /** 是否正在流式生成（增量追加中）。 */
  isStreaming?: boolean;
  /** 是否被中断（取消或出错时标记）。 */
  interrupted?: boolean;
}

/** 右侧 AI 面板标识（Motis 对话 / 助手面板）。 */
export type RightPanelId = 'motis' | 'assistant';

/** 布局面板尺寸配置。 */
export interface PanelSizes {
  /** 功能区宽度（px） */
  functionPanel: number;
  /** AI 区宽度（px） */
  aiPanel: number;
}
