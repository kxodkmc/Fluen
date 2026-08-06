<script lang="ts">
export interface AnimatedListItemProps {
  /** Zero-based index of the item. Used for scroll targeting. */
  index: number;
  /** Animation delay in seconds before the item plays its entrance. */
  delay?: number;
}

export type AnimatedListItemEmits = {
  (e: 'mouseenter'): void;
  (e: 'click'): void;
};
</script>

<script setup lang="ts">
import { motion, useInView } from 'motion-v';
import { useTemplateRef } from 'vue';

const props = withDefaults(defineProps<AnimatedListItemProps>(), {
  delay: 0
});

const emit = defineEmits<AnimatedListItemEmits>();

const itemRef = useTemplateRef<HTMLElement>('itemRef');
const inView = useInView(itemRef, { amount: 0.5, once: false });
</script>

<template>
  <div
    ref="itemRef"
    class="al-item"
    :data-index="index"
    role="option"
    @mouseenter="emit('mouseenter')"
    @click="emit('click')"
  >
    <motion.div
      :initial="{ scale: 0.7, opacity: 0 }"
      :animate="inView ? { scale: 1, opacity: 1 } : { scale: 0.7, opacity: 0 }"
      :transition="{ duration: 0.2, delay }"
    >
      <slot />
    </motion.div>
  </div>
</template>

<style scoped>
/* Self-contained styles (no Tailwind required) — keeps the preset portable. */

.al-item {
  margin-bottom: 1rem;
  cursor: pointer;
}
</style>
