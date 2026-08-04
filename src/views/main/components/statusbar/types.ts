/**
 * 状态栏系统 — 类型定义。
 *
 * 状态栏条目由各业务模块通过 useStatusBar / useStatusEntry 注册，
 * StatusBar.vue 组件消费注册表数据纯渲染。
 */

/** 状态栏分区。 */
export type StatusPosition = 'left' | 'right';

/** 语义色调，渲染组件据此映射 CSS class。 */
export type StatusTone = 'default' | 'info' | 'warning' | 'error';

/** 状态栏条目。 */
export interface StatusEntry {
  /** 唯一标识，建议带模块前缀：`editor.cursor`、`git.branch`。 */
  id: string;
  /** 显示文本。 */
  label: string;
  /** 左侧 / 右侧分区。 */
  position: StatusPosition;
  /** 排序权重，升序排列，默认 0。同 order 按注册时间先后。 */
  order?: number;
  /** SVG path data（可选，24×24 viewBox）。 */
  icon?: string;
  /** 鼠标悬停提示。 */
  tooltip?: string;
  /** 点击回调。 */
  onClick?: () => void;
  /** 语义色调，默认 'default'。 */
  tone?: StatusTone;
}
