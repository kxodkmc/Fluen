/**
 * 编辑器模块的前端类型定义。
 *
 * 与 Rust 后端 `editor::commands` 模块一一对应。
 *
 * @see src-tauri/src/editor/commands.rs
 */

import type { OpenProjectResult } from './project';

/**
 * Editor command error response. Mirrors Rust `EditorErrorResponse`
 * (single `message` field, defined in src-tauri/src/editor/commands.rs).
 */
export interface EditorErrorResponse {
  message: string;
}

/**
 * Response of `editor_save_content` command. Returns the refreshed project
 * state after saving MD content and re-splitting into sec-*.md files.
 */
export type SaveContentResponse = OpenProjectResult;

/**
 * Response of `editor_save_asset` command. Relative path of the saved asset
 * (e.g. "assets/pasted-123.png") under manuscript/.
 */
export type SaveAssetResponse = string;

/**
 * Response of `editor_render_html` command. HTML string for preview rendering.
 */
export type RenderHtmlResponse = string;
