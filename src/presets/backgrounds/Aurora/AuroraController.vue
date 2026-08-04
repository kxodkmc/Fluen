<script lang="ts">
export interface AuroraControllerProps {
  /** Initial preset index. Defaults to 0 (Emerald). */
  initialPresetIndex?: number;
  /** Animation speed — forwarded to Aurora. */
  speed?: number;
  /** Blend amount — forwarded to Aurora. */
  blend?: number;
  /** Amplitude — forwarded to Aurora. */
  amplitude?: number;
}

/** Emitted when the active color stops change (preset switch or manual edit). */
export interface AuroraControllerEmits {
  (e: 'change', colorStops: [string, string, string]): void;
}
</script>

<script setup lang="ts">
import { ref, computed } from 'vue';
import Aurora from './Aurora.vue';
import { auroraPresets } from './auroraPresets';
import { useI18n } from '../../../i18n';

const { t } = useI18n();

/* ------------------------------------------------------------------ *
 * Props & Emits
 * ------------------------------------------------------------------ */

const props = withDefaults(defineProps<AuroraControllerProps>(), {
  initialPresetIndex: 0,
  speed: 0.5,
  blend: 0.5,
  amplitude: 1.0
});

const emit = defineEmits<AuroraControllerEmits>();

/* ------------------------------------------------------------------ *
 * State
 * ------------------------------------------------------------------ */

/** Index into `auroraPresets`. `-1` means "custom" mode. */
const presetIndex = ref(props.initialPresetIndex);

/** Manual color overrides — only active in custom mode. */
const customColors = ref<[string, string, string]>([
  ...auroraPresets[props.initialPresetIndex].colorStops
]);

/** Whether the control panel is expanded. */
const isPanelOpen = ref(true);

/** Forwarded Aurora parameters (editable via sliders). */
const speed = ref(props.speed);
const blend = ref(props.blend);
const amplitude = ref(props.amplitude);

/* ------------------------------------------------------------------ *
 * Computed
 * ------------------------------------------------------------------ */

const isCustom = computed(() => presetIndex.value === -1);

const currentPreset = computed(() =>
  isCustom.value ? null : auroraPresets[presetIndex.value]
);

const activeColorStops = computed<[string, string, string]>(() =>
  isCustom.value ? customColors.value : currentPreset.value!.colorStops
);

/* ------------------------------------------------------------------ *
 * Actions
 * ------------------------------------------------------------------ */

/** Cycle to the next preset (wraps around). Exits custom mode. */
const nextPreset = () => {
  if (isCustom.value) {
    presetIndex.value = 0;
  } else {
    presetIndex.value = (presetIndex.value + 1) % auroraPresets.length;
  }
  // Sync custom colors so sliders stay warm if user switches to custom later.
  customColors.value = [...activeColorStops.value];
  emit('change', activeColorStops.value);
};

/** Apply a manual color change at a specific index — enters custom mode. */
const setCustomColor = (index: 0 | 1 | 2, color: string) => {
  if (!isCustom.value) {
    // Seed custom colors from the current preset before editing.
    customColors.value = [...activeColorStops.value];
    presetIndex.value = -1;
  }
  customColors.value[index] = color;
  emit('change', customColors.value);
};

const togglePanel = () => {
  isPanelOpen.value = !isPanelOpen.value;
};
</script>

<template>
  <div class="ac-wrapper">
    <!-- ── Background layer ────────────────────────────────────────── -->
    <div class="ac-bg">
      <Aurora
        :color-stops="activeColorStops"
        :speed="speed"
        :blend="blend"
        :amplitude="amplitude"
      />
    </div>

    <!-- ── Content layer ───────────────────────────────────────────── -->
    <div class="ac-content">
      <slot />
    </div>

    <!-- ── Floating control panel ──────────────────────────────────── -->
    <div class="ac-panel" :class="{ 'ac-panel--collapsed': !isPanelOpen }">
      <button class="ac-toggle" @click="togglePanel" :aria-label="isPanelOpen ? t('presets.aurora.controller.collapse') : t('presets.aurora.controller.expand')">
        <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.6 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
        </svg>
      </button>

      <Transition name="ac-slide">
        <div v-show="isPanelOpen" class="ac-panel-body">
          <!-- Preset row: "换一个" button + preset name -->
          <div class="ac-row ac-row--preset">
            <button class="ac-next-btn" @click="nextPreset">
              <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M21 2v6h-6" />
                <path d="M3 12a9 9 0 0 1 15-6.7L21 8" />
                <path d="M3 22v-6h6" />
                <path d="M21 12a9 9 0 0 1-15 6.7L3 16" />
              </svg>
              <span>{{ t('presets.aurora.controller.changePreset') }}</span>
            </button>
            <span class="ac-preset-name">{{ isCustom ? t('presets.aurora.controller.custom') : currentPreset?.name }}</span>
          </div>

          <!-- Color stops -->
          <div class="ac-row ac-row--colors">
            <label class="ac-color-cell" v-for="(color, i) in activeColorStops" :key="i">
              <input
                type="color"
                :value="color"
                @input="setCustomColor(i as 0 | 1 | 2, ($event.target as HTMLInputElement).value)"
                class="ac-color-input"
              />
              <span class="ac-color-swatch" :style="{ backgroundColor: color }" />
              <span class="ac-color-label">{{ [t('presets.aurora.controller.colorStops.start'), t('presets.aurora.controller.colorStops.main'), t('presets.aurora.controller.colorStops.end')][i] }}</span>
            </label>
          </div>

          <!-- Parameter sliders -->
          <div class="ac-row ac-row--sliders">
            <label class="ac-slider">
              <span class="ac-slider-label">{{ t('presets.aurora.controller.sliders.speed') }}</span>
              <input type="range" v-model.number="speed" min="0" max="3" step="0.1" class="ac-range" />
            </label>
            <label class="ac-slider">
              <span class="ac-slider-label">{{ t('presets.aurora.controller.sliders.blend') }}</span>
              <input type="range" v-model.number="blend" min="0" max="1" step="0.05" class="ac-range" />
            </label>
            <label class="ac-slider">
              <span class="ac-slider-label">{{ t('presets.aurora.controller.sliders.amplitude') }}</span>
              <input type="range" v-model.number="amplitude" min="0" max="2" step="0.1" class="ac-range" />
            </label>
          </div>
        </div>
      </Transition>
    </div>
  </div>
