/**
 * Mascot 模块 — 类型定义。
 *
 * 定义眼睛、嘴巴、表情预设和状态机的类型。
 * 所有类型为纯数据描述，不含运行时逻辑，便于序列化和扩展。
 *
 * 扩展指南：
 *   - 新增眼睛形状：在 EyeShapeType 添加成员，在 MascotFace 的 closedEyePath 中实现
 *   - 新增嘴巴形状：在 MouthShapeType 添加成员，在 MascotFace 的 mouthPath 中实现
 *   - 新增状态：在 MascotState 添加成员，在 presets.ts 添加预设
 *   - 运行时注册自定义状态：useMascot().registerPreset(name, preset)
 */

import type { Mood } from '../../types/mascot';

// ── 眼睛 ──────────────────────────────────────────────────────────────────

/** 眼睛形状类型。 */
export type EyeShapeType =
  | 'circle' // 圆形（默认睁眼）
  | 'oval' // 椭圆（好奇/惊讶）
  | 'arc-happy' // 上弧（开心闭眼 ⌒）
  | 'arc-sad' // 下弧（难过 ⌣）
  | 'line'; // 直线（闭眼/睡觉 —）

/** 单只眼睛配置。 */
export interface EyeConfig {
  /** 形状类型。 */
  type: EyeShapeType;
  /** 水平半径（circle/oval 为椭圆 rx；arc/line 为半宽）。 */
  rx: number;
  /** 垂直半径（circle/oval 为椭圆 ry；arc 为弧深）。 */
  ry: number;
}

// ── 嘴巴 ──────────────────────────────────────────────────────────────────

/** 嘴巴形状类型。 */
export type MouthShapeType =
  | 'smile' // 微笑弧线
  | 'smile-small' // 小微笑
  | 'neutral' // 平直
  | 'open' // 张嘴（椭圆）
  | 'frown'; // 下垂（不开心）

/** 嘴巴配置。 */
export interface MouthConfig {
  /** 形状类型。 */
  type: MouthShapeType;
  /** 嘴巴宽度。 */
  width: number;
  /** 弧度：正值=上扬（笑），负值=下垂（不开心）。 */
  curve: number;
}

// ── 表情预设 ──────────────────────────────────────────────────────────────

/** 完整表情预设：定义双眼 + 嘴巴 + 动画参数。 */
export interface ExpressionPreset {
  /** 预设唯一标识。 */
  id: string;
  /** 左眼配置。 */
  leftEye: EyeConfig;
  /** 右眼配置。 */
  rightEye: EyeConfig;
  /** 嘴巴配置。 */
  mouth: MouthConfig;
  /** 弹跳强度（0=无，1=最大）。 */
  bounce: number;
  /** 是否启用自动眨眼。 */
  blink: boolean;
}

// ── 状态机 ────────────────────────────────────────────────────────────────

/** Mascot 内置状态。可通过 registerPreset 扩展自定义状态。 */
export type MascotState =
  | 'idle' // 闲置（默认，缓慢呼吸 + 偶尔眨眼）
  | 'happy' // 开心
  | 'curious' // 好奇
  | 'thinking' // 思考
  | 'sleeping' // 睡觉
  | 'excited' // 兴奋
  | 'sad' // 难过
  | 'wink' // 眨眼俏皮
  | 'surprised' // 惊讶
  | (string & {}); // 允许自定义状态（需先 registerPreset）

// ── 配置 ──────────────────────────────────────────────────────────────────

/** Mascot 组件配置。 */
export interface MascotConfig {
  /** 整体高度（px）。 */
  height: number;
  /** 初始状态。 */
  initialState: MascotState;
  /** 自动眨眼间隔（ms），0 = 禁用。 */
  blinkInterval: number;
  /** 眨眼持续时间（ms）。 */
  blinkDuration: number;
  /** 闲置时是否随机切换微表情。 */
  idleMicroExpressions: boolean;
  /** 当前心情（驱动 rest state 与微表情池）。 */
  mood: Mood;
}

// ── 眼球追踪 ──────────────────────────────────────────────────────────────

/** 眼球偏移量（SVG 用户单位）。 */
export interface EyeOffset {
  /** 水平偏移。 */
  x: number;
  /** 垂直偏移。 */
  y: number;
}
