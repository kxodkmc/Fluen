<script lang="ts">
export interface CountUpProps {
  /** The target number to count up to. */
  to: number;
  /** The initial number from which the count starts. */
  from?: number;
  /** Direction of the count; "down" reverses from/to. */
  direction?: 'up' | 'down';
  /** Delay in seconds before the counting starts. */
  delay?: number;
  /** Duration of the count animation — based on the spring damping/stiffness. */
  duration?: number;
  /** CSS class to apply to the component for additional styling. */
  className?: string;
  /** If true, the count starts when the component scrolls into view. */
  startWhen?: boolean;
  /** Character to use as a thousands separator in the displayed number. */
  separator?: string;
  /** Called when the count animation starts. */
  onStart?: () => void;
  /** Called when the count animation ends. */
  onEnd?: () => void;
}
</script>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, computed, useTemplateRef } from 'vue';

const props = withDefaults(defineProps<CountUpProps>(), {
  from: 0,
  direction: 'up',
  delay: 0,
  duration: 2,
  className: '',
  startWhen: true,
  separator: '',
  onStart: undefined,
  onEnd: undefined
});

const elementRef = useTemplateRef<HTMLSpanElement>('elementRef');
const currentValue = ref(props.direction === 'down' ? props.to : props.from);
const isInView = ref(false);
const animationId = ref<number | null>(null);
const hasStarted = ref(false);

let intersectionObserver: IntersectionObserver | null = null;

// Spring physics: duration scales the inverse damping/stiffness.
// Lower duration -> stiffer spring -> faster settle.
const damping = computed(() => 20 + 40 * (1 / props.duration));
const stiffness = computed(() => 100 * (1 / props.duration));

let velocity = 0;
let startTime = 0;

const formatNumber = (value: number) => {
  const options = {
    useGrouping: !!props.separator,
    minimumFractionDigits: 0,
    maximumFractionDigits: 0
  };

  const formattedNumber = Intl.NumberFormat('en-US', options).format(Number(value.toFixed(0)));

  return props.separator ? formattedNumber.replace(/,/g, props.separator) : formattedNumber;
};

const updateDisplay = () => {
  if (elementRef.value) {
    elementRef.value.textContent = formatNumber(currentValue.value);
  }
};

const springAnimation = (timestamp: number) => {
  if (!startTime) startTime = timestamp;

  const target = props.direction === 'down' ? props.from : props.to;
  const current = currentValue.value;

  const displacement = target - current;
  const springForce = displacement * stiffness.value;
  const dampingForce = velocity * damping.value;
  const acceleration = springForce - dampingForce;

  velocity += acceleration * 0.016; // Assuming 60fps
  currentValue.value += velocity * 0.016;

  updateDisplay();

  if (Math.abs(displacement) > 0.01 || Math.abs(velocity) > 0.01) {
    animationId.value = requestAnimationFrame(springAnimation);
  } else {
    currentValue.value = target;
    updateDisplay();
    animationId.value = null;

    props.onEnd?.();
  }
};

const startAnimation = () => {
  if (hasStarted.value || !isInView.value || !props.startWhen) return;

  hasStarted.value = true;

  props.onStart?.();

  setTimeout(() => {
    startTime = 0;
    velocity = 0;
    animationId.value = requestAnimationFrame(springAnimation);
  }, props.delay * 1000);
};

const setupIntersectionObserver = () => {
  if (!elementRef.value) return;

  intersectionObserver = new IntersectionObserver(
    ([entry]) => {
      if (entry.isIntersecting && !isInView.value) {
        isInView.value = true;
        startAnimation();
      }
    },
    {
      threshold: 0,
      rootMargin: '0px'
    }
  );

  intersectionObserver.observe(elementRef.value);
};

const cleanup = () => {
  if (animationId.value) {
    cancelAnimationFrame(animationId.value);
    animationId.value = null;
  }

  if (intersectionObserver) {
    intersectionObserver.disconnect();
    intersectionObserver = null;
  }
};

watch(
  [() => props.from, () => props.to, () => props.direction],
  () => {
    currentValue.value = props.direction === 'down' ? props.to : props.from;
    updateDisplay();
    hasStarted.value = false;
  },
  { immediate: true }
);

watch(
  () => props.startWhen,
  () => {
    if (props.startWhen && isInView.value && !hasStarted.value) {
      startAnimation();
    }
  }
);

onMounted(() => {
  updateDisplay();
  setupIntersectionObserver();
});

onUnmounted(() => {
  cleanup();
});
</script>

<template>
  <span ref="elementRef" :class="className" class="cu-root" />
</template>

<style scoped>
/* Inline-block so width tracks the numeric content, not the line box. */
.cu-root {
  display: inline-block;
  font-variant-numeric: tabular-nums;
}
</style>
