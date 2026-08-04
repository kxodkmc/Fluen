/**
 * Mascot 模块 — 眼球鼠标追踪 composable。
 *
 * 职责：
 *   - 监听 window mousemove，计算鼠标相对于容器中心的方向
 *   - 通过 RAF + lerp 平滑插值，输出 SVG 用户单位偏移量
 *   - 禁用时自动回归 {0, 0} 并停止 RAF
 *
 * 设计原则：
 *   - 与渲染层解耦 —— 仅输出偏移量，不操作 DOM
 *   - 按需启停 RAF —— 收敛后自动取消，避免空转
 *   - 可配置最大偏移与平滑系数
 *
 * @example
 * ```ts
 * const containerRef = ref<HTMLElement | null>(null);
 * const enabled = ref(true);
 * const { eyeOffset } = useEyeTracking(containerRef, enabled);
 * ```
 */

import { ref, watch, onMounted, onUnmounted, type Ref } from 'vue';
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

  /* ── mousemove → 目标偏移 ─────────────────────────────────────────── */

  function handleMouseMove(e: MouseEvent): void {
    const el = containerRef.value;
    if (!el) return;

    const rect = el.getBoundingClientRect();
    const cx = rect.left + rect.width / 2;
    const cy = rect.top + rect.height / 2;

    const dx = e.clientX - cx;
    const dy = e.clientY - cy;
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

  /* ── 禁用时回归中心 ───────────────────────────────────────────────── */

  watch(enabled, (isEnabled) => {
    if (!isEnabled) {
      targetX = 0;
      targetY = 0;
      ensureRAF();
    }
  });

  /* ── 生命周期 ─────────────────────────────────────────────────────── */

  onMounted(() => {
    window.addEventListener('mousemove', handleMouseMove, { passive: true });
  });

  onUnmounted(() => {
    window.removeEventListener('mousemove', handleMouseMove);
    if (rafId !== null) cancelAnimationFrame(rafId);
  });

  return { eyeOffset };
}
