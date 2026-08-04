<script setup lang="ts">
/**
 * Mascot — 桌面宠物主组件。
 *
 * 将 useMascot（逻辑层）与 MascotFace（渲染层）组合为开箱即用的组件。
 *
 * 交互行为：
 *   - hover：切换为 happy 状态
 *   - click：循环切换临时表情（wink → excited → surprised → happy → ...）
 *   - 临时表情持续 ~3s 后自动回归 idle（hover 中则回到 happy）
 *   - 眼球追踪：始终启用，偏移仅对睁开类型眼睛生效（渲染层自动过滤）
 *
 * 扩展接口：
 *   - 通过 ref 可调用 setState、blink、registerPreset
 *   - 通过 props 可控制高度、初始状态、眨眼频率等
 *   - 通过 statusBubble prop 可在宠物上方显示状态气泡（淡入淡出）
 *   - 通过 registerPreset 可注册全新自定义状态
 *
 * @example
 * ```vue
 * <Mascot :height="28" status-bubble="正在思考…" @click="onMascotClick" />
 *
 * const mascotRef = ref();
 * mascotRef.value?.setState('excited');
 * ```
 */
import { ref, onUnmounted } from 'vue';
import MascotFace from './MascotFace.vue';
import { useMascot } from './useMascot';
import { useEyeTracking } from './useEyeTracking';
import { MOOD_REST_STATE } from './presets';
import type { MascotState } from './types';
import type { Mood } from '../../types/mascot';

const props = withDefaults(
  defineProps<{
    /** 整体高度（px），宽度按 100:70 比例自动计算。 */
    height?: number;
    /** 初始状态。 */
    initialState?: MascotState;
    /** 自动眨眼间隔（ms），0 = 禁用。 */
    blinkInterval?: number;
    /** 是否启用闲置微表情。 */
    idleMicroExpressions?: boolean;
    /** 是否响应点击/hover 交互。 */
    interactive?: boolean;
    /** 当前心情（驱动 rest state 与微表情池）。 */
    mood?: Mood;
    /** 状态气泡文本（非空时在宠物上方显示气泡，清空时淡出）。 */
    statusBubble?: string;
  }>(),
  {
    height: 28,
    initialState: 'idle',
    blinkInterval: 4000,
    idleMicroExpressions: true,
    interactive: true,
    mood: 'neutral',
    statusBubble: '',
  },
);

const emit = defineEmits<{
  (e: 'state-change', state: MascotState): void;
  (e: 'click'): void;
}>();

const { state, expression, isBlinking, bounceScale, setState, blink, registerPreset } = useMascot({
  height: props.height,
  initialState: props.initialState,
  blinkInterval: props.blinkInterval,
  idleMicroExpressions: props.idleMicroExpressions,
  mood: props.mood,
});

/* ── 眼球鼠标追踪 ─────────────────────────────────────────────────── */
/** 容器元素引用（供 useEyeTracking 获取位置）。 */
const mascotEl = ref<HTMLElement | null>(null);
/** 追踪始终启用 —— 渲染层自动仅对睁开类型眼睛应用偏移。 */
const { eyeOffset } = useEyeTracking(mascotEl, ref(true));

/* ── 交互逻辑 ─────────────────────────────────────────────────────── */

/** 临时表情持续时间（ms）。 */
const TRANSIENT_MS = 3000;

/** 是否正在 hover。 */
const isHovering = ref(false);

/** 点击循环表情序列。 */
const clickCycle: MascotState[] = ['wink', 'excited', 'surprised', 'happy'];
let cycleIndex = 0;

/** 临时表情回退计时器。 */
let revertTimer: ReturnType<typeof setTimeout> | null = null;

/** 清除回退计时器。 */
function clearRevert(): void {
  if (revertTimer) {
    clearTimeout(revertTimer);
    revertTimer = null;
  }
}

/** 显示临时表情，到期后自动回归 mood 对应的 rest state（或 hover 中的 happy）。 */
function showTransient(s: MascotState): void {
  clearRevert();
  setState(s);
  emit('state-change', s);
  revertTimer = setTimeout(() => {
    revertTimer = null;
    const next = isHovering.value ? 'happy' : (MOOD_REST_STATE[props.mood] ?? 'idle');
    setState(next);
    emit('state-change', next);
  }, TRANSIENT_MS);
}

