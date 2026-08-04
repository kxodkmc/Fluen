/**
 * 任务队列通知系统 — 类型定义。
 *
 * 任务由各业务模块通过 useTaskQueue / useTask 注册，
 * TaskQueueIndicator 在状态栏实时展示最新活跃任务进度，
 * TaskQueuePanel 展开时显示全部任务列表。
 *
 * 设计要点：
 *   - 进度样式通过 progressStyle 字段扩展，当前仅 'bar'
 *   - 进度数据支持 current/total 或直接 ratio（0..1）
 *   - 无 progress 字段时渲染为不确定进度（indeterminate）
 */

/** 任务状态。 */
export type TaskStatus = 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';

/** 进度样式（便于后续扩展，如 spinner、percentage 等）。 */
export type TaskProgressStyle = 'bar';

/** 任务进度数据。 */
export interface TaskProgress {
  /** 当前值（0..total）。 */
  current?: number;
  /** 总值。 */
  total?: number;
  /** 标准化进度 0..1，提供时优先于 current/total。 */
  ratio?: number;
}

/** 任务条目。 */
export interface TaskEntry {
  /** 唯一标识，建议带模块前缀：`ocr.import`、`wiki.build`。 */
  id: string;
  /** 任务标题。 */
  title: string;
  /** 任务状态。 */
  status: TaskStatus;
  /** 进度数据（无明确进度时省略，渲染为不确定进度条）。 */
  progress?: TaskProgress;
  /** 进度样式，默认 'bar'。 */
  progressStyle?: TaskProgressStyle;
  /** 分类标签（用于图标映射），如 'ocr'、'knowledge'。 */
  category?: string;
  /** 可选详情文本（如「已识别 12/30 页」）。 */
  detail?: string;
  /** 创建时间戳。 */
  createdAt: number;
  /** 完成时间戳（status 为终态时设置）。 */
  finishedAt?: number;
  /** 错误信息（status 为 'failed' 时）。 */
  error?: string;
}
