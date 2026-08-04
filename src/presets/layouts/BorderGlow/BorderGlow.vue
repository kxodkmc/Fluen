<script lang="ts">
export interface BorderGlowProps {
  /** Additional CSS classes for the outer wrapper. */
  className?: string;
  /** How close the pointer must be to the edge for the glow to appear (0-100). */
  edgeSensitivity?: number;
  /** HSL values for the glow color, as "H S L" (e.g. "40 80 80"). */
  glowColor?: string;
  /** Background color of the card. */
  backgroundColor?: string;
  /** Corner radius of the card in pixels. */
  borderRadius?: number;
  /** How far the outer glow extends beyond the card in pixels. */
  glowRadius?: number;
  /** Multiplier for glow opacity (0.1-3.0). */
  glowIntensity?: number;
  /** Width of the directional cone mask as a percentage (5-45). */
  coneSpread?: number;
  /** Play an intro sweep animation on mount. */
  animated?: boolean;
  /** Array of 3 hex colors for the mesh gradient border. */
  colors?: string[];
  /** Opacity multiplier for the inner mesh fill (0-1). */
  fillOpacity?: number;
}
</script>

<script setup lang="ts">
import { computed, ref, useTemplateRef, watch } from 'vue';

function parseHSL(hslStr: string): { h: number; s: number; l: number } {
  const match = hslStr.match(/([\d.]+)\s*([\d.]+)%?\s*([\d.]+)%?/);
  if (!match) return { h: 40, s: 80, l: 80 };
  return { h: parseFloat(match[1]), s: parseFloat(match[2]), l: parseFloat(match[3]) };
}

function buildBoxShadow(glowColor: string, intensity: number): string {
  const { h, s, l } = parseHSL(glowColor);
  const base = `${h}deg ${s}% ${l}%`;
  const layers: [number, number, number, number, number, boolean][] = [
    [0, 0, 0, 1, 100, true],
    [0, 0, 1, 0, 60, true],
    [0, 0, 3, 0, 50, true],
    [0, 0, 6, 0, 40, true],
    [0, 0, 15, 0, 30, true],
    [0, 0, 25, 2, 20, true],
    [0, 0, 50, 2, 10, true],
    [0, 0, 1, 0, 60, false],
    [0, 0, 3, 0, 50, false],
    [0, 0, 6, 0, 40, false],
    [0, 0, 15, 0, 30, false],
    [0, 0, 25, 2, 20, false],
    [0, 0, 50, 2, 10, false]
  ];
  return layers
    .map(([x, y, blur, spread, alpha, inset]) => {
      const a = Math.min(alpha * intensity, 100);
      return `${inset ? 'inset ' : ''}${x}px ${y}px ${blur}px ${spread}px hsl(${base} / ${a}%)`;
    })
    .join(', ');
}

function easeOutCubic(x: number) {
  return 1 - Math.pow(1 - x, 3);
}
function easeInCubic(x: number) {
  return x * x * x;
}

interface AnimateOpts {
  start?: number;
  end?: number;
  duration?: number;
  delay?: number;
  ease?: (t: number) => number;
  onUpdate: (v: number) => void;
  onEnd?: () => void;
}

function animateValue({
  start = 0,
  end = 100,
  duration = 1000,
  delay = 0,
  ease = easeOutCubic,
  onUpdate,
  onEnd
}: AnimateOpts) {
  const t0 = performance.now() + delay;
  function tick() {
    const elapsed = performance.now() - t0;
    const t = Math.min(elapsed / duration, 1);
    onUpdate(start + (end - start) * ease(t));
    if (t < 1) requestAnimationFrame(tick);
    else if (onEnd) onEnd();
  }
  setTimeout(() => requestAnimationFrame(tick), delay);
}

const GRADIENT_POSITIONS = ['80% 55%', '69% 34%', '8% 6%', '41% 38%', '86% 85%', '82% 18%', '51% 4%'];
const COLOR_MAP = [0, 1, 2, 0, 1, 2, 1];

function buildMeshGradients(colors: string[]): string[] {
  const gradients: string[] = [];
  for (let i = 0; i < 7; i++) {
    const c = colors[Math.min(COLOR_MAP[i], colors.length - 1)];
    gradients.push(`radial-gradient(at ${GRADIENT_POSITIONS[i]}, ${c} 0px, transparent 50%)`);
  }
  gradients.push(`linear-gradient(${colors[0]} 0 100%)`);
  return gradients;
}

