/**
 * Mascot 模块 — 状态机 composable。
 *
 * 职责：
 *   - 状态管理（当前状态 + 切换 + 预设查询）
 *   - 表情预设运行时注册
 *   - 自动眨眼循环
 *   - 闲置微表情随机切换
 *   - 状态切换弹跳动画（RAF 阻尼弹簧）
 *
 * 设计原则：
 *   - 逻辑与渲染完全解耦 —— useMascot 不引用任何 DOM/SVG
 *   - 预设表可运行时扩展 —— registerPreset
 *   - 弹跳动画基于 requestAnimationFrame，阻尼弹簧模型
 *
 * @example
 * ```ts
 * const { state, expression, isBlinking, bounceScale, setState, blink } = useMascot();
 * setState('happy');
 * ```
 */

import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import type { ExpressionPreset, MascotState, MascotConfig } from './types';
import type { Mood } from '../../types/mascot';
import { DEFAULT_PRESETS, MOOD_REST_STATE, MOOD_MICRO_STATES } from './presets';

/** 默认配置。 */
const DEFAULT_CONFIG: MascotConfig = {
  height: 28,
  initialState: 'idle',
  blinkInterval: 4000,
  blinkDuration: 150,
  idleMicroExpressions: true,
  mood: 'neutral' as Mood,
};

export function useMascot(config?: Partial<MascotConfig>) {
  const cfg = { ...DEFAULT_CONFIG, ...config };

  /* ── 预设表（可运行时扩展） ────────────────────────────────────────── */
  const presets = ref<Record<string, ExpressionPreset>>({ ...DEFAULT_PRESETS });

  /**
   * 注册自定义状态预设。
   * 注册后即可通过 setState(stateName) 切换到该状态。
   */
  function registerPreset(stateName: string, preset: ExpressionPreset): void {
    presets.value = { ...presets.value, [stateName]: preset };
  }

  /* ── 状态 ─────────────────────────────────────────────────────────── */
  const state = ref<MascotState>(cfg.initialState);

  /** 当前表情（从预设表派生，找不到时回退到 idle）。 */
  const expression = computed<ExpressionPreset>(
    () => presets.value[state.value] ?? presets.value['idle']!,
  );

  /**
   * 切换状态。
   * @param newState 目标状态名（内置或已注册的自定义状态）
   */
  function setState(newState: MascotState | string): void {
    if (newState === state.value) return;
    if (!presets.value[newState]) return;
    state.value = newState as MascotState;
  }

  /* ── 眨眼 ─────────────────────────────────────────────────────────── */
  const isBlinking = ref(false);
  let blinkTimer: ReturnType<typeof setInterval> | null = null;

  /** 手动触发一次眨眼。 */
  function blink(): void {
    if (isBlinking.value) return;
    isBlinking.value = true;
    window.setTimeout(() => {
      isBlinking.value = false;
    }, cfg.blinkDuration);
  }

  function startBlinking(): void {
    if (cfg.blinkInterval <= 0) return;
    stopBlinking();
    blinkTimer = window.setInterval(() => {
      if (!expression.value.blink || isBlinking.value) return;
      // 随机性：60% 概率执行眨眼
      if (Math.random() < 0.6) blink();
    }, cfg.blinkInterval);
  }

  function stopBlinking(): void {
    if (blinkTimer) {
      clearInterval(blinkTimer);
      blinkTimer = null;
    }
  }

  /* ── 闲置微表情 ───────────────────────────────────────────────────── */
  let microTimer: ReturnType<typeof setTimeout> | null = null;

  function scheduleMicroExpression(): void {
    if (!cfg.idleMicroExpressions) return;
    const delay = 8000 + Math.random() * 12000;
    microTimer = window.setTimeout(() => {
      if (state.value === 'idle') {
        const microStates = MOOD_MICRO_STATES[cfg.mood] ?? MOOD_MICRO_STATES['neutral']!;
        const pick = microStates[Math.floor(Math.random() * microStates.length)]!;
        setState(pick);
        window.setTimeout(() => {
          setState(MOOD_REST_STATE[cfg.mood] ?? 'idle');
          scheduleMicroExpression();
        }, 2000 + Math.random() * 2000);
      } else {
        scheduleMicroExpression();
      }
    }, delay);
  }

  /* ── 弹跳动画（RAF 阻尼弹簧） ─────────────────────────────────────── */
  /**
   * 弹跳缩放比例（供渲染层绑定）。
   * 状态切换时由阻尼弹簧模型驱动，产生 Q 弹效果。
   */
  const bounceScale = ref(1);
  let bounceRAF: number | null = null;

  function triggerBounce(intensity: number): void {
    if (bounceRAF !== null) cancelAnimationFrame(bounceRAF);
    if (intensity <= 0) {
      bounceScale.value = 1;
      return;
    }

    const duration = 600;
    const startTime = performance.now();
    const amplitude = intensity * 0.08; // 最大 8% 缩放

    function step(now: number): void {
      const t = Math.min((now - startTime) / duration, 1);
      if (t >= 1) {
        bounceScale.value = 1;
        bounceRAF = null;
        return;
      }
      // 阻尼弹簧：1 + A * e^(-3t) * cos(8t)
      bounceScale.value = 1 + amplitude * Math.exp(-3 * t) * Math.cos(8 * t);
      bounceRAF = requestAnimationFrame(step);
    }

    bounceRAF = requestAnimationFrame(step);
  }

  // 状态变化时触发弹跳
  watch(state, (newState) => {
    const preset = presets.value[newState] ?? presets.value['idle'];
    if (preset) triggerBounce(preset.bounce);
  });

  /* ── 生命周期 ─────────────────────────────────────────────────────── */
  onMounted(() => {
    startBlinking();
    scheduleMicroExpression();
  });

  onUnmounted(() => {
    stopBlinking();
    if (microTimer) clearTimeout(microTimer);
    if (bounceRAF !== null) cancelAnimationFrame(bounceRAF);
  });

  return {
    /** 当前状态。 */
    state,
    /** 当前表情（从预设派生）。 */
    expression,
    /** 是否正在眨眼。 */
    isBlinking,
    /** 弹跳缩放比例（供渲染层绑定）。 */
    bounceScale,
    /** 合并后的配置。 */
    config: cfg,
    /** 切换状态。 */
    setState,
    /** 手动触发眨眼。 */
    blink,
    /** 注册自定义状态预设。 */
    registerPreset,
  };
}
