<script lang="ts">
import type { Component } from 'vue';

export interface ElasticSliderProps {
  /** The initial value of the slider. Can be less than startingValue or greater than maxValue. */
  defaultValue?: number;
  /** The starting point for the slider's range, e.g. startingValue=100 starts the slider at 100. */
  startingValue?: number;
  /** The maximum value the slider can reach. */
  maxValue?: number;
  /** Custom class name for the root container. */
  className?: string;
  /** Enables stepped increments on the slider. */
  isStepped?: boolean;
  /** The size of the increments when isStepped is enabled. */
  stepSize?: number;
  /** Custom content for the left icon (component or string). Overridden by the #left-icon slot. */
  leftIcon?: Component | string;
  /** Custom content for the right icon (component or string). Overridden by the #right-icon slot. */
  rightIcon?: Component | string;
  /** Accent color for the slider fill bar. */
  accentColor?: string;
}

export type ElasticSliderEmits = {
  /** Emitted whenever the value changes during dragging. */
  (e: 'update:value', value: number): void;
  /** Emitted when the user releases the slider. */
  (e: 'change', value: number): void;
};
</script>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, useTemplateRef } from 'vue';

/* ------------------------------------------------------------------ *
 * Constants
 * ------------------------------------------------------------------ */

const MAX_OVERFLOW = 50;

/* ------------------------------------------------------------------ *
 * Props & Emits
 * ------------------------------------------------------------------ */

const props = withDefaults(defineProps<ElasticSliderProps>(), {
  defaultValue: 50,
  startingValue: 0,
  maxValue: 100,
  className: '',
  isStepped: false,
  stepSize: 1,
  leftIcon: '-',
  rightIcon: '+',
  accentColor: 'var(--fluen-accent)'
});

const emit = defineEmits<ElasticSliderEmits>();

/* ------------------------------------------------------------------ *
 * Template refs
 * ------------------------------------------------------------------ */

const sliderRef = useTemplateRef<HTMLDivElement>('sliderRef');

/* ------------------------------------------------------------------ *
 * Reactive state
 * ------------------------------------------------------------------ */

const value = ref(props.defaultValue);
const region = ref<'left' | 'middle' | 'right'>('middle');
const clientX = ref(0);
const overflow = ref(0);
const scale = ref(1);
const leftIconScale = ref(1);
const rightIconScale = ref(1);

let scaleAnimation: number | null = null;
let overflowAnimation: number | null = null;

/* ------------------------------------------------------------------ *
 * Watchers
 * ------------------------------------------------------------------ */

watch(
  () => props.defaultValue,
  (newValue) => {
    value.value = newValue;
  }
);

watch(clientX, (latest) => {
  if (sliderRef.value) {
    const { left, right } = sliderRef.value.getBoundingClientRect();
    let newValue: number;

    if (latest < left) {
      region.value = 'left';
      newValue = left - latest;
    } else if (latest > right) {
      region.value = 'right';
      newValue = latest - right;
    } else {
      region.value = 'middle';
      newValue = 0;
    }

    overflow.value = decay(newValue, MAX_OVERFLOW);
  }
});

watch(region, (newRegion, oldRegion) => {
  if (newRegion === 'left' && oldRegion !== 'left') {
    animateIconScale(leftIconScale, true);
  } else if (newRegion === 'right' && oldRegion !== 'right') {
    animateIconScale(rightIconScale, true);
  }
});

/* ------------------------------------------------------------------ *
 * Computed
 * ------------------------------------------------------------------ */

const rangePercentage = computed(() => {
  const totalRange = props.maxValue - props.startingValue;
  if (totalRange === 0) return 0;
  return ((value.value - props.startingValue) / totalRange) * 100;
});

const sliderScaleX = computed(() => {
  if (!sliderRef.value) return 1;
  const { width } = sliderRef.value.getBoundingClientRect();
  return 1 + overflow.value / width;
});

const sliderScaleY = computed(() => {
  const t = overflow.value / MAX_OVERFLOW;
  return 1 + t * (0.8 - 1);
});

const transformOrigin = computed(() => {
  if (!sliderRef.value) return 'center';
  const { left, width } = sliderRef.value.getBoundingClientRect();
  return clientX.value < left + width / 2 ? 'right' : 'left';
});

const sliderHeight = computed(() => {
  const t = (scale.value - 1) / (1.2 - 1);
  return 6 + t * (12 - 6);
});

const sliderMarginTop = computed(() => {
  const t = (scale.value - 1) / (1.2 - 1);
  return 0 + t * (-3 - 0);
});

const sliderMarginBottom = computed(() => {
  const t = (scale.value - 1) / (1.2 - 1);
  return 0 + t * (-3 - 0);
});

const sliderOpacity = computed(() => {
  const t = (scale.value - 1) / (1.2 - 1);
  return 0.7 + t * (1 - 0.7);
});

