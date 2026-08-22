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
