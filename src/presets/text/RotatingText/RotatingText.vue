<script lang="ts">
import type { MotionProps } from 'motion-v';

export type StaggerFrom = 'first' | 'last' | 'center' | 'random' | number;
export type SplitBy = 'characters' | 'words' | 'lines';

type TransitionType = NonNullable<MotionProps['transition']>;
type InitialType = NonNullable<MotionProps['initial']>;
type AnimateType = NonNullable<MotionProps['animate']>;
type ExitType = NonNullable<MotionProps['exit']>;

export interface RotatingTextProps {
  /** An array of text strings to be rotated. */
  texts: string[];
  /** Transition settings for the animations. */
  transition?: TransitionType;
  /** Initial animation state for each element. */
  initial?: InitialType;
  /** Animation state when elements enter. */
  animate?: AnimateType;
  /** Exit animation state for elements. */
  exit?: ExitType;
  /** Mode for AnimatePresence ('wait' finishes exit before enter). */
  animatePresenceMode?: 'sync' | 'wait';
  /** Whether the AnimatePresence component should run its initial animation. */
  animatePresenceInitial?: boolean;
  /** The interval (in milliseconds) between text rotations. */
  rotationInterval?: number;
  /** Delay between each character's animation. */
  staggerDuration?: number;
  /** Specifies the order from which the stagger starts. */
  staggerFrom?: StaggerFrom;
  /** Determines if the rotation should loop back to the first text after the last one. */
  loop?: boolean;
  /** If true, the text rotation starts automatically. */
  auto?: boolean;
  /** Determines how the text is split into animatable elements. */
  splitBy?: SplitBy;
  /** Callback function invoked when the text rotates to the next item. */
  onNext?: (index: number) => void;
  /** Additional class names for the main container element. */
  mainClassName?: string;
  /** Additional class names for the container wrapping each split group (e.g., a word). */
  splitLevelClassName?: string;
  /** Additional class names for each individual animated element. */
  elementLevelClassName?: string;
}
</script>

<script setup lang="ts">
import { AnimatePresence, Motion } from 'motion-v';
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';

interface WordElement {
  characters: string[];
  needsSpace: boolean;
}

// Merge class names (falsy values are filtered out). Preserves user-provided
// classes from mainClassName / splitLevelClassName / elementLevelClassName.
const cn = (...classes: (string | undefined | null | boolean)[]): string => {
  return classes.filter(Boolean).join(' ');
};

const props = withDefaults(defineProps<RotatingTextProps>(), {
  transition: () =>
    ({
      type: 'spring',
      damping: 25,
      stiffness: 300
    }) as TransitionType,
  initial: () => ({ y: '100%', opacity: 0 }) as InitialType,
  animate: () => ({ y: 0, opacity: 1 }) as AnimateType,
  exit: () => ({ y: '-120%', opacity: 0 }) as ExitType,
  animatePresenceMode: 'wait',
  animatePresenceInitial: false,
  rotationInterval: 2000,
  staggerDuration: 0,
  staggerFrom: 'first',
  loop: true,
  auto: true,
  splitBy: 'characters',
  onNext: undefined,
  mainClassName: '',
  splitLevelClassName: '',
  elementLevelClassName: ''
});

const currentTextIndex = ref(0);
let intervalId: ReturnType<typeof setInterval> | null = null;

const splitIntoCharacters = (text: string): string[] => {
  if (typeof Intl !== 'undefined' && 'Segmenter' in Intl) {
    const IntlWithSegmenter = Intl as typeof Intl & {
      Segmenter: new (
        locales?: string | string[],
        options?: { granularity: 'grapheme' | 'word' | 'sentence' }
      ) => {
        segment: (text: string) => Iterable<{ segment: string }>;
      };
    };
    const segmenter = new IntlWithSegmenter.Segmenter('en', { granularity: 'grapheme' });
    return [...segmenter.segment(text)].map(({ segment }) => segment);
  }

  return [...text];
};

const elements = computed((): WordElement[] => {
  const currentText = props.texts[currentTextIndex.value] ?? '';

  switch (props.splitBy) {
    case 'characters': {
      const words = currentText.split(' ');
      return words.map((word, i) => ({
        characters: splitIntoCharacters(word),
        needsSpace: i !== words.length - 1
      }));
    }
    case 'words': {
      const words = currentText.split(' ');
      return words.map((word, i) => ({
        characters: [word],
        needsSpace: i !== words.length - 1
      }));
    }
    case 'lines': {
      const lines = currentText.split('\n');
      return lines.map((line, i) => ({
        characters: [line],
        needsSpace: i !== lines.length - 1
      }));
    }
    default: {
      const parts = currentText.split(props.splitBy as string);
      return parts.map((part, i) => ({
        characters: [part],
        needsSpace: i !== parts.length - 1
      }));
    }
  }
});