const props = withDefaults(defineProps<BorderGlowProps>(), {
  className: '',
  edgeSensitivity: 30,
  glowColor: '40 80 80',
  backgroundColor: 'var(--fluen-surface)',
  borderRadius: 28,
  glowRadius: 40,
  glowIntensity: 1.0,
  coneSpread: 25,
  animated: false,
  colors: () => ['#a855f7', '#ea5ec1', '#1456f0'],
  fillOpacity: 0.5
});

const cardRef = useTemplateRef<HTMLDivElement>('cardRef');
const isHovered = ref(false);
const cursorAngle = ref(45);
const edgeProximity = ref(0);
const sweepActive = ref(false);

const getCenterOfElement = (el: HTMLElement) => {
  const { width, height } = el.getBoundingClientRect();
  return [width / 2, height / 2];
};

const getEdgeProximity = (el: HTMLElement, x: number, y: number) => {
  const [cx, cy] = getCenterOfElement(el);
  const dx = x - cx;
  const dy = y - cy;
  let kx = Infinity;
  let ky = Infinity;
  if (dx !== 0) kx = cx / Math.abs(dx);
  if (dy !== 0) ky = cy / Math.abs(dy);
  return Math.min(Math.max(1 / Math.min(kx, ky), 0), 1);
};

const getCursorAngle = (el: HTMLElement, x: number, y: number) => {
  const [cx, cy] = getCenterOfElement(el);
  const dx = x - cx;
  const dy = y - cy;
  if (dx === 0 && dy === 0) return 0;
  const radians = Math.atan2(dy, dx);
  let degrees = radians * (180 / Math.PI) + 90;
  if (degrees < 0) degrees += 360;
  return degrees;
};

const handlePointerMove = (e: PointerEvent) => {
  const card = cardRef.value;
  if (!card) return;
  const rect = card.getBoundingClientRect();
  const x = e.clientX - rect.left;
  const y = e.clientY - rect.top;
  edgeProximity.value = getEdgeProximity(card, x, y);
  cursorAngle.value = getCursorAngle(card, x, y);
};

watch(
  () => [props.animated],
  () => {
    if (!props.animated) return;
    const angleStart = 110;
    const angleEnd = 465;
    sweepActive.value = true;
    cursorAngle.value = angleStart;

    animateValue({ duration: 500, onUpdate: v => (edgeProximity.value = v / 100) });
    animateValue({
      ease: easeInCubic,
      duration: 1500,
      end: 50,
      onUpdate: v => {
        cursorAngle.value = (angleEnd - angleStart) * (v / 100) + angleStart;
      }
    });
    animateValue({
      ease: easeOutCubic,
      delay: 1500,
      duration: 2250,
      start: 50,
      end: 100,
      onUpdate: v => {
        cursorAngle.value = (angleEnd - angleStart) * (v / 100) + angleStart;
      }
    });
    animateValue({
      ease: easeInCubic,
      delay: 2500,
      duration: 1500,
      start: 100,
      end: 0,
      onUpdate: v => (edgeProximity.value = v / 100),
      onEnd: () => (sweepActive.value = false)
    });
  },
  {
    deep: true,
    immediate: true
  }
);

const colorSensitivity = computed(() => props.edgeSensitivity + 20);
const isVisible = computed(() => isHovered.value || sweepActive.value);
const borderOpacity = computed(() =>
  isVisible.value
    ? Math.max(0, (edgeProximity.value * 100 - colorSensitivity.value) / (100 - colorSensitivity.value))
    : 0
);
const glowOpacity = computed(() =>
  isVisible.value ? Math.max(0, (edgeProximity.value * 100 - props.edgeSensitivity) / (100 - props.edgeSensitivity)) : 0
);

const meshGradients = computed(() => buildMeshGradients(props.colors));
const borderBg = computed(() => meshGradients.value.map(g => `${g} border-box`));
const fillBg = computed(() => meshGradients.value.map(g => `${g} padding-box`));
const angleDeg = computed(() => `${cursorAngle.value.toFixed(3)}deg`);

const baseShadow =
  'var(--fluen-shadow-card)';
</script>

