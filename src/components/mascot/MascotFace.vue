<script setup lang="ts">
/**
 * MascotFace — Mascot SVG 渲染层。
 *
 * 纯渲染组件：接收表情数据 + 眨眼/弹跳状态，输出 SVG。
 * 不含状态管理逻辑，所有数据由父组件传入。
 *
 * SVG 结构（viewBox 0 0 100 70）：
 *   <rect>     — 圆角矩形脸部
 *   <g> × 2   — 左右眼（ellipse + path 交叉淡变）
 *   <g>        — 嘴巴（path / ellipse 交叉淡变）
 *
 * 动画策略：
 *   - 眼睛大小变化：CSS transition on rx/ry（Q弹 cubic-bezier）
 *   - 眼睛开闭切换：CSS transition on opacity（交叉淡变）
 *   - 眨眼：CSS transform scaleY（class 触发）
 *   - 眼球追踪：父组件传入 eyeOffset，叠加到眼睛 translate
 *   - 嘴巴形状切换：CSS transition on opacity（交叉淡变）
 *   - 弹跳：CSS transform scale（父组件 RAF 驱动）
 *   - 呼吸：CSS keyframe 永续动画
 *   - 描边宽度：vector-effect: non-scaling-stroke（任意尺寸下保持一致）
 */
import { computed } from 'vue';
import type { ExpressionPreset, EyeConfig, EyeOffset, MouthConfig } from './types';

const props = defineProps<{
  /** 当前表情预设。 */
  expression: ExpressionPreset;
  /** 是否正在眨眼。 */
  isBlinking: boolean;
  /** 弹跳缩放比例（1 = 无弹跳）。 */
  bounceScale: number;
  /** 眼球偏移量（SVG 用户单位），用于鼠标追踪。 */
  eyeOffset?: EyeOffset;
}>();

/* ── SVG 布局常量 ──────────────────────────────────────────────────── */
const FACE = { x: 3, y: 3, w: 94, h: 64, rx: 18 } as const;
const LEFT_EYE = { cx: 32, cy: 27 } as const;
const RIGHT_EYE = { cx: 68, cy: 27 } as const;
const MOUTH = { cx: 50, cy: 47 } as const;

/* ── 眼睛渲染辅助 ──────────────────────────────────────────────────── */

/** 判断眼睛是否为"睁开"类型（用 ellipse 渲染）。 */
function isOpenEye(eye: EyeConfig): boolean {
  return eye.type === 'circle' || eye.type === 'oval';
}

/* ── 眼球偏移（鼠标追踪，仅睁开类型眼睛生效） ─────────────────── */
const ox = computed(() => props.eyeOffset?.x ?? 0);
const oy = computed(() => props.eyeOffset?.y ?? 0);

const leftEyeTransform = computed(() => {
  const dx = isOpenEye(props.expression.leftEye) ? ox.value : 0;
  const dy = isOpenEye(props.expression.leftEye) ? oy.value : 0;
  return `translate(${LEFT_EYE.cx + dx}, ${LEFT_EYE.cy + dy})`;
});
const rightEyeTransform = computed(() => {
  const dx = isOpenEye(props.expression.rightEye) ? ox.value : 0;
  const dy = isOpenEye(props.expression.rightEye) ? oy.value : 0;
  return `translate(${RIGHT_EYE.cx + dx}, ${RIGHT_EYE.cy + dy})`;
});

/** 生成闭眼弧线路径。SVG Y 轴向下。 */
function closedEyePath(eye: EyeConfig): string {
  const { type, rx, ry } = eye;
  switch (type) {
    case 'arc-happy':
      // ︶ 上弧（开心）— 控制点在上方
      return `M ${-rx} 0 Q 0 ${-ry} ${rx} 0`;
    case 'arc-sad':
      // ︵ 下弧（难过）— 控制点在下方
      return `M ${-rx} 0 Q 0 ${ry} ${rx} 0`;
    case 'line':
      // — 直线（睡觉）
      return `M ${-rx} 0 L ${rx} 0`;
    default:
      return '';
  }
}

/** 眨眼时 ry 缩放因子。 */
const blinkScale = computed(() => (props.isBlinking ? 0.1 : 1));

/* ── 嘴巴渲染辅助 ──────────────────────────────────────────────────── */

/** 判断嘴巴是否为弧线类型（用 path 渲染）。 */
function isArcMouth(mouth: MouthConfig): boolean {
  return mouth.type !== 'open';
}

/** 生成嘴巴弧线路径。curve > 0 = 笑，< 0 = 哭。 */
function mouthPath(mouth: MouthConfig): string {
  const { type, width, curve } = mouth;
  const half = width / 2;
  switch (type) {
    case 'smile':
    case 'smile-small':
      // ∪ 形（微笑）
      return `M ${-half} 0 Q 0 ${curve} ${half} 0`;
    case 'frown':
      // ∩ 形（不开心）
      return `M ${-half} 0 Q 0 ${curve} ${half} 0`;
    case 'neutral':
      // 直线
      return `M ${-half} 0 L ${half} 0`;
    default:
      return '';
  }
}
</script>

