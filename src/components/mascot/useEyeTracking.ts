/**
 * Mascot 模块 — 眼球鼠标追踪 composable。
 *
 * 职责：
 *   - 订阅全局鼠标位置（useMousePosition），计算鼠标相对于容器中心的方向
 *   - 通过 RAF + lerp 平滑插值，输出 SVG 用户单位偏移量
 *   - 禁用时自动回归 {0, 0} 并停止 RAF
 *
 * 设计原则：
 *   - 与渲染层解耦 —— 仅输出偏移量，不操作 DOM
 *   - 按需启停 RAF —— 收敛后自动取消，避免空转
 *   - 可配置最大偏移与平滑系数
 *   - 数据来源透明 —— 通过 useMousePosition 统一接收 window / iframe 上报的鼠标位置，
 *     iframe 内的 mousemove 经 postMessage 转发后也能驱动眼球追踪
 *
 * @example
 * ```ts
 * const containerRef = ref<HTMLElement | null>(null);
 * const enabled = ref(true);
 * const { eyeOffset } = useEyeTracking(containerRef, enabled);
 * ```
 */

import { ref, watch, onUnmounted, type Ref } from 'vue';
import { useMousePosition } from '../../composables/useMousePosition';
import type { EyeOffset } from './types';

export interface EyeTrackingOptions {
  /** 最大偏移量（SVG 用户单位）。 */
  maxOffset: number;
  /** 平滑系数（0–1，越大越灵敏）。 */
  smoothing: number;
  /** 鼠标距中心多少 px 时达到最大偏移。 */
  fullTrackDist: number;
}

const DEFAULT_OPTIONS: EyeTrackingOptions = {
  maxOffset: 2.5,
  smoothing: 0.12,
  fullTrackDist: 120,
};

/** 零偏移常量。 */
const ZERO_OFFSET: EyeOffset = { x: 0, y: 0 };

/** 收敛阈值 —— 偏移量与目标距离小于此值时视为已到达。 */
const CONVERGENCE = 0.01;

export function useEyeTracking(
  containerRef: Ref<HTMLElement | null>,
  enabled: Ref<boolean>,
  options?: Partial<EyeTrackingOptions>,
): { eyeOffset: Ref<EyeOffset> } {
  const opts = { ...DEFAULT_OPTIONS, ...options };

  /** 全局鼠标位置（来自 useMousePosition 单例，含 iframe 上报）。 */
  const { mouseX, mouseY } = useMousePosition();

  /** 当前眼球偏移（供渲染层绑定）。 */
  const eyeOffset = ref<EyeOffset>({ ...ZERO_OFFSET });

  let targetX = 0;
  let targetY = 0;
  let rafId: number | null = null;

  /* ── RAF 平滑插值 ─────────────────────────────────────────────────── */

  function ensureRAF(): void {
    if (rafId === null) {
      rafId = requestAnimationFrame(step);
    }
  }

  function step(): void {
    const cur = eyeOffset.value;
    const dx = targetX - cur.x;
    const dy = targetY - cur.y;

    if (Math.abs(dx) < CONVERGENCE && Math.abs(dy) < CONVERGENCE) {
      // 收敛 —— 直接对齐并停止 RAF
      eyeOffset.value = { x: targetX, y: targetY };
      rafId = null;
      return;
    }

    eyeOffset.value = {
      x: cur.x + dx * opts.smoothing,
      y: cur.y + dy * opts.smoothing,
    };
    rafId = requestAnimationFrame(step);
  }

  /* ── 鼠标位置变化 → 目标偏移 ──────────────────────────────────────── */

  function updateTarget(): void {
    const el = containerRef.value;
    if (!el) return;

    const rect = el.getBoundingClientRect();
    const cx = rect.left + rect.width / 2;
    const cy = rect.top + rect.height / 2;

    const dx = mouseX.value - cx;
    const dy = mouseY.value - cy;
    const dist = Math.hypot(dx, dy);

    if (dist < 1) {
      targetX = 0;
      targetY = 0;
    } else {
      // 单位向量 × 距离衰减系数 × 最大偏移
      const factor = Math.min(dist / opts.fullTrackDist, 1) * opts.maxOffset;
      targetX = (dx / dist) * factor;
      targetY = (dy / dist) * factor;
    }

    if (enabled.value) ensureRAF();
  }

  // 订阅全局鼠标位置变化（覆盖 window mousemove 与 iframe mousemove 上报两种来源）
  // flush: 'post' 确保 DOM 更新后再读取 bounding rect（containerRef 可能因布局变化而位移）
  const stopWatch = watch([mouseX, mouseY], updateTarget, { flush: 'post' });

  /* ── 禁用时回归中心 ───────────────────────────────────────────────── */

  const stopEnabledWatch = watch(enabled, (isEnabled) => {
    if (!isEnabled) {
      targetX = 0;
      targetY = 0;
      ensureRAF();
    }
  });

  onUnmounted(() => {
    stopWatch();
    stopEnabledWatch();
    if (rafId !== null) cancelAnimationFrame(rafId);
  });

  return { eyeOffset };
}