const getStaggerDelay = (index: number, totalChars: number): number => {
  const { staggerDuration, staggerFrom } = props;

  switch (staggerFrom) {
    case 'first':
      return index * staggerDuration;
    case 'last':
      return (totalChars - 1 - index) * staggerDuration;
    case 'center': {
      const center = Math.floor(totalChars / 2);
      return Math.abs(center - index) * staggerDuration;
    }
    case 'random': {
      const randomIndex = Math.floor(Math.random() * totalChars);
      return Math.abs(randomIndex - index) * staggerDuration;
    }
    default:
      return Math.abs((staggerFrom as number) - index) * staggerDuration;
  }
};

const handleIndexChange = (newIndex: number): void => {
  currentTextIndex.value = newIndex;
  props.onNext?.(newIndex);
};

const next = (): void => {
  const isAtEnd = currentTextIndex.value === props.texts.length - 1;
  const nextIndex = isAtEnd ? (props.loop ? 0 : currentTextIndex.value) : currentTextIndex.value + 1;

  if (nextIndex !== currentTextIndex.value) {
    handleIndexChange(nextIndex);
  }
};

const previous = (): void => {
  const isAtStart = currentTextIndex.value === 0;
  const prevIndex = isAtStart
    ? props.loop
      ? props.texts.length - 1
      : currentTextIndex.value
    : currentTextIndex.value - 1;

  if (prevIndex !== currentTextIndex.value) {
    handleIndexChange(prevIndex);
  }
};

const jumpTo = (index: number): void => {
  const validIndex = Math.max(0, Math.min(index, props.texts.length - 1));
  if (validIndex !== currentTextIndex.value) {
    handleIndexChange(validIndex);
  }
};

const reset = (): void => {
  if (currentTextIndex.value !== 0) {
    handleIndexChange(0);
  }
};

const cleanupInterval = (): void => {
  if (intervalId) {
    clearInterval(intervalId);
    intervalId = null;
  }
};

const startInterval = (): void => {
  if (props.auto) {
    intervalId = setInterval(next, props.rotationInterval);
  }
};

defineExpose({
  next,
  previous,
  jumpTo,
  reset
});

watch(
  () => props,
  () => {
    cleanupInterval();
    startInterval();
  }
);

onMounted(() => {
  startInterval();
});

onUnmounted(() => {
  cleanupInterval();
});
</script>

<template>
  <Motion
    tag="span"
    :class="cn('rt-main', mainClassName)"
    v-bind="$attrs"
    :transition="transition"
    layout
  >
    <span class="rt-sr-only">
      {{ texts[currentTextIndex] }}
    </span>

    <AnimatePresence :mode="animatePresenceMode" :initial="animatePresenceInitial">
      <Motion
        :key="currentTextIndex"
        tag="span"
        :class="splitBy === 'lines' ? 'rt-lines' : 'rt-wrap'"
        aria-hidden="true"
        layout
      >
        <span v-for="(wordObj, wordIndex) in elements" :key="wordIndex" :class="cn('rt-split', splitLevelClassName)">
          <Motion
            v-for="(char, charIndex) in wordObj.characters"
            :key="charIndex"
            tag="span"
            :initial="initial"
            :animate="animate"
            :exit="exit"
            :transition="{
              ...transition,
              delay: getStaggerDelay(
                elements.slice(0, wordIndex).reduce((sum, word) => sum + word.characters.length, 0) + charIndex,
                elements.reduce((sum, word) => sum + word.characters.length, 0)
              )
            }"
            :class="cn('rt-char', elementLevelClassName)"
          >
            {{ char }}
          </Motion>
          <span v-if="wordObj.needsSpace" class="rt-space"></span>
        </span>
      </Motion>
    </AnimatePresence>
  </Motion>
</template>

<style scoped>
/* Self-contained styles (no Tailwind required) — keeps the preset portable.
   User-provided classes from mainClassName / splitLevelClassName /
   elementLevelClassName are merged via cn() and apply alongside these. */
.rt-main {
  display: flex;
  flex-wrap: wrap;
  white-space: pre-wrap;
  position: relative;
}

.rt-wrap {
  display: flex;
  flex-wrap: wrap;
  white-space: pre-wrap;
  position: relative;
}

.rt-lines {
  display: flex;
  flex-direction: column;
  width: 100%;
}

.rt-split {
  display: inline-flex;
}

.rt-char {
  display: inline-block;
}

.rt-space {
  white-space: pre;
}

/* Screen-reader-only: visible to assistive tech, hidden visually. */
.rt-sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}
</style>
