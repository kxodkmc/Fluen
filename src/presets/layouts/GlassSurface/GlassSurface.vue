<script lang="ts">
import type { CSSProperties } from 'vue';

export type GlassBlendMode =
  | 'normal'
  | 'multiply'
  | 'screen'
  | 'overlay'
  | 'darken'
  | 'lighten'
  | 'color-dodge'
  | 'color-burn'
  | 'hard-light'
  | 'soft-light'
  | 'difference'
  | 'exclusion'
  | 'hue'
  | 'saturation'
  | 'color'
  | 'luminosity'
  | 'plus-darker'
  | 'plus-lighter';

export interface GlassSurfaceProps {
  /** Width of the glass surface (pixels or CSS value like '100%'). */
  width?: string | number;
  /** Height of the glass surface (pixels or CSS value like '100vh'). */
  height?: string | number;
  /** Border radius in pixels. */
  borderRadius?: number;
  /** Border width factor for displacement map. */
  borderWidth?: number;
  /** Brightness percentage for displacement map. */
  brightness?: number;
  /** Opacity of displacement map elements. */
  opacity?: number;
  /** Input blur amount in pixels. */
  blur?: number;
  /** Output blur (stdDeviation). */
  displace?: number;
  /** Background frost opacity (0-1). */
  backgroundOpacity?: number;
  /** Backdrop filter saturation factor. */
  saturation?: number;
  /** Main displacement scale. */
  distortionScale?: number;
  /** Red channel extra displacement offset. */
  redOffset?: number;
  /** Green channel extra displacement offset. */
  greenOffset?: number;
  /** Blue channel extra displacement offset. */
  blueOffset?: number;
  /** X displacement channel selector. */
  xChannel?: 'R' | 'G' | 'B';
  /** Y displacement channel selector. */
  yChannel?: 'R' | 'G' | 'B';
  /** Mix blend mode for displacement map. */
  mixBlendMode?: GlassBlendMode;
  /** Additional CSS class names. */
  className?: string;
  /** Inline styles object. */
  style?: CSSProperties;
}
</script>

<script setup lang="ts">
import { computed, useTemplateRef, onMounted, onUnmounted, watch, nextTick } from 'vue';
import { useTheme } from '../../../theme';

const props = withDefaults(defineProps<GlassSurfaceProps>(), {
  width: '200px',
  height: '200px',
  borderRadius: 20,
  borderWidth: 0.07,
  brightness: 70,
  opacity: 0.93,
  blur: 11,
  displace: 0.5,
  backgroundOpacity: 0,
  saturation: 1,
  distortionScale: -180,
  redOffset: 0,
  greenOffset: 10,
  blueOffset: 20,
  xChannel: 'R',
  yChannel: 'G',
  mixBlendMode: 'difference',
  className: '',
  style: () => ({}) as CSSProperties
});

const { currentMode } = useTheme();
const isDarkMode = computed(() => currentMode.value === 'dark');

// Generate unique IDs for SVG elements
const generateUniqueId = () => Math.random().toString(36).substring(2, 15);

const uniqueId = generateUniqueId();
const filterId = `glass-filter-${uniqueId}`;
const redGradId = `red-grad-${uniqueId}`;
const blueGradId = `blue-grad-${uniqueId}`;

const containerRef = useTemplateRef<HTMLDivElement>('containerRef');
const feImageRef = useTemplateRef<SVGFEMergeElement>('feImageRef');
const redChannelRef = useTemplateRef<SVGFEDisplacementMapElement>('redChannelRef');
const greenChannelRef = useTemplateRef<SVGFEDisplacementMapElement>('greenChannelRef');
const blueChannelRef = useTemplateRef<SVGFEDisplacementMapElement>('blueChannelRef');
const gaussianBlurRef = useTemplateRef<SVGFEGaussianBlurElement>('gaussianBlurRef');

let resizeObserver: ResizeObserver | null = null;

const generateDisplacementMap = () => {
  const rect = containerRef.value?.getBoundingClientRect();
  const actualWidth = rect?.width || 400;
  const actualHeight = rect?.height || 200;
  const edgeSize = Math.min(actualWidth, actualHeight) * (props.borderWidth * 0.5);

  // Note: xmlns attribute is required on the embedded SVG so feImage can load it as an image.
  const svgContent = `
      <svg viewBox="0 0 ${actualWidth} ${actualHeight}" xmlns="http://www.w3.org/2000/svg">
        <defs>
          <linearGradient id="${redGradId}" x1="100%" y1="0%" x2="0%" y2="0%">
            <stop offset="0%" stop-color="#0000"/>
            <stop offset="100%" stop-color="red"/>
          </linearGradient>
          <linearGradient id="${blueGradId}" x1="0%" y1="0%" x2="0%" y2="100%">
            <stop offset="0%" stop-color="#0000"/>
            <stop offset="100%" stop-color="blue"/>
          </linearGradient>
        </defs>
        <rect x="0" y="0" width="${actualWidth}" height="${actualHeight}" fill="black"></rect>
        <rect x="0" y="0" width="${actualWidth}" height="${actualHeight}" rx="${props.borderRadius}" fill="url(#${redGradId})" />
        <rect x="0" y="0" width="${actualWidth}" height="${actualHeight}" rx="${props.borderRadius}" fill="url(#${blueGradId})" style="mix-blend-mode: ${props.mixBlendMode}" />
        <rect x="${edgeSize}" y="${edgeSize}" width="${actualWidth - edgeSize * 2}" height="${actualHeight - edgeSize * 2}" rx="${props.borderRadius}" fill="hsl(0 0% ${props.brightness}% / ${props.opacity})" style="filter:blur(${props.blur}px)" />
      </svg>
    `;

  return `data:image/svg+xml,${encodeURIComponent(svgContent)}`;
};