</template>

<style scoped>
/* ── Layout ──────────────────────────────────────────────────────── */
.ac-wrapper {
  position: relative;
  width: 100%;
  height: 100%;
  min-height: 100vh;
}

.ac-bg {
  position: fixed;
  inset: 0;
  z-index: 0;
  pointer-events: none;
}

.ac-content {
  position: relative;
  z-index: 1;
}

/* ── Control panel ───────────────────────────────────────────────── */
.ac-panel {
  position: fixed;
  top: 1.25rem;
  right: 1.25rem;
  z-index: 50;
  display: flex;
  align-items: flex-start;
  gap: 0.5rem;
}

.ac-toggle {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border: 1px solid rgba(255, 255, 255, 0.15);
  border-radius: 9999px;
  background: rgba(10, 10, 10, 0.6);
  backdrop-filter: blur(12px) saturate(1.4);
  -webkit-backdrop-filter: blur(12px) saturate(1.4);
  color: rgba(255, 255, 255, 0.7);
  cursor: pointer;
  transition: color 0.2s ease, border-color 0.2s ease, background 0.2s ease;
  flex-shrink: 0;
}

.ac-toggle:hover {
  color: #fff;
  border-color: rgba(255, 255, 255, 0.3);
  background: rgba(10, 10, 10, 0.8);
}

.ac-panel-body {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  padding: 1rem 1.1rem;
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 16px;
  background: rgba(10, 10, 10, 0.55);
  backdrop-filter: blur(16px) saturate(1.6);
  -webkit-backdrop-filter: blur(16px) saturate(1.6);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
  min-width: 260px;
}

/* ── Rows ────────────────────────────────────────────────────────── */
.ac-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.ac-row--preset {
  justify-content: space-between;
}

.ac-row--colors {
  justify-content: space-between;
  gap: 0.25rem;
}

.ac-row--sliders {
  flex-direction: column;
  align-items: stretch;
  gap: 0.5rem;
}

/* ── "换一个" button (primary CTA — pill per DESIGN.md) ──────────── */
.ac-next-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  padding: 7px 16px;
  border: none;
  border-radius: 9999px;
  background: #f6f6f6;
  color: #0a0a0a;
  font-size: 13px;
  font-weight: 600;
  font-family: inherit;
  cursor: pointer;
  transition: background 0.15s ease, transform 0.1s ease;
  white-space: nowrap;
}

.ac-next-btn:hover {
  background: #fff;
}

.ac-next-btn:active {
  transform: scale(0.97);
  background: #d4d4d4;
}

.ac-preset-name {
  font-size: 12px;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.6);
  letter-spacing: 0.02em;
}

/* ── Color inputs ────────────────────────────────────────────────── */
.ac-color-cell {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.3rem;
  cursor: pointer;
}

.ac-color-input {
  position: absolute;
  width: 1px;
  height: 1px;
  opacity: 0;
  pointer-events: none;
}

.ac-color-swatch {
  width: 28px;
  height: 28px;
  border-radius: 8px;
  border: 1px solid rgba(255, 255, 255, 0.2);
  transition: border-color 0.2s ease, transform 0.1s ease;
}

.ac-color-cell:hover .ac-color-swatch {
  border-color: rgba(255, 255, 255, 0.5);
  transform: scale(1.08);
}

.ac-color-label {
  font-size: 10px;
  color: rgba(255, 255, 255, 0.45);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

/* ── Sliders ─────────────────────────────────────────────────────── */
.ac-slider {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
}

.ac-slider-label {
  font-size: 11px;
  color: rgba(255, 255, 255, 0.5);
  letter-spacing: 0.03em;
  flex-shrink: 0;
  width: 2rem;
}

.ac-range {
  flex: 1;
  -webkit-appearance: none;
  appearance: none;
  height: 4px;
  border-radius: 9999px;
  background: rgba(255, 255, 255, 0.15);
  outline: none;
  cursor: pointer;
}

.ac-range::-webkit-slider-thumb {
  -webkit-appearance: none;
  appearance: none;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: #f6f6f6;
  cursor: pointer;
  transition: transform 0.1s ease;
}

.ac-range::-webkit-slider-thumb:hover {
  transform: scale(1.2);
}

.ac-range::-moz-range-thumb {
  width: 14px;
  height: 14px;
  border: none;
  border-radius: 50%;
  background: #f6f6f6;
  cursor: pointer;
}

/* ── Slide transition ────────────────────────────────────────────── */
.ac-slide-enter-active,
.ac-slide-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}

.ac-slide-enter-from,
.ac-slide-leave-to {
  opacity: 0;
  transform: translateX(8px);
}
</style>
