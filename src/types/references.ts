/**
 * 参考文献模块的前端类型定义。
 *
 * 与 Rust 后端 `references::model` / `references::commands` 一一对应，
 * 序列化 / 反序列化格式遵循 serde 的默认规则（snake_case）。
 *
 * @see src-tauri/src/references/model.rs
 * @see src-tauri/src/references/commands.rs
 */

import type { OcrProgress } from './aiServices';

// ---------------------------------------------------------------------------
// 文献格式与状态
// ---------------------------------------------------------------------------

/** 文献文件格式（对应 Rust `ReferenceFormat`）。 */
export type ReferenceFormat = 'pdf' | 'image';

/** 导入状态（对应 Rust `ReferenceStatus`）。 */
export type ReferenceStatus = 'pending' | 'processing' | 'completed' | 'failed';

// ---------------------------------------------------------------------------
// 文献条目
// ---------------------------------------------------------------------------

/** 文献索引条目，对应 `references-index.json` 数组元素。 */
export interface ReferenceEntry {
  /** 文献唯一 ID（`ref-{32位UUID4}`）。 */
  id: string;
  /** 标题。 */
  title: string;
  /** 原始文件名。 */
  original_filename: string;
  /** 文件格式。 */
  format: ReferenceFormat;
  /** 文件内容 SHA-256 哈希（用于去重）。 */
  file_hash: string;
  /** 原文件备份路径（相对项目根）。 */
  file_path: string;
  /** Markdown 文件路径（相对项目根）。 */
  md_path: string;
  /** 资源目录路径（相对项目根）。 */
  resource_dir: string;
  /** 添加时间（ISO 8601）。 */
  added_at: string;
  /** 来源网站（预留）。 */
  source?: string | null;
  /** AI 摘要（预留）。 */
  ai_summary?: string | null;
  /** 作者列表（AI 校正解析，未解析时为空/缺省）。 */
  authors?: string[];
  /** 导入状态。 */
  status: ReferenceStatus;
  /** 失败原因（`status === 'failed'` 时有值）。 */
  error?: string | null;
}

// ---------------------------------------------------------------------------
// 一致性校验报告
// ---------------------------------------------------------------------------

/** 一致性校验报告（`check_references_consistency` 返回）。 */
export interface ConsistencyReport {
  /** 孤儿文件（raw/ 下有文件但 index 无记录）。 */
  orphan_files: string[];
  /** 缺失文件的条目（index 有记录但 raw/md 缺失）。 */
  broken_entries: string[];
  /** 已修复的孤儿文件数（移动到 .orphan/）。 */
  repaired: number;
}

// ---------------------------------------------------------------------------
// 事件 payload（由后端 Tauri 事件推送）
// ---------------------------------------------------------------------------

/** `reference:import_started` 事件 payload。 */
export interface ImportStartedPayload {
  job_id: string | null;
  reference_id: string;
  filename: string;
}

/** `reference:import_progress` 事件 payload。 */
export interface ImportProgressPayload {
  job_id: string | null;
  reference_id: string;
  stage: string;
  ocr_progress?: OcrProgress | null;
}

/** `reference:import_completed` 事件 payload。 */
export interface ImportCompletedPayload {
  job_id: string | null;
  reference_id: string;
  entry: ReferenceEntry;
}

/** `reference:import_failed` 事件 payload。 */
export interface ImportFailedPayload {
  job_id: string | null;
  reference_id: string;
  error: string;
}

// ---------------------------------------------------------------------------
// 导入错误类型
// ---------------------------------------------------------------------------

/**
 * 文献模块错误（对应 Rust `ReferenceError`）。
 *
 * 后端通过 `thiserror` 的 `#[error(...)]` 将错误序列化为字符串，
 * 前端接收到的 `reject` 值为字符串而非结构化对象。
 *
 * 可能的错误消息包括：
 * - `"文件不存在: {path}"`
 * - `"不支持的文件格式: {extension}（仅支持 PDF 和图片）"`
 * - `"文件已导入: {existing_title}"`
 * - `"OCR 服务不可用: {message}"`
 * - `"导入任务已取消"`
 * - `"文献不存在: {id}"`
 * - `"IO 错误: {message}"`
 */
export type ReferenceErrorString = string;