<template>
  <div
    ref="cardRef"
    class="bg-card"
    :class="props.className"
    @pointermove="handlePointerMove"
    @pointerenter="isHovered = true"
    @pointerleave="isHovered = false"
    :style="{
      background: props.backgroundColor,
      borderRadius: props.borderRadius + 'px',
      transform: 'translate3d(0, 0, 0.01px)',
      boxShadow: baseShadow
    }"
  >
    <!-- mesh gradient border -->
    <div
      class="bg-layer"
      :style="{
        border: '1px solid transparent',
        background: [
          `linear-gradient(${props.backgroundColor} 0 100%) padding-box`,
          'linear-gradient(rgb(255 255 255 / 0%) 0% 100%) border-box',
          ...borderBg
        ].join(', '),
        opacity: borderOpacity,
        maskImage: `conic-gradient(from ${angleDeg} at center, black ${props.coneSpread}%, transparent ${
          props.coneSpread + 15
        }%, transparent ${100 - props.coneSpread - 15}%, black ${100 - props.coneSpread}%)`,
        WebkitMaskImage: `conic-gradient(from ${angleDeg} at center, black ${props.coneSpread}%, transparent ${
          props.coneSpread + 15
        }%, transparent ${100 - props.coneSpread - 15}%, black ${100 - props.coneSpread}%)`,
        transition: isVisible ? 'opacity 0.25s ease-out' : 'opacity 0.75s ease-in-out'
      }"
    />

    <!-- mesh gradient fill -->
    <div
      class="bg-layer"
      :style="{
        border: '1px solid transparent',
        background: fillBg.join(', '),
        maskImage: [
          'linear-gradient(to bottom, black, black)',
          'radial-gradient(ellipse at 50% 50%, black 40%, transparent 65%)',
          'radial-gradient(ellipse at 66% 66%, black 5%, transparent 40%)',
          'radial-gradient(ellipse at 33% 33%, black 5%, transparent 40%)',
          'radial-gradient(ellipse at 66% 33%, black 5%, transparent 40%)',
          'radial-gradient(ellipse at 33% 66%, black 5%, transparent 40%)',
          `conic-gradient(from ${angleDeg} at center, transparent 5%, black 15%, black 85%, transparent 95%)`
        ].join(', '),
        WebkitMaskImage: [
          'linear-gradient(to bottom, black, black)',
          'radial-gradient(ellipse at 50% 50%, black 40%, transparent 65%)',
          'radial-gradient(ellipse at 66% 66%, black 5%, transparent 40%)',
          'radial-gradient(ellipse at 33% 33%, black 5%, transparent 40%)',
          'radial-gradient(ellipse at 66% 33%, black 5%, transparent 40%)',
          'radial-gradient(ellipse at 33% 66%, black 5%, transparent 40%)',
          `conic-gradient(from ${angleDeg} at center, transparent 5%, black 15%, black 85%, transparent 95%)`
        ].join(', '),
        maskComposite: 'subtract, add, add, add, add, add',
        WebkitMaskComposite: 'source-out, source-over, source-over, source-over, source-over, source-over',
        opacity: borderOpacity * props.fillOpacity,
        mixBlendMode: 'soft-light',
        transition: isVisible ? 'opacity 0.25s ease-out' : 'opacity 0.75s ease-in-out'
      }"
    />

    <!-- outer glow -->
    <span
      class="bg-glow-outer"
      :style="{
        inset: `-${props.glowRadius}px`,
        maskImage: `conic-gradient(from ${angleDeg} at center, black 2.5%, transparent 10%, transparent 90%, black 97.5%)`,
        WebkitMaskImage: `conic-gradient(from ${angleDeg} at center, black 2.5%, transparent 10%, transparent 90%, black 97.5%)`,
        opacity: glowOpacity,
        mixBlendMode: 'plus-lighter',
        transition: isVisible ? 'opacity 0.25s ease-out' : 'opacity 0.75s ease-in-out'
      }"
    >
      <span
        class="bg-glow-inner"
        :style="{
          inset: `${props.glowRadius}px`,
          boxShadow: buildBoxShadow(props.glowColor, props.glowIntensity)
        }"
      />
    </span>

    <!-- content -->
    <div class="bg-content">
      <slot />
    </div>
  </div>
</template>

<style scoped>
/* Self-contained styles (no Tailwind required) — keeps the preset portable. */
.bg-card {
  position: relative;
  display: grid;
  isolation: isolate;
  border: 1px solid var(--fluen-hairline);
}

.bg-layer {
  position: absolute;
  z-index: -1;
  inset: 0;
  border-radius: inherit;
}

.bg-glow-outer {
  position: absolute;
  z-index: 1;
  border-radius: inherit;
  pointer-events: none;
}

.bg-glow-inner {
  position: absolute;
  border-radius: inherit;
}

.bg-content {
  position: relative;
  z-index: 1;
  display: flex;
  flex-direction: column;
  overflow: auto;
}
</style>
