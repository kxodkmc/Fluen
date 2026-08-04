<script lang="ts">
import type { ButtonHTMLAttributes, Component, VNode } from 'vue';

export interface StepperProps {
  /** The first step to display when the stepper is initialized (1-based). */
  initialStep?: number;
  /** Callback fired whenever the step changes. */
  onStepChange?: (step: number) => void;
  /** Callback fired when the stepper completes its final step. */
  onFinalStepCompleted?: () => void;
  /** Custom class name for the outer card container. */
  stepCircleContainerClassName?: string;
  /** Custom class name for the row holding the step circles / connectors. */
  stepContainerClassName?: string;
  /** Custom class name for the step's main content container. */
  contentClassName?: string;
  /** Custom class name for the footer area containing navigation buttons. */
  footerClassName?: string;
  /** Extra props passed to the Back button. */
  backButtonProps?: ButtonHTMLAttributes;
  /** Extra props passed to the Next / Complete button. */
  nextButtonProps?: ButtonHTMLAttributes;
  /** Text for the Back button. */
  backButtonText?: string;
  /** Text for the Next button when not on the last step. */
  nextButtonText?: string;
  /** Text for the Complete button on the last step. */
  completeButtonText?: string;
  /** Disables click interaction on step indicators. */
  disableStepIndicators?: boolean;
  /** Renders a custom step indicator component. */
  renderStepIndicator?: Component;
  /** Accent color for active / completed indicators and the next button. */
  accentColor?: string;
  /** Hover color for the next button. */
  accentColorHover?: string;
}

export type StepperEmits = {
  /** Emitted whenever the step changes. */
  (e: 'step-change', step: number): void;
  /** Emitted when the stepper completes its final step. */
  (e: 'final-step-completed'): void;
};
</script>

<script setup lang="ts">
import { AnimatePresence, Motion } from 'motion-v';
import { Comment, Fragment, Text, computed, ref, useSlots, type Directive } from 'vue';

/* ------------------------------------------------------------------ *
 * Props & Emits
 * ------------------------------------------------------------------ */

const props = withDefaults(defineProps<StepperProps>(), {
  initialStep: 1,
  onStepChange: () => {},
  onFinalStepCompleted: () => {},
  stepCircleContainerClassName: '',
  stepContainerClassName: '',
  contentClassName: '',
  footerClassName: '',
  backButtonProps: () => ({}),
  nextButtonProps: () => ({}),
  backButtonText: 'Back',
  nextButtonText: 'Continue',
  completeButtonText: 'Complete',
  disableStepIndicators: false,
  renderStepIndicator: undefined,
  accentColor: 'var(--fluen-accent)',
  accentColorHover: 'var(--fluen-accent-hover)'
});

const emit = defineEmits<StepperEmits>();

/* ------------------------------------------------------------------ *
 * State
 * ------------------------------------------------------------------ */

const slots = useSlots();
const currentStep = ref<number>(props.initialStep);
const direction = ref<number>(0);
const parentHeight = ref<number>(0);

/* ------------------------------------------------------------------ *
 * Computed
 * ------------------------------------------------------------------ */

const stepsArray = computed<VNode[]>(() => {
  const defaultSlot = slots.default?.() ?? [];
  return defaultSlot
    .flatMap((node) =>
      node.type === Fragment ? (node.children as VNode[]) : node
    )
    .filter((node) => node.type !== Comment && node.type !== Text);
});

const totalSteps = computed<number>(() => stepsArray.value.length);
const isCompleted = computed<boolean>(() => currentStep.value > totalSteps.value);
const isLastStep = computed<boolean>(() => currentStep.value === totalSteps.value);

/* ------------------------------------------------------------------ *
 * Layout height directive — measures the rendered step height so the
 * container can animate to match.
 * ------------------------------------------------------------------ */

const vLayoutHeight: Directive<HTMLElement, (height: number) => void> = {
  mounted(el, binding) {
    binding.value(el.offsetHeight);
  },
  updated(el, binding) {
    binding.value(el.offsetHeight);
  }
};

const measureHeight = (height: number): void => {
  parentHeight.value = height;
};

/* ------------------------------------------------------------------ *
 * Step logic
 * ------------------------------------------------------------------ */

type StepStatus = 'active' | 'inactive' | 'complete';

const getStepStatus = (step: number): StepStatus => {
  if (currentStep.value === step) return 'active';
  if (currentStep.value < step) return 'inactive';
  return 'complete';
};

