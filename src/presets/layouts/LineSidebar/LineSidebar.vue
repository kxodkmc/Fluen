<script lang="ts">
import type { CSSProperties } from 'vue';

/** 邻近衰减曲线类型。 */
export type LineSidebarFalloff = 'linear' | 'smooth' | 'sharp';

export interface LineSidebarProps {
  /** 渲染为侧边栏条目的标签列表。 */
  items?: string[];
  /** 鼠标靠近时，条目与标记线偏向的强调色。 */
  accentColor?: string;
  /** 条目标签的静止文字色。 */
  textColor?: string;
  /** 前导标记线的静止颜色。 */
  markerColor?: string;
  /** 是否在标签前显示零填充序号。 */
  showIndex?: boolean;
  /** 是否显示标记线与短刻度。 */
  showMarker?: boolean;
  /** 鼠标影响条目的垂直距离（像素）。 */
  proximityRadius?: number;
  /** 完全邻近时标签水平位移的最大值（像素）。 */
  maxShift?: number;
  /** 距离映射到邻近效果的曲线类型。 */
  falloff?: LineSidebarFalloff;
  /** 标记线长度（像素），中间刻度按比例缩放。 */
  markerLength?: number;
  /** 标签与标记线之间的间距（像素）。 */
  markerGap?: number;
  /** 中间刻度长度占 markerLength 的比例。 */
  tickScale?: number;
  /** 为 true 时，中间刻度也随鼠标邻近而放大。 */
  scaleTick?: boolean;
  /** 条目之间的垂直间距（像素）。 */
  itemGap?: number;
  /** 标签字号（rem）。 */
  fontSize?: number;
  /** 邻近响应的过渡时长（毫秒）。 */
  smoothing?: number;
  /** 挂载时默认选中的条目索引。 */
  defaultActive?: number | null;
  /** 外层包装的附加 CSS 类名。 */
  className?: string;
}

export type LineSidebarEmits = {
  /** 点击条目时触发，点击的条目同时变为激活态。 */
  (e: 'itemClick', index: number, label: string): void;
};
</script>

<script setup lang="ts">
import { computed, ref, watch, onUnmounted, type ComponentPublicInstance } from 'vue';

/* ------------------------------------------------------------------ *
 * 衰减曲线
 * ------------------------------------------------------------------ */
const FALLOFF_CURVES: Record<LineSidebarFalloff, (p: number) => number> = {
  linear: p => p,
  smooth: p => p * p * (3 - 2 * p),
  sharp: p => p * p * p
};

/* ------------------------------------------------------------------ *
 * Props & Emits
 * ------------------------------------------------------------------ */
const props = withDefaults(defineProps<LineSidebarProps>(), {
  items: () => [
    'Overview',
    'Components',
    'Animations',
    'Backgrounds',
    'Showcase',
    'Playground',
    'Templates',
    'Changelog',
    'Community',
    'Resources',
    'Documentation',
    'Support'
  ],
  accentColor: 'var(--fluen-brand-purple)',
  textColor: 'var(--fluen-steel)',
  markerColor: 'var(--fluen-stone)',
  showIndex: true,
  showMarker: true,
  proximityRadius: 100,
  maxShift: 30,
  falloff: 'smooth',
  markerLength: 60,
  markerGap: 0,
  tickScale: 0.5,
  scaleTick: true,
  itemGap: 20,
  fontSize: 1.1,
  smoothing: 100,
  defaultActive: null,
  className: ''
});

const emit = defineEmits<LineSidebarEmits>();

/* ------------------------------------------------------------------ *
 * 状态与引用
 * ------------------------------------------------------------------ */
const listRef = ref<HTMLUListElement | null>(null);
const itemRefs = ref<(HTMLLIElement | null)[]>([]);
let targets: number[] = [];
const current: number[] = [];
let rafId: number | null = null;
let last = 0;

const activeIndex = ref<number | null>(props.defaultActive);

const setItemRef = (el: Element | ComponentPublicInstance | null, index: number) => {
  itemRefs.value[index] = el as HTMLLIElement | null;
};

/* ------------------------------------------------------------------ *
 * 动画循环 — 指数衰减插值，实现平滑的邻近响应
 * ------------------------------------------------------------------ */
const runFrame = (now: number) => {
  const dt = Math.min((now - last) / 1000, 0.05);
  last = now;
  const tau = Math.max(props.smoothing, 1) / 1000;
  const k = 1 - Math.exp(-dt / tau);

  let moving = false;
  const els = itemRefs.value;
  for (let i = 0; i < els.length; i++) {
    const el = els[i];
    if (!el) continue;
    const target = Math.max(targets[i] || 0, activeIndex.value === i ? 1 : 0);
    const cur = current[i] || 0;
    const next = cur + (target - cur) * k;
    const settled = Math.abs(target - next) < 0.0015;
    const value = settled ? target : next;
    current[i] = value;
    el.style.setProperty('--effect', value.toFixed(4));
    if (!settled) moving = true;
  }

  rafId = moving ? requestAnimationFrame(runFrame) : null;
};

const startLoop = () => {
  if (rafId != null) return;
  last = performance.now();
  rafId = requestAnimationFrame(runFrame);
};

/* ------------------------------------------------------------------ *
 * 指针事件
 * ------------------------------------------------------------------ */
const handlePointerMove = (e: PointerEvent) => {
  const list = listRef.value;
  if (!list) return;
  const rect = list.getBoundingClientRect();
  const pointerY = e.clientY - rect.top;
  const ease = FALLOFF_CURVES[props.falloff] ?? FALLOFF_CURVES.linear;
  const els = itemRefs.value;
  for (let i = 0; i < els.length; i++) {
    const el = els[i];
    if (!el) continue;
    const center = el.offsetTop + el.offsetHeight / 2;
    const distance = Math.abs(pointerY - center);
    targets[i] = ease(Math.max(0, 1 - distance / props.proximityRadius));
  }
  startLoop();
};

