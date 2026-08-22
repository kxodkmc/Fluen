/**
 * 文献阅读器类型定义。
 *
 * 与 Rust 后端 `references::reader` 一一对应，
 * 序列化 / 反序列化格式遵循 serde 默认规则（snake_case）。
 *
 * @see src-tauri/src/references/reader.rs
 */

import type { ReferenceFormat } from './references';

// ---------------------------------------------------------------------------
// 后端命令返回类型
// ---------------------------------------------------------------------------

/** `reference_read_content` 返回的文献内容（MD 原文 + 元数据）。 */
export interface ReaderContent {
  /** 文献 Markdown 原文。 */
  md: string;
  /** 文献元数据。 */
  meta: ReaderMeta;
}

/** 文献元数据（阅读器所需字段）。 */
export interface ReaderMeta {
  id: string;
  title: string;
  original_filename: string;
  format: ReferenceFormat;
  added_at: string;
  source?: string | null;
  ai_summary?: string | null;
  /** MD 文件相对路径（相对项目根）。 */
  md_path: string;
  /** 资源目录相对路径（相对项目根）。 */
  resource_dir: string;
}

// ---------------------------------------------------------------------------
// 块级元素映射（BlockMap）—— 标记/翻译的锚点基础
// ---------------------------------------------------------------------------

/**
 * 块级元素类型枚举。
 *
 * 与 markdown-it 的 block token 类型对应，仅保留可标注的块级单元。
 */
export type BlockType =
  | 'paragraph'
  | 'heading'
  | 'list_item'
  | 'code_block'
  | 'block_quote'
  | 'table'
  | 'table_row'
  | 'image'
  | 'hr'
  | 'other';

/**
 * 块级元素的复合键——唯一标识一个块。
 *
 * 由 `block_type + source_line + occurrence` 三元组构成：
 * - `source_line`：MD 源码行号（markdown-it token.map[0]）
 * - `occurrence`：同 (type, line) 的出现序号，解决同行多 block 冲突
 */
export interface BlockKey {
  block_type: BlockType;
  source_line: number;
  occurrence: number;
}

/** 块内容指纹——校验与迁移依据。 */
export interface Fingerprint {
  /** 块纯文本 SHA-256 前 16 hex。 */
  hash: string;
  /** 首部 32 字符（可读快照）。 */
  prefix: string;
  /** 尾部 32 字符。 */
  suffix: string;
}

/** BlockMap 单条目。 */
export interface BlockMapEntry {
  /** 复合键字符串："{type}:{line}:{occurrence}"。 */
  key: string;
  block_type: BlockType;
  source_line: number;
  occurrence: number;
  /** 纯文本（去 markup）。 */
  text: string;
  fingerprint: Fingerprint;
  /** DOM 选择器："[data-block-key='...']"。 */
  selector: string;
}

/**
 * BlockMap——渲染时生成的块级元素索引。
 *
 * 供标记/翻译锚点解析查询用。每次渲染重建。
 */
export interface BlockMap {
  entries: BlockMapEntry[];
  /** key → entry 快速查询。 */
  byKey: Map<string, BlockMapEntry>;
  /** fingerprint.hash → entries（支持迁移搜索）。 */
  byFingerprint: Map<string, BlockMapEntry[]>;
}

// ---------------------------------------------------------------------------
// 阅读器渲染选项
// ---------------------------------------------------------------------------

/** 阅读器字号（px）。 */
export type ReaderFontSize = 14 | 16 | 18 | 20;

/** 阅读器行高倍数。 */
export type ReaderLineHeight = 1.5 | 1.7 | 1.9;

/** 阅读器渲染选项（深浅色跟随应用主题，不单独设置）。 */
export interface ReaderOptions {
  fontSize: ReaderFontSize;
  lineHeight: ReaderLineHeight;
}

/** 默认阅读器选项。 */
export const DEFAULT_READER_OPTIONS: ReaderOptions = {
  fontSize: 16,
  lineHeight: 1.7,
};