const updateStep = (newStep: number): void => {
  currentStep.value = newStep;
  if (newStep > totalSteps.value) {
    props.onFinalStepCompleted();
    emit('final-step-completed');
  } else {
    props.onStepChange(newStep);
    emit('step-change', newStep);
  }
};

const handleBack = (): void => {
  if (currentStep.value > 1) {
    direction.value = -1;
    updateStep(currentStep.value - 1);
  }
};

const handleNext = (): void => {
  if (!isLastStep.value) {
    direction.value = 1;
    updateStep(currentStep.value + 1);
  }
};

const handleComplete = (): void => {
  direction.value = 1;
  updateStep(totalSteps.value + 1);
};

const handleStepIndicatorClick = (step: number): void => {
  if (step !== currentStep.value && !props.disableStepIndicators) {
    direction.value = step > currentStep.value ? 1 : -1;
    updateStep(step);
  }
};

const handleCustomStepClick = (clicked: number): void => {
  direction.value = clicked > currentStep.value ? 1 : -1;
  updateStep(clicked);
};

/* ------------------------------------------------------------------ *
 * Motion variants
 * ------------------------------------------------------------------ */

const stepVariants = {
  enter: (dir: number) => ({
    x: dir >= 0 ? '-100%' : '100%',
    opacity: 0
  }),
  center: {
    x: '0%',
    opacity: 1
  },
  exit: (dir: number) => ({
    x: dir >= 0 ? '50%' : '-50%',
    opacity: 0
  })
};

const indicatorVariants = computed(() => ({
  inactive: { scale: 1, backgroundColor: 'var(--fluen-charcoal)', color: 'var(--fluen-stone)' },
  active: { scale: 1, backgroundColor: props.accentColor, color: props.accentColor },
  complete: { scale: 1, backgroundColor: props.accentColor, color: props.accentColorHover }
}));

const lineVariants = computed(() => ({
  incomplete: { width: 0, backgroundColor: 'transparent' },
  complete: { width: '100%', backgroundColor: props.accentColor }
}));
</script>