const handlePointerLeave = () => {
  targets = targets.map(() => 0);
  startLoop();
};

const handleClick = (index: number, label: string) => {
  activeIndex.value = index;
  emit('itemClick', index, label);
};

/* ------------------------------------------------------------------ *
 * 根元素 CSS 变量 — 供 scoped CSS 通过 var() 引用
 * ------------------------------------------------------------------ */
const rootStyle = computed<CSSProperties>(() => ({
  ['--ls-accent-color' as string]: props.accentColor,
  ['--ls-text-color' as string]: props.textColor,
  ['--ls-marker-color' as string]: props.markerColor,
  ['--ls-marker-length' as string]: `${props.markerLength}px`,
  ['--ls-marker-gap' as string]: `${props.markerGap}px`,
  ['--ls-tick-scale' as string]: props.tickScale,
  ['--ls-max-shift' as string]: `${props.maxShift}px`,
  ['--ls-item-gap' as string]: `${props.itemGap}px`,
  ['--ls-font-size' as string]: `${props.fontSize}rem`,
  ['--ls-smoothing' as string]: `${props.smoothing}ms`
}));

watch(activeIndex, () => startLoop(), { immediate: true });

onUnmounted(() => {
  if (rafId != null) cancelAnimationFrame(rafId);
});
</script>

<template>
  <nav
    class="ls-nav"
    :class="[className, { 'ls-nav--with-marker': showMarker }]"
    :style="rootStyle"
  >
    <ul
      ref="listRef"
      class="ls-list"
      @pointermove="handlePointerMove"
      @pointerleave="handlePointerLeave"
    >
      <li
        v-for="(label, index) in items"
        :key="`${label}-${index}`"
        :ref="(el) => setItemRef(el, index)"
        class="ls-item"
        :class="{
          'ls-item--with-tick': showMarker,
          'ls-item--scale-tick': showMarker && scaleTick
        }"
        :aria-current="activeIndex === index ? 'true' : undefined"
        @click="handleClick(index, label)"
      >
        <!-- 标记线 -->
        <span v-if="showMarker" aria-hidden="true" class="ls-marker" />

        <!-- 标签 -->
        <span class="ls-label">
          <span v-if="showIndex" class="ls-index">
            {{ String(index + 1).padStart(2, '0') }}
          </span>
          <span class="ls-text">{{ label }}</span>
        </span>
      </li>
    </ul>
  </nav>
</template>

<style scoped>
/* Self-contained styles (no Tailwind required) — keeps the preset portable. */

/* ── 根容器 ─────────────────────────────────────────────────────── */
.ls-nav {
  position: relative;
  display: flex;
  justify-content: flex-start;
}

.ls-nav--with-marker {
  padding-left: calc(var(--ls-marker-length) + var(--ls-marker-gap));
}

/* ── 列表 ───────────────────────────────────────────────────────── */
.ls-list {
  display: flex;
  flex-direction: column;
  margin: 0;
  padding: 1rem 0;
  list-style: none;
  gap: var(--ls-item-gap);
}

/* ── 条目 ───────────────────────────────────────────────────────── */
.ls-item {
  position: relative;
  cursor: pointer;
}

/* 扩展点击区域 */
.ls-item::before {
  content: '';
  position: absolute;
  left: -3rem;
  right: -3rem;
  top: -0.375rem;
  bottom: -0.375rem;
}

/* 中间刻度（条目之间的短横线） */
.ls-item--with-tick::after {
  content: '';
  position: absolute;
  left: calc(-1 * var(--ls-marker-length) - var(--ls-marker-gap));
  top: calc(100% + var(--ls-item-gap) / 2);
  height: 1px;
  width: calc(var(--ls-marker-length) * var(--ls-tick-scale));
  background-color: var(--ls-marker-color);
  opacity: 0.5;
  transform: translateY(-50%);
  transform-origin: left;
}

/* 刻度随邻近放大 */
.ls-item--with-tick.ls-item--scale-tick::after {
  transform: translateY(-50%) scaleX(calc(0.7 + var(--effect, 0) * 0.6));
}

/* 最后一个条目不显示刻度 */
.ls-list > .ls-item:last-child::after {
  content: none;
}

/* ── 标记线 ─────────────────────────────────────────────────────── */
.ls-marker {
  position: absolute;
  top: 50%;
  left: calc(-1 * var(--ls-marker-length) - var(--ls-marker-gap));
  width: var(--ls-marker-length);
  height: 1px;
  transform: translateY(-50%) scaleX(calc(0.7 + var(--effect, 0) * 0.5));
  transform-origin: left;
  background-color: color-mix(
    in srgb,
    var(--ls-accent-color) calc(var(--effect, 0) * 100%),
    var(--ls-marker-color)
  );
}

/* ── 标签 ───────────────────────────────────────────────────────── */
.ls-label {
  display: inline-flex;
  position: relative;
  align-items: baseline;
  font-size: var(--ls-font-size);
  line-height: 1.2;
  transform: translateX(calc(var(--effect, 0) * var(--ls-max-shift)));
  color: color-mix(
    in srgb,
    var(--ls-accent-color) calc(var(--effect, 0) * 100%),
    var(--ls-text-color)
  );
}

/* ── 序号 ───────────────────────────────────────────────────────── */
.ls-index {
  opacity: calc(0.55 + var(--effect, 0) * 0.45);
  margin-right: 0.6rem;
  font-family: var(--fluen-font-mono);
  font-size: 0.85em;
}
</style>
