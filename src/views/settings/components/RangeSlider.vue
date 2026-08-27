<script setup lang="ts">
/**
 * RangeSlider — 设置页通用拟物滑块。
 *
 * 设计要点：
 * - 凹槽轨道（inset 阴影）+ 凸起旋钮（径向渐变高光），轻度拟物；
 * - 非拖拽状态（点击轨道 / 键盘调节）带弹簧过渡，拖拽时 1:1 实时跟随；
 * - 悬停 / 聚焦 / 拖拽时旋钮上方浮现数值气泡；
 * - 透明原生 input 覆盖在视觉层之上，保留键盘与无障碍语义。
 *
 * 深浅色模式通过 `:root[data-theme='dark']` 覆写局部阴影变量实现。
 */
import { computed, ref } from 'vue';

interface Props {
  /** 关联 label 的 id。 */
  id?: string;
  /** 当前值。 */
  modelValue: number;
  /** 最小值。 */
  min: number;
  /** 最大值。 */
  max: number;
  /** 步长。 */
  step?: number;
  /** 禁用状态。 */
  disabled?: boolean;
  /** 是否显示两端刻度标记。 */
  showMarks?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  id: undefined,
  step: 1,
  disabled: false,
  showMarks: true,
});

const emit = defineEmits<{
  (e: 'update:modelValue', value: number): void;
  (e: 'change'): void;
}>();

/** 是否正在拖拽（拖拽时关闭位置过渡，保证跟手）。 */
const dragging = ref(false);

/** 当前值在 [min, max] 中的占比（0-1）。 */
const pct = computed(() => {
  const span = props.max - props.min;
  if (span <= 0) return 0;
  return Math.min(1, Math.max(0, (props.modelValue - props.min) / span));
});

function onInput(event: Event): void {
  const target = event.target as HTMLInputElement;
  emit('update:modelValue', Number(target.value));
}

function startDrag(): void {
  dragging.value = true;
}

function endDrag(): void {
  dragging.value = false;
}
</script>

<template>
  <div
    class="range-slider"
    :class="{ 'is-dragging': dragging, 'is-disabled': disabled }"
    :style="{ '--rs-pct': pct }"
  >
    <div class="range-slider__body">
      <input
        :id="id"
        class="range-slider__input"
        type="range"
        :min="min"
        :max="max"
        :step="step"
        :value="modelValue"
        :disabled="disabled"
        @input="onInput"
        @change="emit('change')"
        @pointerdown="startDrag"
        @pointerup="endDrag"
        @pointercancel="endDrag"
        @lostpointercapture="endDrag"
        @blur="endDrag"
      />
      <div class="range-slider__visual" aria-hidden="true">
        <div class="range-slider__track">
          <div class="range-slider__fill" />
        </div>
        <div class="range-slider__knob">
          <span class="range-slider__bubble">{{ modelValue }}</span>
        </div>
      </div>
    </div>
    <div v-if="showMarks" class="range-slider__marks">
      <span>{{ min }}</span>
      <span>{{ max }}</span>
    </div>
  </div>
</template>

<style scoped>
/* ── 主题变量（深浅色各自调校凹槽与旋钮的立体质感） ────────────────── */
.range-slider {
  --rs-groove: var(--fluen-surface-deep);
  --rs-groove-inset:
    inset 0 1.5px 3px rgba(0, 0, 0, 0.14),
    inset 0 -1px 0 rgba(255, 255, 255, 0.8);
  --rs-knob-gloss: rgba(255, 255, 255, 0.9);
  --rs-spring: cubic-bezier(0.22, 1.2, 0.36, 1);

  display: flex;
  flex-direction: column;
  width: 100%;
}

:root[data-theme='dark'] .range-slider {
  --rs-groove-inset:
    inset 0 1.5px 3px rgba(0, 0, 0, 0.6),
    inset 0 -1px 0 rgba(255, 255, 255, 0.07);
  --rs-knob-gloss: rgba(255, 255, 255, 0.16);
}

/* ── 布局 ──────────────────────────────────────────────────────────── */
.range-slider__body {
  position: relative;
  height: 28px;
}

.range-slider.is-disabled {
  opacity: 0.55;
}

/* ── 原生输入（透明覆盖层，负责交互与无障碍） ──────────────────────── */
.range-slider__input {
  position: absolute;
  inset: 0;
  z-index: 2;
  width: 100%;
  height: 100%;
  margin: 0;
  appearance: none;
  -webkit-appearance: none;
  background: transparent;
  opacity: 0;
  cursor: pointer;
}

