/**
 * 最近打开文章项目的前端类型定义。
 *
 * 与 Rust 后端 `recent_projects::model` 模块一一对应。
 *
 * @see src-tauri/src/recent_projects/model.rs
 */

/** 单条最近打开项目记录。 */
export interface RecentProjectEntry {
  /** 项目根路径（绝对路径，作为唯一标识）。 */
  project_path: string;
  /** 文章标题（来自 `config.yaml`）。 */
  title: string;
  /** 作者姓名（来自 `config.yaml`）。 */
  author: string;
  /** 最后打开时间（ISO 8601 UTC）。 */
  opened_at: string;
}

/** 最近打开项目列表，对应 `recent_projects.json`。 */
export interface RecentProjectsData {
  /** 数据文件版本号。 */
  version: string;
  /** 最近打开项目列表（按 `opened_at` 降序）。 */
  entries: RecentProjectEntry[];
}
