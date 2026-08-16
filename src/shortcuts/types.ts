/**
 * 全局快捷键模块 — 类型定义。
 *
 * 模块定位：提供「键位字符串 → 命令 id → 业务处理器」的低耦合注册/派发机制，
 * 由全局 `keydown` 监听（capture 阶段）统一响应，命中即消费（阻止默认行为）。
 *
 * 设计原则：
 * - 快捷键模块不知道任何业务细节，只维护「绑定表」与「命令表」；
 * - 业务方注册命令处理器（handler），再把键位绑定到命令 id；
 * - 未来支持用户自定义键位时，仅需替换绑定表来源，命令侧零改动。
 */

/** 命令 id（业务语义标识，如 `save-document`）。 */
export type CommandId = string;

/** 修饰键位集合。`mod` 为跨平台占位符，解析时按平台映射为 ctrl（非 mac）/ meta（mac）。 */
export type Modifier = 'ctrl' | 'meta' | 'shift' | 'alt';

/** 解析后的键位绑定（规范形式）。 */
export interface KeyBinding {
  /** 主键：单字符小写（`s`、`1`）或规范特殊键名（`F5`、`Enter`、`ArrowUp`…）。 */
  key: string;
  ctrl: boolean;
  meta: boolean;
  shift: boolean;
  alt: boolean;
}

/** 命令处理器。返回 void；命中绑定即视为已消费（阻止默认行为与冒泡）。 */
export type CommandHandler = (event: KeyboardEvent) => void;

/** 平台标识，用于 `Mod` 修饰键的映射。 */
export type ShortcutPlatform = 'mac' | 'other';
