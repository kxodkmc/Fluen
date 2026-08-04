/**
 * Mascot 模块 — 公共 API。
 *
 * 导出：
 *   - Mascot：开箱即用的主组件（推荐使用）
 *   - MascotFace：纯 SVG 渲染组件（高级用法，自定义状态管理时使用）
 *   - useMascot：状态机 composable（高级用法，独立使用逻辑层时使用）
 *   - useEyeTracking：眼球鼠标追踪 composable（高级用法）
 *   - DEFAULT_PRESETS：默认表情预设表
 *   - BUILTIN_STATES：所有内置状态名
 *   - 类型：MascotState, ExpressionPreset, EyeConfig, MouthConfig 等
 *
 * @example 基本用法
 * ```vue
 * import { Mascot } from '@/components/mascot';
 *
 * <Mascot :height="28" @click="onClick" />
 * ```
 *
 * @example 高级用法：自定义状态
 * ```ts
 * import { useMascot } from '@/components/mascot';
 *
 * const { registerPreset, setState } = useMascot();
 * registerPreset('working', {
 *   id: 'working',
 *   leftEye: { type: 'circle', rx: 4, ry: 4 },
 *   rightEye: { type: 'circle', rx: 4, ry: 4 },
 *   mouth: { type: 'neutral', width: 8, curve: 0 },
 *   bounce: 0.2,
 *   blink: true,
 * });
 * setState('working');
 * ```
 */

export { default as Mascot } from './Mascot.vue';
export { default as MascotFace } from './MascotFace.vue';
export { useMascot } from './useMascot';
export { useEyeTracking } from './useEyeTracking';
export { DEFAULT_PRESETS, BUILTIN_STATES } from './presets';
export type {
  MascotState,
  ExpressionPreset,
  EyeConfig,
  EyeShapeType,
  EyeOffset,
  MouthConfig,
  MouthShapeType,
  MascotConfig,
} from './types';
