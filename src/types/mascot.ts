/**
 * 宠物助手（Mascot）的前端类型定义。
 *
 * 与 Rust 后端 `mascot::model` 模块一一对应，
 * 序列化 / 反序列化格式遵循 serde 的默认规则（snake_case 字段名）。
 *
 * @see src-tauri/src/mascot/model.rs
 */

/** 心情（对应 Rust `Mood`，serde `rename_all = "lowercase"`）。 */
export type Mood = 'happy' | 'neutral' | 'sad';

/** 宠物助手配置，对应 `mascot_config.json`。 */
export interface MascotConfig {
  /** 配置文件版本号。 */
  version: string;
  /** 宠物名称。 */
  name: string;
  /** 是否启用宠物助手。 */
  enabled: boolean;
  /** LLM 提供商 ID（可选）。 */
  provider_id: string | null;
  /** LLM 模型 ID（可选）。 */
  model_id: string | null;
  /** 是否启用 MCP 工具调用。 */
  mcp_enabled: boolean;
  /** 是否启用 Skills 能力。 */
  skills_enabled: boolean;
  /** 是否启用函数调用。 */
  function_calling_enabled: boolean;
  /** 宠物人格描述。 */
  personality: string;
  /** 是否展示详细思考内容（默认 false，仅显示"思考中…"）。 */
  show_thinking_content: boolean;
  /** 是否使用专业化表述（默认 false，使用拟人化文案）。 */
  professional_expression: boolean;
}

/** 宠物助手运行时数据，对应 `mascot_data.json`。 */
export interface MascotData {
  /** 数据文件版本号。 */
  version: string;
  /** 当前心情。 */
  mood: Mood;
  /** 好感度（0-100）。 */
  affinity: number;
  /** 心情最近更新时间（ISO 8601 字符串，可选）。 */
  mood_updated_at: string | null;
  /** 好感度最近更新时间（ISO 8601 字符串，可选）。 */
  affinity_updated_at: string | null;
}
