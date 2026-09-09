/**
 * 知识库模块的前端类型定义。
 *
 * 与 Rust 后端 `knowledge_builder::types` / `knowledge_builder::events` /
 * `knowledge_builder::kb_adapter` 一一对应，序列化格式遵循 serde 默认规则。
 *
 * @see src-tauri/src/knowledge_builder/types.rs
 * @see src-tauri/src/knowledge_builder/events.rs
 * @see src-tauri/src/knowledge_builder/kb_adapter.rs
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

/** 检索方式（对应 Rust `SearchMethod`）。 */
export type RetrievalMethod = 'keyword' | 'semantic' | 'hybrid';

/** 元信息查询类型（对应 Rust `knowledge_meta` 的 `query_type` 参数）。 */
export type MetaQueryType = 'overview' | 'recent';

/** 关联引用（列表视图：谓词 + 目标 wikiID）。 */
export interface RelationRef {
  /** 关联谓词（如 `related` / `作者` / `应用了`）。 */
  predicate: string;
  /** 目标条目 wikiID。 */
  id: string;
}

/** 关联条目（详情视图：谓词 + wikiID + 标题）。 */
export interface RelationInfo {
  /** 关联谓词。 */
  predicate: string;
  /** 目标条目 wikiID。 */
  id: string;
  /** 目标条目标题（缺失时回退为 ID）。 */
  title: string;
}

/** 知识库条目（对应 Rust `WikiEntry`，列表查询时不含正文）。 */
export interface WikiEntry {
  id: string;
  wiki_type: WikiType;
  title: string;
  file_path: string;
  /** 仅 summaries：源文献 refID（`ref-xxx`）。 */
  source?: string;
  /** 出向关联（带谓词）。 */
  relations: RelationRef[];
  /** 创建时间（RFC3339）。 */
  created: string;
  /** 更新时间（RFC3339）。 */
  updated: string;
}

/**
 * 知识库条目详情（对应 Rust `WikiEntryDetail`）。
 *
 * 由 `knowledge_get_entry` 命令返回；正文已由后端剥离 `<ref-xxx>`
 * 溯源标签，可直接渲染。
 */
export interface WikiEntryDetail {
  id: string;
  wiki_type: WikiType;
  title: string;
  file_path: string;
  /** 仅 summaries：源文献 refID（`ref-xxx`）。 */
  source?: string;
  /** 出向关联（谓词 + wikiID + 标题）。 */
  relations: RelationInfo[];
  /** 正文（已剥离溯源标签）。 */
  content?: string;
  created: string;
  updated: string;
}

/** 单条检索结果（对应 Rust `QueryMatch`）。 */
export interface QueryMatch {
  wiki_id: string;
  /** 条目类型（Rust 序列化为 `type`）。 */
  wiki_type: WikiType;
  title: string;
  file_path: string;
  /** 匹配度得分。 */
  score: number;
  /** 正文（仅 include_content=true 时返回）。 */
  content?: string;
}

/** 检索结果（对应 Rust `QueryResult`）。 */
export interface QueryResult {
  success: boolean;
  results: QueryMatch[];
}

/** 近期更新条目（对应 Rust `RecentEntry`，仅含 id 与 title）。 */
export interface RecentEntry {
  id: string;
  title: string;
}

/** 元信息数据体（对应 Rust `MetaData`；新库无 tags 概念）。 */
export interface MetaData {
  total_entries: number;
  /** query_type='recent' 时返回。 */
  recent_entries?: RecentEntry[];
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
