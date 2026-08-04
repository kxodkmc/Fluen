/**
 * MarksOverlay postMessage 协议类型。
 *
 * 定义外层 Vue 与 iframe（MarksOverlay 脚本）之间的双向消息。
 * iframe 因 sandbox 限制，仅通过 postMessage 与外层通信。
 *
 * ## 消息流向
 *
 * ```text
 * 外层 Vue ──→ iframe
 *   marks:render    渲染高亮（含 marks 列表）
 *   marks:clear     清除所有高亮
 *   marks:remove    移除单条高亮
 *   marks:scroll    滚动到指定高亮
 *
 * iframe ──→ 外层 Vue
 *   selection:create  用户选区完成（请求创建标记）
 *   mark:click        用户点击高亮（请求跳转/编辑）
 *   marks:ready       iframe 已就绪（可接收 marks:render）
 * ```
 *
 * @module reader/marks/types
 */

import type { BlockType, MarkColor, MarkStatus } from '../../../../../types/marks';

// ---------------------------------------------------------------------------
// 序列化形式（postMessage 载荷，扁平化便于跨 iframe 传输）
// ---------------------------------------------------------------------------

/**
 * 序列化的选区锚点——iframe 上报给外层的选区信息。
 *
 * `block_key` 为字符串形式（`"{type}:{line}:{occurrence}"`），
 * 外层据此从 BlockMap 查找完整 BlockKey + Fingerprint。
 */
export interface SerializedAnchor {
  /** 块复合键字符串（`"{type}:{line}:{occurrence}"`）。 */
  block_key: string;
  /** 块类型。 */
  block_type: BlockType;
  /** MD 源码行号。 */
  source_line: number;
  /** 同 (type, line) 出现序号。 */
  occurrence: number;
  /** 选区起始字符偏移（块内）。 */
  start_offset: number;
  /** 选区结束字符偏移（块内）。 */
  end_offset: number;
  /** 整个块的纯文本（外层用于校验指纹，可选）。 */
  block_text: string;
  /** 选中的文本（与外层 Mark.text 一致）。 */
  selected_text: string;
}

/**
 * 序列化的标记——外层下发给 iframe 渲染高亮。
 *
 * 仅含渲染所需字段，不含附注/时间戳等元数据。
 */
export interface SerializedMark {
  id: string;
  /** 块复合键字符串。 */
  block_key: string;
  /** 块内起始偏移。 */
  start_offset: number;
  /** 块内结束偏移。 */
  end_offset: number;
  /** 划线文本（用于校验偏移仍正确）。 */
  text: string;
  color: MarkColor;
  status: MarkStatus;
}

// ---------------------------------------------------------------------------
// 外层 → iframe 消息
// ---------------------------------------------------------------------------

/** 渲染高亮（替换全部）。 */
export interface MarksRenderMessage {
  type: 'marks:render';
  marks: SerializedMark[];
}

/** 清除所有高亮。 */
export interface MarksClearMessage {
  type: 'marks:clear';
}

/** 移除单条高亮（不重渲染全部，性能更优）。 */
export interface MarksRemoveMessage {
  type: 'marks:remove';
  markId: string;
}

/** 滚动到指定高亮。 */
export interface MarksScrollMessage {
  type: 'marks:scroll';
  markId: string;
}

/** 外层 → iframe 消息联合类型。 */
export type OuterToInnerMessage =
  | MarksRenderMessage
  | MarksClearMessage
  | MarksRemoveMessage
  | MarksScrollMessage;

// ---------------------------------------------------------------------------
// iframe → 外层消息
// ---------------------------------------------------------------------------

/** 选区在 iframe 视口内的 bounding rect（外层定位颜色选择浮层用）。 */
export interface SelectionRect {
  left: number;
  top: number;
  right: number;
  bottom: number;
  width: number;
  height: number;
}

/** 用户选区完成，请求创建标记。 */
export interface SelectionCreateMessage {
  type: 'selection:create';
  anchor: SerializedAnchor;
  text: string;
  /** 选区 bounding rect（iframe 视口坐标，跨块时省略）。 */
  rect?: SelectionRect;
}

/** 用户点击已有高亮。 */
export interface MarkClickMessage {
  type: 'mark:click';
  markId: string;
}

/** iframe 已就绪，可接收 marks:render。 */
export interface MarksReadyMessage {
  type: 'marks:ready';
}

/** iframe → 外层消息联合类型。 */
export type InnerToOuterMessage =
  | SelectionCreateMessage
  | MarkClickMessage
  | MarksReadyMessage;

// ---------------------------------------------------------------------------
// 类型守卫
// ---------------------------------------------------------------------------

/** 判断消息是否来自 MarksOverlay（iframe 内）。 */
export function isInnerMessage(data: unknown): data is InnerToOuterMessage {
  if (!data || typeof data !== 'object') return false;
  const t = (data as { type?: unknown }).type;
  return (
    t === 'selection:create' ||
    t === 'mark:click' ||
    t === 'marks:ready'
  );
}