const leftIconTranslateX = computed(() => {
  return region.value === 'left' ? -overflow.value / scale.value : 0;
});

const rightIconTranslateX = computed(() => {
  return region.value === 'right' ? overflow.value / scale.value : 0;
});

/* ------------------------------------------------------------------ *
 * Math helpers
 * ------------------------------------------------------------------ */

/** Sigmoid-based decay so overflow saturates smoothly at `max`. */
const decay = (inputValue: number, max: number): number => {
  if (max === 0) return 0;
  const entry = inputValue / max;
  const sigmoid = 2 * (1 / (1 + Math.exp(-entry)) - 0.5);
  return sigmoid * max;
};

/* ------------------------------------------------------------------ *
 * Animation utilities
 * ------------------------------------------------------------------ */

type AnimatableRef = { value: number };

interface AnimationOptions {
  type?: 'tween' | 'spring';
  bounce?: number;
  duration?: number;
}

const animate = (target: AnimatableRef, to: number, options: AnimationOptions = {}): number => {
  const { type = 'tween', bounce = 0, duration = 0.3 } = options;

  if (type === 'spring') {
    return animateSpring(target, to, bounce, duration);
  }
  return animateValue(target, to, duration * 1000);
};

/** Cubic ease-out tween. */
const animateValue = (target: AnimatableRef, to: number, duration = 300): number => {
  const start = target.value;
  const diff = to - start;
  const startTime = performance.now();

  const animateFrame = (currentTime: number): number | null => {
    const elapsed = currentTime - startTime;
    const progress = Math.min(elapsed / duration, 1);
    const easeOut = 1 - Math.pow(1 - progress, 3);

    target.value = start + diff * easeOut;

    if (progress < 1) {
      return requestAnimationFrame(animateFrame);
    }
    return null;
  };

  return requestAnimationFrame(animateFrame);
};

/** Damped spring animation with configurable bounce. */
const animateSpring = (
  target: AnimatableRef,
  to: number,
  bounce = 0.5,
  duration = 600
): number => {
  const start = target.value;
  const startTime = performance.now();

  const mass = 1;
  const stiffness = 170;
  const damping = 26 * (1 - bounce);

  const dampingRatio = damping / (2 * Math.sqrt(mass * stiffness));
  const angularFreq = Math.sqrt(stiffness / mass);
  const dampedFreq = angularFreq * Math.sqrt(1 - dampingRatio * dampingRatio);

  const animateFrame = (currentTime: number): number | null => {
    const elapsed = currentTime - startTime;
    const t = elapsed / 1000;

    let displacement: number;

    if (dampingRatio < 1) {
      const envelope = Math.exp(-dampingRatio * angularFreq * t);
      const cos = Math.cos(dampedFreq * t);
      const sin = Math.sin(dampedFreq * t);
      displacement = envelope * (cos + ((dampingRatio * angularFreq) / dampedFreq) * sin);
    } else {
      displacement = Math.exp(-angularFreq * t);
    }

    const currentValue = to + (start - to) * displacement;
    target.value = currentValue;

    const velocity = Math.abs(currentValue - to);
    const isSettled = velocity < 0.01 && elapsed > 100;

    if (!isSettled && elapsed < duration * 3) {
      return requestAnimationFrame(animateFrame);
    }

    target.value = to;
    return null;
  };

  return requestAnimationFrame(animateFrame);
};

/** Pop-and-settle animation for icons when entering an overflow region. */
const animateIconScale = (target: AnimatableRef, isActive: boolean): void => {
  if (isActive) {
    animate(target, 1.4, { duration: 0.125 });
    setTimeout(() => animate(target, 1, { duration: 0.125 }), 125);
  } else {
    animate(target, 1, { duration: 0.25 });
  }
};

/* ------------------------------------------------------------------ *
 * Pointer / touch handlers
 * ------------------------------------------------------------------ */

const handlePointerMove = (e: PointerEvent): void => {
  if (e.buttons > 0 && sliderRef.value) {
    const { left, width } = sliderRef.value.getBoundingClientRect();

    let newValue =
      props.startingValue + ((e.clientX - left) / width) * (props.maxValue - props.startingValue);

    if (props.isStepped) {
      newValue = Math.round(newValue / props.stepSize) * props.stepSize;
    }

    newValue = Math.min(Math.max(newValue, props.startingValue), props.maxValue);
    value.value = newValue;
    emit('update:value', newValue);

    clientX.value = e.clientX;
  }
};

const handlePointerDown = (e: PointerEvent): void => {
  handlePointerMove(e);
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
};

const handlePointerUp = (): void => {
  if (overflowAnimation) {
    cancelAnimationFrame(overflowAnimation);
  }
  overflowAnimation = animate(overflow, 0, { type: 'spring', bounce: 0.4, duration: 500 });
  emit('change', value.value);
};

const handleMouseEnter = (): void => {
  if (scaleAnimation) {
    cancelAnimationFrame(scaleAnimation);
  }
  scaleAnimation = animate(scale, 1.2, { duration: 0.2 });
};