const updateDisplacementMap = () => {
  if (feImageRef.value) {
    feImageRef.value.setAttribute('href', generateDisplacementMap());
  }
};

const supportsSVGFilters = () => {
  if (typeof window === 'undefined' || typeof navigator === 'undefined') return false;
  const isWebkit = /Safari/.test(navigator.userAgent) && !/Chrome/.test(navigator.userAgent);
  const isFirefox = /Firefox/.test(navigator.userAgent);
  if (isWebkit || isFirefox) return false;
  const div = document.createElement('div');
  div.style.backdropFilter = `url(#${filterId})`;
  return div.style.backdropFilter !== '';
};

const supportsBackdropFilter = () => {
  if (typeof window === 'undefined') return false;
  return CSS.supports('backdrop-filter', 'blur(10px)');
};

const containerStyles = computed<CSSProperties>(() => {
  const baseStyles: CSSProperties = {
    ...props.style,
    width: typeof props.width === 'number' ? `${props.width}px` : props.width,
    height: typeof props.height === 'number' ? `${props.height}px` : props.height,
    borderRadius: `${props.borderRadius}px`,
    // Custom properties for potential downstream theming
    ['--glass-frost' as string]: props.backgroundOpacity,
    ['--glass-saturation' as string]: props.saturation
  };

  const svgSupported = supportsSVGFilters();
  const backdropFilterSupported = supportsBackdropFilter();

  if (svgSupported) {
    return {
      ...baseStyles,
      background: `color-mix(in srgb, var(--fluen-surface) ${Math.round(props.backgroundOpacity * 100)}%, transparent)`,
      backdropFilter: `url(#${filterId}) saturate(${props.saturation})`,
      boxShadow: isDarkMode.value
        ? `0 0 2px 1px color-mix(in oklch, var(--fluen-on-dark), transparent 65%) inset,
           0 0 10px 4px color-mix(in oklch, var(--fluen-on-dark), transparent 85%) inset,
           var(--fluen-shadow-card),
           var(--fluen-shadow-card) inset`
        : `0 0 2px 1px color-mix(in oklch, var(--fluen-ink), transparent 85%) inset,
           0 0 10px 4px color-mix(in oklch, var(--fluen-ink), transparent 90%) inset,
           var(--fluen-shadow-card),
           var(--fluen-shadow-card) inset`
    };
  }

  // Fallback paths
  if (isDarkMode.value) {
    if (!backdropFilterSupported) {
      return {
        ...baseStyles,
        background: `color-mix(in srgb, var(--fluen-surface) 40%, transparent)`,
        border: `1px solid var(--fluen-hairline)`,
        boxShadow: `inset 0 1px 0 0 var(--fluen-hairline),
                    inset 0 -1px 0 0 var(--fluen-hairline-soft)`
      };
    }
    return {
      ...baseStyles,
      background: `color-mix(in srgb, var(--fluen-surface) 10%, transparent)`,
      backdropFilter: 'blur(12px) saturate(1.8) brightness(1.2)',
      WebkitBackdropFilter: 'blur(12px) saturate(1.8) brightness(1.2)',
      border: `1px solid var(--fluen-hairline)`,
      boxShadow: `inset 0 1px 0 0 var(--fluen-hairline),
                  inset 0 -1px 0 0 var(--fluen-hairline-soft)`
    };
  }

  if (!backdropFilterSupported) {
    return {
      ...baseStyles,
      background: `color-mix(in srgb, var(--fluen-surface) 40%, transparent)`,
      border: `1px solid var(--fluen-hairline)`,
      boxShadow: `inset 0 1px 0 0 var(--fluen-hairline),
                  inset 0 -1px 0 0 var(--fluen-hairline-soft)`
    };
  }
  return {
    ...baseStyles,
    background: `color-mix(in srgb, var(--fluen-surface) 25%, transparent)`,
    backdropFilter: 'blur(12px) saturate(1.8) brightness(1.1)',
    WebkitBackdropFilter: 'blur(12px) saturate(1.8) brightness(1.1)',
    border: `1px solid var(--fluen-hairline)`,
    boxShadow: `var(--fluen-shadow-card),
                var(--fluen-shadow-card),
                inset 0 1px 0 0 var(--fluen-hairline),
                inset 0 -1px 0 0 var(--fluen-hairline-soft)`
  };
});

