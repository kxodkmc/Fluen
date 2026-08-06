/**
 * 任务队列通用类型（对应 Rust `task_queue::types`）。
 *
 * 任务队列由后端统一调度：每个任务持久化到 `{project}/data/task-queue.json`，
 * 同一项目内严格串行（FIFO）。任务生命周期管理通过 `task_queue_*` 命令，
 * 执行进度由各业务模块的事件（如 `reference:*`、`kb-build:*`）推送。
 *
 * `kind` 为 internally tagged enum，前端按 `kind` 字段分发到不同业务。
 */

// ---------------------------------------------------------------------------
// 任务状态
// ---------------------------------------------------------------------------

/** 任务状态（对应 Rust `TaskStatus`）。 */
export type TaskStatus = 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';

// ---------------------------------------------------------------------------
// 任务种类
// ---------------------------------------------------------------------------

/** 知识库构建任务参数。 */
export interface TaskKindKnowledgeBuild {
  kind: 'knowledge_build';
  /** 文献 ID。 */
  ref_id: string;
  /** 场景化模型引用。 */
  model_ref: { provider_id: string; model_id: string };
  /** 构建选项（后端默认值兜底）。 */
  options: unknown;
}

/** 文献导入任务参数。 */
export interface TaskKindReferenceImport {
  kind: 'reference_import';
  /** 源文件绝对路径（PDF / 图片）。 */
  file_path: string;
  /** 预分配的文献 ID（`ref-{uuid}`）。 */
  reference_id: string;
  /** 是否跳过文件去重（重试 / 强制导入）。 */
  force: boolean;
}

/** 任务种类（对应 Rust `TaskKind`）。 */
export type TaskKind = TaskKindKnowledgeBuild | TaskKindReferenceImport;

// ---------------------------------------------------------------------------
// 任务记录
// ---------------------------------------------------------------------------

/** 任务记录（对应 Rust `TaskRecord`，含前端所需字段）。 */
export interface TaskRecord {
  /** 任务 ID（`task-{uuid}`）。 */
  id: string;
  /** 项目绝对路径。 */
  project_path: string;
  /** 任务种类与参数。 */
  kind: TaskKind;
  /** 任务状态。 */
  status: TaskStatus;
  /** 断点续传信息（业务相关，前端通常不解析）。 */
  checkpoint: unknown;
  /** 失败原因（仅 `failed` 状态有值）。 */
  error?: string | null;
  /** ISO 8601 创建时间。 */
  created_at: string;
  /** ISO 8601 更新时间。 */
  updated_at: string;
  /** ISO 8601 开始执行时间。 */
  started_at?: string | null;
  /** ISO 8601 完成时间（含失败 / 取消）。 */
  finished_at?: string | null;
}