<template>
  <div class="st-root">
    <div :class="['st-card', stepCircleContainerClassName]">
      <!-- ── Step indicators ─────────────────────────────────────── -->
      <div :class="['st-indicators', stepContainerClassName]">
        <template v-for="(_, index) in stepsArray" :key="index + 1">
          <component
            :is="renderStepIndicator"
            v-if="renderStepIndicator"
            :step="index + 1"
            :current-step="currentStep"
            :on-step-click="handleCustomStepClick"
          />

          <div
            v-else
            class="st-indicator"
            :class="{ 'st-indicator--disabled': disableStepIndicators }"
            @click="handleStepIndicatorClick(index + 1)"
          >
            <Motion
              as="div"
              class="st-indicator-circle"
              :animate="getStepStatus(index + 1)"
              :variants="indicatorVariants"
              :initial="false"
              :transition="{ duration: 0.3 }"
            >
              <svg
                v-if="getStepStatus(index + 1) === 'complete'"
                class="st-check-icon"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                viewBox="0 0 24 24"
              >
                <Motion
                  as="path"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  d="M5 13l4 4L19 7"
                  :initial="{ pathLength: 0 }"
                  :animate="{ pathLength: 1 }"
                  :transition="{ delay: 0.1, type: 'tween', ease: 'easeOut', duration: 0.3 }"
                />
              </svg>
              <div v-else-if="getStepStatus(index + 1) === 'active'" class="st-active-dot" />
              <span v-else class="st-indicator-num">{{ index + 1 }}</span>
            </Motion>
          </div>

          <!-- Connector line between indicators -->
          <div v-if="index < totalSteps - 1" class="st-connector">
            <Motion
              as="div"
              class="st-connector-fill"
              :variants="lineVariants"
              :initial="false"
              :animate="currentStep > index + 1 ? 'complete' : 'incomplete'"
              :transition="{ duration: 0.4 }"
            />
          </div>
        </template>
      </div>

      <!-- ── Content area ────────────────────────────────────────── -->
      <Motion
        as="div"
        :class="['st-content', contentClassName]"
        :animate="{ height: isCompleted ? 0 : parentHeight }"
        :transition="{ type: 'spring', duration: 0.4 }"
      >
        <AnimatePresence :initial="false" mode="sync" :custom="direction">
          <Motion
            v-if="!isCompleted"
            v-layout-height="measureHeight"
            as="div"
            :key="currentStep"
            :custom="direction"
            :variants="stepVariants"
            initial="enter"
            animate="center"
            exit="exit"
            :transition="{ duration: 0.4 }"
            class="st-step"
          >
            <div class="st-step-inner">
              <component :is="stepsArray[currentStep - 1]" />
            </div>
          </Motion>
        </AnimatePresence>
      </Motion>

      <!-- ── Footer with navigation buttons ──────────────────────── -->
      <div v-if="!isCompleted" :class="['st-footer', footerClassName]">
        <div :class="['st-footer-row', { 'st-footer-row--start': currentStep !== 1 }]">
          <button
            v-if="currentStep !== 1"
            class="st-back-btn"
            v-bind="backButtonProps"
            @click="handleBack"
          >
            {{ backButtonText }}
          </button>
          <button
            class="st-next-btn"
            v-bind="nextButtonProps"
            @click="isLastStep ? handleComplete() : handleNext()"
          >
            {{ isLastStep ? completeButtonText : nextButtonText }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* Self-contained styles (no Tailwind required) — keeps the preset portable. */

.st-root {
  display: flex;
  flex-direction: column;
  flex: 1;
  justify-content: center;
  align-items: center;
  padding: 1rem;
  min-height: 100%;
}

.st-card {
  margin: 0 auto;
  width: 100%;
  max-width: 28rem;
  border-radius: 2rem;
  border: 1px solid var(--fluen-charcoal);
  box-shadow: var(--fluen-shadow-modal);
  overflow: hidden;
  box-sizing: border-box;
}

/* ── Step indicators ────────────────────────────────────────────── */
.st-indicators {
  display: flex;
  width: 100%;
  align-items: center;
  padding: 1.5rem 1rem;
  box-sizing: border-box;
}

@media (min-width: 480px) {
  .st-indicators {
    padding: 2rem;
  }
}

.st-indicator {
  position: relative;
  outline: none;
  cursor: pointer;
}

.st-indicator--disabled {
  pointer-events: none;
  opacity: 0.5;
}

.st-indicator-circle {
  display: flex;
  justify-content: center;
  align-items: center;
  border-radius: 9999px;
  width: 1.75rem;
  height: 1.75rem;
  font-weight: 600;
  font-size: 0.8rem;
  flex-shrink: 0;
}

@media (min-width: 480px) {
  .st-indicator-circle {
    width: 2rem;
    height: 2rem;
    font-size: 0.875rem;
  }
}

.st-check-icon {
  width: 1rem;
  height: 1rem;
  color: var(--fluen-on-accent);
}

.st-active-dot {
  background: var(--fluen-on-accent);
  border-radius: 9999px;
  width: 0.75rem;
  height: 0.75rem;
}

.st-indicator-num {
  font-size: 0.875rem;
}

.st-connector {
  position: relative;
  flex: 1;
  min-width: 8px;
  background: var(--fluen-steel);
  margin: 0 0.35rem;
  border-radius: 9999px;
  height: 2px;
  overflow: hidden;
}

@media (min-width: 480px) {
  .st-connector {
    margin: 0 0.5rem;
  }
}

.st-connector-fill {
  position: absolute;
  top: 0;
  left: 0;
  height: 100%;
}

/* ── Content ────────────────────────────────────────────────────── */
.st-content {
  position: relative;
  overflow: hidden;
}

.st-step {
  position: absolute;
  left: 0;
  right: 0;
  top: 0;
}

.st-step-inner {
  padding: 0 1rem;
}

@media (min-width: 480px) {
  .st-step-inner {
    padding: 0 2rem;
  }
}

/* ── Footer ─────────────────────────────────────────────────────── */
.st-footer {
  padding: 0 1rem 1.5rem;
}

@media (min-width: 480px) {
  .st-footer {
    padding: 0 2rem 2rem;
  }
}

.st-footer-row {
  display: flex;
  margin-top: 2.5rem;
  justify-content: flex-end;
}

.st-footer-row--start {
  justify-content: space-between;
}

.st-back-btn {
  cursor: pointer;
  border: none;
  background: transparent;
  border-radius: 0.25rem;
  padding: 0.25rem 0.5rem;
  color: var(--fluen-stone);
  font: inherit;
  transition: color 350ms ease;
}

.st-back-btn:hover {
  color: var(--fluen-steel);
}

.st-next-btn {
  display: flex;
  justify-content: center;
  align-items: center;
  background: v-bind(accentColor);
  border: none;
  padding: 0.375rem 0.875rem;
  border-radius: 9999px;
  font-weight: 500;
  color: var(--fluen-on-accent);
  letter-spacing: -0.025em;
  transition: background-color 350ms ease;
  cursor: pointer;
}

.st-next-btn:hover {
  background: v-bind(accentColorHover);
}
</style>