function handleClick(): void {
  emit('click');
  if (!props.interactive) return;
  const next = clickCycle[cycleIndex % clickCycle.length]!;
  cycleIndex++;
  showTransient(next);
}

function handleMouseEnter(): void {
  isHovering.value = true;
  if (!props.interactive) return;
  // 临时表情进行中时不覆盖
  if (revertTimer) return;
  setState('happy');
  emit('state-change', 'happy');
}

function handleMouseLeave(): void {
  isHovering.value = false;
  if (!props.interactive) return;
  // 临时表情进行中时等计时器处理
  if (revertTimer) return;
  const next = MOOD_REST_STATE[props.mood] ?? 'idle';
  setState(next);
  emit('state-change', next);
}

onUnmounted(clearRevert);

/* ── 暴露接口 ─────────────────────────────────────────────────────── */
defineExpose({
  /** 切换状态。 */
  setState,
  /** 手动触发眨眼。 */
  blink,
  /** 注册自定义状态预设。 */
  registerPreset,
  /** 当前状态（只读 ref）。 */
  state,
  /** 当前表情（只读 computed）。 */
  expression,
});
</script>

<template>
  <div
    ref="mascotEl"
    class="mascot"
    :class="{ 'mascot--interactive': interactive }"
    :style="{ height: height + 'px' }"
    data-tauri-drag-region="false"
    @click="handleClick"
    @mouseenter="handleMouseEnter"
    @mouseleave="handleMouseLeave"
  >
    <Transition name="mascot-bubble">
      <div
        v-if="statusBubble"
        class="mascot__bubble"
        data-tauri-drag-region="false"
      >
        {{ statusBubble }}
      </div>
    </Transition>
    <MascotFace
      :expression="expression"
      :is-blinking="isBlinking"
      :bounce-scale="bounceScale"
      :eye-offset="eyeOffset"
    />
  </div>
</template>

<style scoped>
.mascot {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  /* viewBox 100:70 → 宽度按比例自适应 */
  aspect-ratio: 100 / 70;
  cursor: default;
  overflow: visible;
  -webkit-app-region: no-drag;
  user-select: none;
}

.mascot--interactive {
  cursor: pointer;
}

.mascot--interactive:hover :deep(.mascot-face__face) {
  stroke: var(--fluen-accent);
}

/* ── 状态气泡 ─────────────────────────────────────────────────────── */
/* 圆角矩形气泡，带小三角指向宠物。显示在宠物**下方**（Motis 位于标题栏，
   上方会被窗口边界遮挡）。遵循 DESIGN.md：rounded.lg(12px)、accent 边框、
   canvas 背景、card 阴影、DM Sans 字体。适配深色/浅色主题。 */
.mascot__bubble {
  position: absolute;
  top: calc(100% + 8px);
  left: 50%;
  transform: translateX(-50%);
  width: max-content;
  min-width: 200px;
  max-width: 520px;
  padding: 10px 16px;
  border: 1px solid var(--fluen-accent);
  border-radius: 16px;
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  font-weight: 400;
  line-height: 1.55;
  text-align: left;
  white-space: normal;
  word-break: break-word;
  overflow: hidden;
  box-shadow:
    0 4px 12px rgba(0, 0, 0, 0.12),
    0 0 0 1px rgba(255, 255, 255, 0.06) inset;
  pointer-events: none;
  -webkit-app-region: no-drag;
  user-select: none;
  z-index: 10;
}

/* 小三角指向宠物（旋转方块裁出两边的边框），位于气泡上方 */
.mascot__bubble::after {
  content: '';
  position: absolute;
  top: -6px;
  left: 50%;
  width: 10px;
  height: 10px;
  background: var(--fluen-canvas);
  border-left: 1px solid var(--fluen-accent);
  border-top: 1px solid var(--fluen-accent);
  transform: translateX(-50%) rotate(45deg);
}

/* ── 淡入淡出动画（纯 CSS transition） ────────────────────────────── */
/* 200ms ease，符合 DESIGN.md 推荐的交互态过渡时长。 */
.mascot-bubble-enter-active,
.mascot-bubble-leave-active {
  transition:
    opacity 200ms ease,
    transform 200ms ease;
}

.mascot-bubble-enter-from,
.mascot-bubble-leave-to {
  opacity: 0;
  /* 保留 translateX(-50%) 居中，叠加轻微上浮 + 缩放 */
  transform: translateX(-50%) translateY(-4px) scale(0.96);
}
</style>