const updateFilterElements = () => {
  const elements = [
    { ref: redChannelRef, offset: props.redOffset },
    { ref: greenChannelRef, offset: props.greenOffset },
    { ref: blueChannelRef, offset: props.blueOffset }
  ];

  elements.forEach(({ ref, offset }) => {
    if (ref.value) {
      ref.value.setAttribute('scale', (props.distortionScale + offset).toString());
      ref.value.setAttribute('xChannelSelector', props.xChannel);
      ref.value.setAttribute('yChannelSelector', props.yChannel);
    }
  });

  if (gaussianBlurRef.value) {
    gaussianBlurRef.value.setAttribute('stdDeviation', props.displace.toString());
  }
};

const setupResizeObserver = () => {
  if (!containerRef.value || typeof ResizeObserver === 'undefined') return;
  resizeObserver = new ResizeObserver(() => {
    setTimeout(updateDisplacementMap, 0);
  });
  resizeObserver.observe(containerRef.value);
};

watch(
  [
    () => props.width,
    () => props.height,
    () => props.borderRadius,
    () => props.borderWidth,
    () => props.brightness,
    () => props.opacity,
    () => props.blur,
    () => props.displace,
    () => props.distortionScale,
    () => props.redOffset,
    () => props.greenOffset,
    () => props.blueOffset,
    () => props.xChannel,
    () => props.yChannel,
    () => props.mixBlendMode
  ],
  () => {
    updateDisplacementMap();
    updateFilterElements();
  }
);

watch([() => props.width, () => props.height], () => {
  setTimeout(updateDisplacementMap, 0);
});

onMounted(() => {
  nextTick(() => {
    updateDisplacementMap();
    updateFilterElements();
    setupResizeObserver();
  });
});

// Cleanup at top level (not nested in onMounted) — Vue lifecycle best practice.
onUnmounted(() => {
  if (resizeObserver) {
    resizeObserver.disconnect();
    resizeObserver = null;
  }
});
</script>

<template>
  <div
    ref="containerRef"
    class="gs-container"
    :class="[isDarkMode ? 'gs-focus-dark' : 'gs-focus-light', className]"
    :style="containerStyles"
  >
    <svg class="gs-svg" xmlns="http://www.w3.org/2000/svg">
      <defs>
        <filter :id="filterId" color-interpolation-filters="sRGB" x="0%" y="0%" width="100%" height="100%">
          <feImage ref="feImageRef" x="0" y="0" width="100%" height="100%" preserveAspectRatio="none" result="map" />

          <feDisplacementMap ref="redChannelRef" in="SourceGraphic" in2="map" id="redchannel" result="dispRed" />
          <feColorMatrix
            in="dispRed"
            type="matrix"
            values="1 0 0 0 0
                    0 0 0 0 0
                    0 0 0 0 0
                    0 0 0 1 0"
            result="red"
          />

          <feDisplacementMap ref="greenChannelRef" in="SourceGraphic" in2="map" id="greenchannel" result="dispGreen" />
          <feColorMatrix
            in="dispGreen"
            type="matrix"
            values="0 0 0 0 0
                    0 1 0 0 0
                    0 0 0 0 0
                    0 0 0 1 0"
            result="green"
          />

          <feDisplacementMap ref="blueChannelRef" in="SourceGraphic" in2="map" id="bluechannel" result="dispBlue" />
          <feColorMatrix
            in="dispBlue"
            type="matrix"
            values="0 0 0 0 0
                    0 0 0 0 0
                    0 0 1 0 0
                    0 0 0 1 0"
            result="blue"
          />

          <feBlend in="red" in2="green" mode="screen" result="rg" />
          <feBlend in="rg" in2="blue" mode="screen" result="output" />
          <feGaussianBlur ref="gaussianBlurRef" in="output" stdDeviation="0.7" />
        </filter>
      </defs>
    </svg>

    <div class="gs-content">
      <slot />
    </div>
  </div>
</template>

<style scoped>
/* Self-contained styles (no Tailwind required) — keeps the preset portable. */
.gs-container {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  transition: opacity 260ms ease-out;
}

.gs-svg {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
  opacity: 0;
  z-index: -10;
}

.gs-content {
  position: relative;
  z-index: 10;
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0.5rem;
  border-radius: inherit;
}

/* Focus-visible outlines (dark / light variants) */
.gs-focus-dark:focus-visible {
  outline: 2px solid var(--fluen-accent);
  outline-offset: 2px;
}
.gs-focus-light:focus-visible {
  outline: 2px solid var(--fluen-accent);
  outline-offset: 2px;
}
</style>
