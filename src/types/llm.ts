/**
 * LLM 配置的前端类型定义。
 *
 * 与 Rust 后端 `llm_config::model` 模块一一对应，
 * 序列化 / 反序列化格式遵循 serde 的默认规则（snake_case 字段名）。
 *
 * @see src-tauri/src/llm_config/model.rs
 */

/** API 风格（对应 Rust `ApiStyle`，serde 默认 PascalCase 序列化）。 */
export type ApiStyle = 'OpenAI' | 'Anthropic';

/** 提供商类型（对应 Rust `ProviderType`，serde `rename_all = "snake_case"`）。 */
export type ProviderType = 'custom' | 'cloud';

/** 模型能力标志。 */
export interface ModelCapabilities {
  thinking: boolean;
  vision: boolean;
  audio: boolean;
  video: boolean;
  tool_calling: boolean;
  streaming: boolean;
}

/** 单个模型的配置。 */
export interface ModelConfig {
  id: string;
  name: string;
  capabilities: ModelCapabilities;
  max_output_tokens: number | null;
  context_window: number | null;
  description: string | null;
  enabled: boolean;
}

/** 单个 LLM 提供商的配置。 */
export interface ProviderConfig {
  id: string;
  name: string;
  provider_type: ProviderType;
  openai_base_url: string | null;
  anthropic_base_url: string | null;
  api_key: string | null;
  default_style: ApiStyle;
  extra_headers: Record<string, string>;
  enabled: boolean;
  models: ModelConfig[];
  created_at: string | null;
  updated_at: string | null;
}

/** 场景化模型引用——指向 `providers` 中具体的 provider+model 组合。 */
export interface SceneModelRef {
  provider_id: string;
  model_id: string;
}

/**
 * 场景化模型配置集合。
 *
 * 每个字段对应一个业务场景的专用模型槽位。`null` 表示该场景回退到
 * 全局 `active_provider_id` / `active_model_id`。
 */
export interface SceneModels {
  /** 知识库构建专用模型槽位。 */
  knowledge_build?: SceneModelRef | null;
}

/**
 * Embedding 模型配置。
 *
 * 控制知识库语义检索的 Embedding 模型选择。
 * `model_ref` 为 `null` 时使用内置免费提供商（ModelScope Qwen3-Embedding-4B）。
 */
export interface EmbeddingConfig {
  /** 是否启用 Embedding（默认 `true`）。禁用时降级为纯关键词检索。 */
  enabled: boolean;
  /** 用户自定义 Embedding 模型引用。`null` 时使用内置免费提供商。 */
  model_ref?: SceneModelRef | null;
}

/** LLM 配置顶层结构，对应 `llm_config.json`。 */
export interface LlmConfig {
  version: string;
  active_provider_id: string | null;
  active_model_id: string | null;
  providers: ProviderConfig[];
  /** 场景化模型槽位（按业务场景指定专用模型）。 */
  scene_models?: SceneModels | null;
  /** Embedding 模型配置。 */
  embedding?: EmbeddingConfig | null;
}
