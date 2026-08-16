/**
 * 项目模块的前端类型定义。
 *
 * 与 Rust 后端 `project::model` 模块一一对应。
 *
 * @see src-tauri/src/project/model.rs
 */

// ---------------------------------------------------------------------------
// config.yaml
// ---------------------------------------------------------------------------

/** 项目配置，对应 `config.yaml`。 */
export interface ProjectConfig {
  version: string;
  title: string;
  author: string;
  description?: string;
  created_at: string;
  updated_at: string;
}

// ---------------------------------------------------------------------------
// sections.json
// ---------------------------------------------------------------------------

/** `sections.json` 中的章节条目。 */
export interface SectionMeta {
  id: string;
  order: number;
  title: string;
  title_html?: string;
  references: string[];
}

// ---------------------------------------------------------------------------
// 软校验警告
// ---------------------------------------------------------------------------

/** 警告类型。 */
export type WarningKind =
  | 'missing_dir'
  | 'missing_file'
  | 'orphan_section_entry'
  | 'orphan_section_file';

/** 单条校验警告。 */
export interface ProjectWarning {
  kind: WarningKind;
  target: string;
  message: string;
}

// ---------------------------------------------------------------------------
// 打开项目返回值
// ---------------------------------------------------------------------------

/** `open_project` 命令返回的完整数据。 */
export interface OpenProjectResult {
  config: ProjectConfig;
  project_path: string;
  sections: SectionMeta[];
  main_md: string;
  warnings: ProjectWarning[];
}

// ---------------------------------------------------------------------------
// 结构化错误响应
// ---------------------------------------------------------------------------

/** 后端返回的结构化错误，通过 `kind` 精确判断类型。 */
export type ProjectErrorResponse =
  | { kind: 'NotADirectory'; path: string }
  | { kind: 'ConfigError'; reason: string }
  | { kind: 'SectionsIndexError'; reason: string }
  | { kind: 'SectionFileMissing'; section_id: string }
  | { kind: 'SectionParseError'; section_id: string; reason: string }
  | { kind: 'Validation'; reason: string }
  | { kind: 'AlreadyExists'; path: string }
  | { kind: 'Io'; reason: string };

// ---------------------------------------------------------------------------
// 章节操作请求（前端 → 后端）
// ---------------------------------------------------------------------------

/** `create_section` 命令的请求参数。 */
export interface CreateSectionRequest {
  project_path: string;
  title: string;
}

/** `rename_heading` 命令的请求参数。 */
export interface RenameHeadingRequest {
  project_path: string;
  section_id: string;
  level: number;
  old_text: string;
  new_text: string;
}

/** `insert_heading` 命令的请求参数。 */
export interface InsertHeadingRequest {
  project_path: string;
  section_id: string;
  anchor_level: number;
  anchor_text: string;
  new_level: number;
  new_text: string;
}

/** `save_document` 命令的请求参数。 */
export interface SaveDocumentRequest {
  project_path: string;
  content: string;
}
