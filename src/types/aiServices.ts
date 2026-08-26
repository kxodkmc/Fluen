/**
 * AI 服务配置的前端类型定义。
 *
 * 与 Rust 后端 `ai_services::model` 模块一一对应，
 * 序列化 / 反序列化格式遵循 serde 的默认规则。
 *
 * @see src-tauri/src/ai_services/model.rs
 */

/** 服务类型（对应 Rust `ServiceCategory`，serde `rename_all = "snake_case"`）。 */
export type ServiceCategory = 'ocr' | 'tts' | 'asr';

/** 文献导入模式（对应 Rust `ReferenceImportMode`，serde `rename_all = "snake_case"`）。 */
export type ReferenceImportMode = 'ocr' | 'ocr_with_ai_correction' | 'ai_only';

/** 部署模式（对应 Rust `DeploymentMode`）。 */
export type DeploymentMode = 'api' | 'local';

/** API 认证方案（对应 Rust `AuthScheme`）。 */
export type AuthScheme = 'bearer' | 'token';

/** 单个 AI 服务模型的配置。 */
export interface AiServiceModel {
  id: string;
  name: string;
  enabled: boolean;
}

/** 单个 AI 服务提供商的配置。 */
export interface AiServiceProvider {
  id: string;
  name: string;
  category: ServiceCategory;
  deployment: DeploymentMode;
  api_base_url: string | null;
  api_key: string | null;
  auth_scheme: AuthScheme;
  /** 提供商专属配置（JSON），由前端按 category 解析。 */
  provider_config: Record<string, unknown> | null;
  models: AiServiceModel[];
  active_model_id: string | null;
  enabled: boolean;
  created_at: string | null;
  updated_at: string | null;
}

/** AI 服务配置顶层结构，对应 `ai_services_config.json`。 */
export interface AiServicesConfig {
  version: string;
  /** 每个服务类型的活跃提供商 ID。 */
  active_providers: Partial<Record<ServiceCategory, string>>;
  providers: AiServiceProvider[];
  /** 文献导入默认模式（设置页可配置，默认纯 OCR）。 */
  default_reference_import_mode: ReferenceImportMode;
  /** 文献导入时 AI 校正的最大响应时间（秒），默认 600。 */
  reference_import_timeout_secs: number;
}

// ---------------------------------------------------------------------------
// PaddleOCR 专属配置类型
// ---------------------------------------------------------------------------

/** PaddleOCR API 调用模式。 */
export type OcrApiMode = 'job' | 'sync';

/** PaddleOCR 识别选项。 */
export interface PaddleOcrOptions {
  use_doc_orientation_classify: boolean;
  use_doc_unwarping: boolean;
  use_chart_recognition: boolean;
}

/** PaddleOCR 提供商专属配置。 */
export interface PaddleOcrConfig {
  api_mode: OcrApiMode;
  options: PaddleOcrOptions;
}

// ---------------------------------------------------------------------------
// OCR 结果类型
// ---------------------------------------------------------------------------

/** OCR 进度信息（事件 payload）。 */
export interface OcrProgress {
  state: string;
  total_pages?: number | null;
  extracted_pages?: number | null;
  message?: string | null;
}

/** OCR 结果中的图片。 */
export interface OcrImage {
  name: string;
  path: string;
}

/** 单页 OCR 结果。 */
export interface OcrPage {
  index: number;
  markdown: string;
  images: OcrImage[];
}

/** OCR 完整结果。 */
export interface OcrResult {
  pages: OcrPage[];
  temp_dir: string;
}
