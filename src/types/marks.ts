/**
 * 文献标记（marks）类型定义。
 *
 * 与 Rust 后端 `references::marks` 一一对应，
 * 序列化 / 反序列化格式遵循 serde 默认规则（snake_case）。
 *
 * 复用 reader.ts 中的 BlockKey / Fingerprint / BlockType（同一概念）。
 *
 * @see src-tauri/src/references/marks.rs
 */

import type { BlockKey, Fingerprint } from './reader';

// 重导出共享类型，方便调用方单点引入
export type { BlockKey, BlockType, Fingerprint } from './reader';

// ---------------------------------------------------------------------------
// 标记颜色与状态
// ---------------------------------------------------------------------------

/** 标记颜色（对应 Rust `MarkColor`）。 */
export type MarkColor = 'yellow' | 'green' | 'blue' | 'pink';

/** 标记状态（对应 Rust `MarkStatus`）。 */
export type MarkStatus = 'active' | 'migrated' | 'degraded' | 'orphaned';

/** 全部可选颜色（用于颜色选择器）。 */
export const MARK_COLORS: MarkColor[] = ['yellow', 'green', 'blue', 'pink'];

// ---------------------------------------------------------------------------
// 选区范围与锚点
// ---------------------------------------------------------------------------

/** 块内字符偏移范围（半开区间：`[start_offset, end_offset)`）。 */
export interface TextRange {
  start_offset: number;
  end_offset: number;
}

/** 标记锚点——定位选区在文档中的位置。 */
export interface MarkAnchor {
  /** 块复合键。 */
  block_key: BlockKey;
  /** 块内容指纹（迁移校验）。 */
  block_fingerprint: Fingerprint;
  /** 块内字符偏移。 */
  range: TextRange;
}

// ---------------------------------------------------------------------------
// 标记条目
// ---------------------------------------------------------------------------

/** 单条标记（对应 Rust `Mark`）。 */
export interface Mark {
  /** 标记 ID（`mk-{16位hex}`）。 */
  id: string;
  /** 所属文献 ID。 */
  reference_id: string;
  /** 锚点。 */
  anchor: MarkAnchor;
  /** 划线文本快照（列表展示用）。 */
  text: string;
  /** 颜色。 */
  color: MarkColor;
  /** 附注（可选）。 */
  note?: string | null;
  /** 状态。 */
  status: MarkStatus;
  /** 最近一次解析时间（RFC 3339）。 */
  last_resolved_at: string;
  /** 创建时间（RFC 3339）。 */
  created_at: string;
  /** 最近更新时间（RFC 3339）。 */
  updated_at: string;
}