const handleMouseLeave = (): void => {
  if (scaleAnimation) {
    cancelAnimationFrame(scaleAnimation);
  }
  scaleAnimation = animate(scale, 1, { duration: 0.2 });
};

const handleTouchStart = (): void => {
  if (scaleAnimation) {
    cancelAnimationFrame(scaleAnimation);
  }
  scaleAnimation = animate(scale, 1.2, { duration: 0.2 });
};

const handleTouchEnd = (): void => {
  if (scaleAnimation) {
    cancelAnimationFrame(scaleAnimation);
  }
  scaleAnimation = animate(scale, 1, { duration: 0.2 });
};

/* ------------------------------------------------------------------ *
 * Lifecycle
 * ------------------------------------------------------------------ */

onMounted(() => {
  value.value = props.defaultValue;
});

onUnmounted(() => {
  if (scaleAnimation) {
    cancelAnimationFrame(scaleAnimation);
    scaleAnimation = null;
  }
  if (overflowAnimation) {
    cancelAnimationFrame(overflowAnimation);
    overflowAnimation = null;
  }
});
</script>

<template>
  <div :class="['es-root', className]">
    <div
      class="es-row"
      :style="{
        scale: scale,
        opacity: sliderOpacity
      }"
      @mouseenter="handleMouseEnter"
      @mouseleave="handleMouseLeave"
      @touchstart="handleTouchStart"
      @touchend="handleTouchEnd"
    >
      <!-- Left icon -->
      <div
        class="es-icon"
        :style="{
          transform: `translateX(${leftIconTranslateX}px) scale(${leftIconScale})`
        }"
      >
        <slot name="left-icon">
          <component
            :is="leftIcon"
            v-if="leftIcon && typeof leftIcon === 'object'"
          />
          <span v-else-if="leftIcon">{{ leftIcon }}</span>
          <span v-else>-</span>
        </slot>
      </div>

      <!-- Slider track -->
      <div
        ref="sliderRef"
        class="es-slider"
        @pointermove="handlePointerMove"
        @pointerdown="handlePointerDown"
        @pointerup="handlePointerUp"
      >
        <div
          class="es-track-wrapper"
          :style="{
            transform: `scaleX(${sliderScaleX}) scaleY(${sliderScaleY})`,
            transformOrigin: transformOrigin,
            height: `${sliderHeight}px`,
            marginTop: `${sliderMarginTop}px`,
            marginBottom: `${sliderMarginBottom}px`
          }"
        >
          <div class="es-track">
            <div class="es-fill" :style="{ width: `${rangePercentage}%` }" />
          </div>
        </div>
      </div>

      <!-- Right icon -->
      <div
        class="es-icon"
        :style="{
          transform: `translateX(${rightIconTranslateX}px) scale(${rightIconScale})`
        }"
      >
        <slot name="right-icon">
          <component
            :is="rightIcon"
            v-if="rightIcon && typeof rightIcon === 'object'"
          />
          <span v-else-if="rightIcon">{{ rightIcon }}</span>
          <span v-else>+</span>
        </slot>
      </div>
    </div>

    <!-- Value readout -->
    <p class="es-value">{{ Math.round(value) }}</p>
  </div>
</template>

<style scoped>
/* Self-contained styles (no Tailwind required) — keeps the preset portable. */

.es-root {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 1rem;
  width: 12rem;
}

/* ── Slider row (icons + track) ────────────────────────────────── */

.es-row {
  display: flex;
  width: 100%;
  align-items: center;
  justify-content: center;
  gap: 1rem;
  touch-action: none;
  user-select: none;
}

/* ── Icons ─────────────────────────────────────────────────────── */

.es-icon {
  transition: transform 200ms ease-out;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

/* ── Slider track area ─────────────────────────────────────────── */

.es-slider {
  position: relative;
  display: flex;
  flex-grow: 1;
  width: 100%;
  max-width: 20rem;
  align-items: center;
  padding: 1rem 0;
  cursor: grab;
  touch-action: none;
  user-select: none;
}

.es-slider:active {
  cursor: grabbing;
}

.es-track-wrapper {
  display: flex;
  flex-grow: 1;
}

.es-track {
  position: relative;
  flex-grow: 1;
  height: 100%;
  overflow: hidden;
  border-radius: 9999px;
  background: var(--fluen-muted);
}

.es-fill {
  position: absolute;
  top: 0;
  left: 0;
  height: 100%;
  border-radius: 9999px;
  background: v-bind(accentColor);
  transition: width 60ms linear;
}

/* ── Value readout ─────────────────────────────────────────────── */

.es-value {
  position: absolute;
  bottom: -0.25rem;
  margin: 0;
  color: var(--fluen-stone);
  font-weight: 500;
  font-size: 0.875rem;
  letter-spacing: 0.025em;
  pointer-events: none;
}
</style>