.range-slider__input:disabled {
  cursor: not-allowed;
}

/* ── 视觉层 ────────────────────────────────────────────────────────── */
.range-slider__visual {
  position: absolute;
  inset: 0;
  z-index: 1;
  pointer-events: none;
}

/* 凹槽轨道 */
.range-slider__track {
  position: absolute;
  top: 50%;
  left: 0;
  right: 0;
  height: 10px;
  transform: translateY(-50%);
  border-radius: 9999px;
  background: var(--rs-groove);
  box-shadow: var(--rs-groove-inset);
}

/* 已填充段 */
.range-slider__fill {
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  width: calc(12px + (100% - 24px) * var(--rs-pct, 0));
  border-radius: inherit;
  background: var(--fluen-accent);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.25);
  transition: width 0.28s var(--rs-spring);
}

/* 旋钮 */
.range-slider__knob {
  position: absolute;
  top: 50%;
  left: calc(12px + (100% - 24px) * var(--rs-pct, 0));
  width: 24px;
  height: 24px;
  transform: translate(-50%, -50%) scale(1);
  border-radius: 50%;
  border: 1px solid var(--fluen-hairline);
  background: radial-gradient(
    130% 130% at 50% 0%,
    var(--fluen-canvas) 30%,
    var(--fluen-surface-deep) 100%
  );
  box-shadow:
    inset 0 1px 0 var(--rs-knob-gloss),
    0 1px 2px rgba(0, 0, 0, 0.16),
    0 5px 12px rgba(0, 0, 0, 0.12);
  transition:
    left 0.28s var(--rs-spring),
    transform 0.16s ease,
    box-shadow 0.16s ease;
}

.range-slider__knob::after {
  content: '';
  position: absolute;
  left: 50%;
  top: 50%;
  width: 6px;
  height: 6px;
  transform: translate(-50%, -50%);
  border-radius: 50%;
  background: var(--fluen-accent);
  box-shadow: inset 0 1px 1px rgba(0, 0, 0, 0.18);
}

.range-slider__input:hover:not(:disabled) + .range-slider__visual .range-slider__knob {
  transform: translate(-50%, -50%) scale(1.08);
}

.range-slider__input:active:not(:disabled) + .range-slider__visual .range-slider__knob {
  transform: translate(-50%, -50%) scale(1.14);
  box-shadow:
    inset 0 1px 0 var(--rs-knob-gloss),
    0 1px 2px rgba(0, 0, 0, 0.22);
}

.range-slider__input:focus-visible + .range-slider__visual .range-slider__knob {
  outline: 2px solid var(--fluen-accent);
  outline-offset: 2px;
}

/* 拖拽时关闭位置过渡，保证跟手 */
.range-slider.is-dragging .range-slider__fill {
  transition: none;
}

.range-slider.is-dragging .range-slider__knob {
  transition: transform 0.16s ease, box-shadow 0.16s ease;
}

/* ── 数值气泡 ──────────────────────────────────────────────────────── */
.range-slider__bubble {
  position: absolute;
  left: 50%;
  bottom: calc(100% + 7px);
  transform: translateX(-50%) translateY(5px) scale(0.8);
  padding: 2px 7px;
  border-radius: 6px;
  background: var(--fluen-ink);
  color: var(--fluen-canvas);
  font-family: var(--fluen-font-mono);
  font-size: 0.7rem;
  font-weight: 600;
  line-height: 1.4;
  white-space: nowrap;
  opacity: 0;
  transition:
    opacity 0.18s ease,
    transform 0.22s var(--rs-spring);
}

.range-slider__bubble::after {
  content: '';
  position: absolute;
  left: 50%;
  top: 100%;
  width: 6px;
  height: 6px;
  margin-top: -3px;
  transform: translateX(-50%) rotate(45deg);
  background: var(--fluen-ink);
}

.range-slider:hover:not(.is-disabled) .range-slider__bubble,
.range-slider:focus-within .range-slider__bubble,
.range-slider.is-dragging .range-slider__bubble {
  opacity: 1;
  transform: translateX(-50%) translateY(0) scale(1);
}

/* ── 两端刻度标记 ──────────────────────────────────────────────────── */
.range-slider__marks {
  display: flex;
  justify-content: space-between;
  padding: 0 4px;
  margin-top: 0.35rem;
  font-family: var(--fluen-font-mono);
  font-size: 0.7rem;
  color: var(--fluen-muted);
}
</style>
