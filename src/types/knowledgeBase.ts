/**
 * 知识库模块的前端类型定义。
 *
 * 与 Rust 后端 `knowledge_builder::types` / `knowledge_builder::events` /
 * `fluen_knowledge::types` 一一对应，序列化格式遵循 serde 默认规则。
 *
 * @see src-tauri/src/knowledge_builder/types.rs
 * @see src-tauri/src/knowledge_builder/events.rs
 * @see crates/fluen-knowledge/src/types.rs
 */

// ---------------------------------------------------------------------------
// 构建选项与阶段
// ---------------------------------------------------------------------------

/** 知识库构建选项（对应 Rust `KnowledgeBuildOptions`）。 */
export interface KnowledgeBuildOptions {
  /** 是否创建综述页（summary）。 */
  create_summary?: boolean;
  /** 是否创建概念页（concept）。 */
  create_concepts?: boolean;
  /** 是否创建实体页（entity）。 */
  create_entities?: boolean;
  /** 是否自动建立 summary ↔ concept/entity relations。 */
  auto_relations?: boolean;
  /** 概念提取数量上限。 */
  max_concepts?: number;
  /** 实体提取数量上限。 */
  max_entities?: number;
}

/** 构建阶段（对应 Rust `BuildStage`，snake_case 序列化）。 */
export type BuildStage =
  | 'planning'
  | 'creating_summary'
  | 'creating_concepts'
  | 'creating_entities'
  | 'establishing_relations'
  | 'done';

// ---------------------------------------------------------------------------
// 知识库条目
// ---------------------------------------------------------------------------

/** 条目类型（对应 Rust `WikiType`）。 */
export type WikiType = 'concept' | 'entity' | 'summary';

/** 检索方式（对应 Rust `RetrievalMethod`）。 */
export type RetrievalMethod = 'keyword' | 'semantic' | 'hybrid';

/** 元信息查询类型（对应 Rust `MetaQueryType`）。 */
export type MetaQueryType = 'overview' | 'tags' | 'recent';

/** 知识库条目（对应 Rust `WikiEntry`，列表查询时不含正文）。 */
export interface WikiEntry {
  id: string;
  wiki_type: WikiType;
  title: string;
  file_path: string;
  /** 仅 summaries：源文献路径，如 `raw/ref-xxx.pdf`。 */
  source?: string;
  /** 仅 summaries：作者 wikiID 列表（DB 读取时为空）。 */
  authors?: string[];
  /** 标签 ID 列表。 */
  tags?: string[];
  /** 关联 wikiID 列表（出向）。 */
  relations?: string[];
  /** 正文（列表查询时为空，详情查询时填充）。 */
  content?: string;
  /** 创建时间（RFC3339）。 */
  created: string;
  /** 更新时间（RFC3339）。 */
  updated: string;
}

/**
 * 知识库条目详情（对应 Rust `WikiEntryDetail`）。
 *
 * 通过 `#[serde(flatten)]` 扩展 `WikiEntry`，额外提供标签名与关联条目标题。
 * 由 `knowledge_get_entry` 命令返回。
 */
export interface WikiEntryDetail extends WikiEntry {
  /** 标签名称列表，与 `tags` 一一对应（空时省略）。 */
  tag_titles?: string[];
  /** 关联条目标题列表，与 `relations` 一一对应（空时省略）。 */
  relation_titles?: string[];
}

/** 标签（对应 Rust `WikiTag`）。 */
export interface WikiTag {
  id: string;
  title: string;
}

/** 实际使用的检索方式（对应 Rust `RetrievalMethodUsed`，snake_case 序列化）。 */
export type RetrievalMethodUsed =
  | 'direct_id_lookup'
  | 'keyword'
  | 'semantic'
  | 'hybrid';

/** 单条检索结果（对应 Rust `QueryMatch`）。 */
export interface QueryMatch {
  wiki_id: string;
  /** 条目类型（Rust 序列化为 `type`）。 */
  wiki_type: WikiType;
  title: string;
  file_path: string;
  /** 匹配度得分（ID 直查时固定 1.0）。 */
  score: number;
  /** 正文（仅 include_content=true 时返回）。 */
  content?: string;
}

/** 检索结果（对应 Rust `QueryResult`）。 */
export interface QueryResult {
  success: boolean;
  retrieval_method_used: RetrievalMethodUsed;
  results: QueryMatch[];
}

/** 近期更新条目（对应 Rust `RecentEntry`，仅含 id 与 title）。 */
export interface RecentEntry {
  id: string;
  title: string;
}

/** 元信息数据体（对应 Rust `MetaData`，按 query_type 返回不同字段）。 */
export interface MetaData {
  total_entries: number;
  total_tags: number;
  embedding_enabled: boolean;
  /** query_type='tags' 时返回。 */
  tags?: WikiTag[];
  /** query_type='recent' 时返回。 */
  recent_entries?: RecentEntry[];
}

/** 元信息查询结果（对应 Rust `MetaResult`）。 */
export interface MetaResult {
  success: boolean;
  data: MetaData;
}

// ---------------------------------------------------------------------------
// 构建任务记录
// ---------------------------------------------------------------------------

/**
 * 任务队列通用类型统一收口在 `./taskQueue`，此处 re-export 保持
 * 既有 import 路径兼容（useKnowledgeBase 等）。
 */
export type { TaskKind, TaskKindKnowledgeBuild, TaskRecord, TaskStatus } from './taskQueue';

// ---------------------------------------------------------------------------
// 事件 payload（由后端 Tauri 事件推送）
// ---------------------------------------------------------------------------

/** `kb-build:started` 事件 payload。 */
export interface KbBuildStartedPayload {
  task_id: string;
  ref_id: string;
  title?: string | null;
}

/** `kb-build:progress` 事件 payload。 */
export interface KbBuildProgressPayload {
  task_id: string;
  ref_id: string;
  stage: string;
  created_count: number;
  total_planned?: number | null;
  detail?: string | null;
}

/** `kb-build:completed` 事件 payload。 */
export interface KbBuildCompletedPayload {
  task_id: string;
  ref_id: string;
  summary_id?: string | null;
  concept_ids: string[];
  entity_ids: string[];
  relations_established: boolean;
}

/** `kb-build:failed` 事件 payload。 */
export interface KbBuildFailedPayload {
  task_id: string;
  ref_id: string;
  error: string;
}

/** `kb-build:cancelled` 事件 payload。 */
export interface KbBuildCancelledPayload {
  task_id: string;
  ref_id: string;
}

// ---------------------------------------------------------------------------
// 前端构建状态
// ---------------------------------------------------------------------------

/**
 * 单个文献的知识库构建状态（前端维护，用于 UI 展示状态徽标）。
 *
 * - `idle`：尚未加入知识库
 * - `building`：构建任务进行中
 * - `added`：已成功加入知识库（存在 summary 条目）
 * - `failed`：最近一次构建失败
 */
export type KnowledgeBuildStatus = 'idle' | 'building' | 'added' | 'partial' | 'failed';
