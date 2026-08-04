/**
 * Mascot 模块 — 表情预设表。
 *
 * 为每个内置状态定义对应的眼睛 + 嘴巴 + 动画参数。
 * 坐标系基于 viewBox "0 0 100 70"，眼睛半径/嘴巴宽度均为 SVG 用户单位。
 *
 * 扩展方式：
 *   useMascot().registerPreset('my-state', { id: 'my-state', leftEye: {...}, ... })
 *   mascotRef.value.setState('my-state')
 */

import type { ExpressionPreset, MascotState } from './types';

/** 默认表情预设：状态 → 表情。 */
export const DEFAULT_PRESETS: Record<string, ExpressionPreset> = {
  /* ── 闲置：圆形小眼 + 微笑 ──────────────────────────────────────────── */
  idle: {
    id: 'idle',
    leftEye: { type: 'circle', rx: 7, ry: 7 },
    rightEye: { type: 'circle', rx: 7, ry: 7 },
    mouth: { type: 'smile-small', width: 10, curve: 2.5 },
    bounce: 0.15,
    blink: true,
  },

  /* ── 开心：弧形闭眼 + 大笑 ──────────────────────────────────────────── */
  happy: {
    id: 'happy',
    leftEye: { type: 'arc-happy', rx: 7, ry: 5 },
    rightEye: { type: 'arc-happy', rx: 7, ry: 5 },
    mouth: { type: 'smile', width: 14, curve: 5 },
    bounce: 0.4,
    blink: true,
  },

  /* ── 好奇：竖椭圆大眼 + 抿嘴 ────────────────────────────────────────── */
  curious: {
    id: 'curious',
    leftEye: { type: 'oval', rx: 6, ry: 8 },
    rightEye: { type: 'oval', rx: 6, ry: 8 },
    mouth: { type: 'neutral', width: 6, curve: 0 },
    bounce: 0.1,
    blink: true,
  },

  /* ── 思考：不对称眼睛 + 微抿 ────────────────────────────────────────── */
  thinking: {
    id: 'thinking',
    leftEye: { type: 'circle', rx: 6, ry: 6 },
    rightEye: { type: 'circle', rx: 7, ry: 4 },
    mouth: { type: 'neutral', width: 8, curve: -1 },
    bounce: 0.05,
    blink: false,
  },

  /* ── 睡觉：闭眼直线 + 小微笑 ────────────────────────────────────────── */
  sleeping: {
    id: 'sleeping',
    leftEye: { type: 'line', rx: 7, ry: 1 },
    rightEye: { type: 'line', rx: 7, ry: 1 },
    mouth: { type: 'smile-small', width: 8, curve: 2 },
    bounce: 0.2,
    blink: false,
  },

  /* ── 兴奋：大圆眼 + 张嘴 ────────────────────────────────────────────── */
  excited: {
    id: 'excited',
    leftEye: { type: 'circle', rx: 8, ry: 8 },
    rightEye: { type: 'circle', rx: 8, ry: 8 },
    mouth: { type: 'open', width: 8, curve: 0 },
    bounce: 0.6,
    blink: true,
  },

  /* ── 难过：下弧眼 + 嘴角下垂 ────────────────────────────────────────── */
  sad: {
    id: 'sad',
    leftEye: { type: 'arc-sad', rx: 7, ry: 5 },
    rightEye: { type: 'arc-sad', rx: 7, ry: 5 },
    mouth: { type: 'frown', width: 12, curve: -4 },
    bounce: 0.05,
    blink: false,
  },

  /* ── 眨眼俏皮：左眼弧形 + 右眼圆 ────────────────────────────────────── */
  wink: {
    id: 'wink',
    leftEye: { type: 'arc-happy', rx: 7, ry: 5 },
    rightEye: { type: 'circle', rx: 7, ry: 7 },
    mouth: { type: 'smile', width: 12, curve: 4 },
    bounce: 0.3,
    blink: false,
  },

  /* ── 惊讶：大椭圆眼 + 小张嘴 ────────────────────────────────────────── */
  surprised: {
    id: 'surprised',
    leftEye: { type: 'oval', rx: 7, ry: 9 },
    rightEye: { type: 'oval', rx: 7, ry: 9 },
    mouth: { type: 'open', width: 5, curve: 0 },
    bounce: 0.5,
    blink: false,
  },
};

/** 获取所有内置状态名。 */
export const BUILTIN_STATES: MascotState[] = Object.keys(DEFAULT_PRESETS) as MascotState[];

/** 心情 → 静止回归状态映射。 */
export const MOOD_REST_STATE: Record<string, MascotState> = {
  happy: 'happy',
  neutral: 'idle',
  sad: 'sad',
};

/** 心情 → 闲置微表情池映射。 */
export const MOOD_MICRO_STATES: Record<string, MascotState[]> = {
  happy: ['happy', 'curious', 'wink'],
  neutral: ['curious', 'happy', 'wink'],
  sad: ['sad', 'sleeping'],
};