<template>
  <svg
    class="mascot-face"
    viewBox="0 0 100 70"
    xmlns="http://www.w3.org/2000/svg"
    :style="{ transform: `scale(${bounceScale})` }"
    aria-hidden="true"
  >
    <!-- 脸部圆角矩形 -->
    <rect
      class="mascot-face__face"
      :x="FACE.x"
      :y="FACE.y"
      :width="FACE.w"
      :height="FACE.h"
      :rx="FACE.rx"
    />

    <!-- 特征组（呼吸动画） -->
    <g class="mascot-face__features">
      <!-- ═══ 左眼 ═══ -->
      <g :transform="leftEyeTransform">
        <!-- 睁眼（ellipse） -->
        <ellipse
          class="mascot-face__eye-open"
          :rx="expression.leftEye.rx"
          :ry="expression.leftEye.ry * blinkScale"
          :style="{ opacity: isOpenEye(expression.leftEye) ? 1 : 0 }"
        />
        <!-- 闭眼（path 弧线/直线） -->
        <path
          class="mascot-face__eye-closed"
          :d="closedEyePath(expression.leftEye)"
          :style="{ opacity: isOpenEye(expression.leftEye) ? 0 : 1 }"
        />
      </g>

      <!-- ═══ 右眼 ═══ -->
      <g :transform="rightEyeTransform">
        <ellipse
          class="mascot-face__eye-open"
          :rx="expression.rightEye.rx"
          :ry="expression.rightEye.ry * blinkScale"
          :style="{ opacity: isOpenEye(expression.rightEye) ? 1 : 0 }"
        />
        <path
          class="mascot-face__eye-closed"
          :d="closedEyePath(expression.rightEye)"
          :style="{ opacity: isOpenEye(expression.rightEye) ? 0 : 1 }"
        />
      </g>

      <!-- ═══ 嘴巴 ═══ -->
      <g :transform="`translate(${MOUTH.cx}, ${MOUTH.cy})`">
        <!-- 弧线嘴巴 -->
        <path
          class="mascot-face__mouth-arc"
          :d="mouthPath(expression.mouth)"
          :style="{ opacity: isArcMouth(expression.mouth) ? 1 : 0 }"
        />
        <!-- 张嘴（椭圆） -->
        <ellipse
          class="mascot-face__mouth-open"
          :rx="expression.mouth.width / 2"
          :ry="3"
          :style="{ opacity: expression.mouth.type === 'open' ? 1 : 0 }"
        />
      </g>
    </g>
  </svg>
</template>

<style scoped>
.mascot-face {
  display: block;
  width: 100%;
  height: 100%;
  overflow: visible;
  transform-origin: center;
}

/* ── 脸部 ─────────────────────────────────────────────────────────── */
.mascot-face__face {
  fill: var(--fluen-canvas);
  stroke: var(--fluen-hairline);
  stroke-width: 1;
  vector-effect: non-scaling-stroke;
  transition: stroke 0.2s ease;
}

/* ── 特征组（呼吸动画） ───────────────────────────────────────────── */
.mascot-face__features {
  animation: mascot-breathe 3.5s ease-in-out infinite;
}

@keyframes mascot-breathe {
  0%,
  100% {
    transform: translateY(0);
  }
  50% {
    transform: translateY(-0.8px);
  }
}

/* ── 眼睛：睁开（ellipse） ────────────────────────────────────────── */
.mascot-face__eye-open {
  fill: var(--fluen-ink);
  transition:
    rx 0.3s cubic-bezier(0.34, 1.56, 0.64, 1),
    ry 0.12s ease,
    opacity 0.25s ease;
}

/* ── 眼睛：闭眼（path 弧线/直线） ─────────────────────────────────── */
.mascot-face__eye-closed {
  fill: none;
  stroke: var(--fluen-ink);
  stroke-width: 1.5;
  stroke-linecap: round;
  vector-effect: non-scaling-stroke;
  transition: opacity 0.25s ease;
}

/* ── 嘴巴：弧线 ───────────────────────────────────────────────────── */
.mascot-face__mouth-arc {
  fill: none;
  stroke: var(--fluen-ink);
  stroke-width: 1.5;
  stroke-linecap: round;
  vector-effect: non-scaling-stroke;
  transition: opacity 0.25s ease;
}

/* ── 嘴巴：张嘴（椭圆） ──────────────────────────────────────────── */
.mascot-face__mouth-open {
  fill: var(--fluen-ink);
  transition:
    opacity 0.25s ease,
    rx 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
}
</style>
