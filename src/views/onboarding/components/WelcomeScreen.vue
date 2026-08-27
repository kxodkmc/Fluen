<script setup lang="ts">
/**
 * WelcomeScreen — the opening phase of the onboarding flow.
 *
 * Displays an animated greeting "你好！欢迎使用 Fluen" using motion-v for the
 * entrance, where the app name inherits the ShinyText preset for the shimmering
 * effect, followed by a short description and a "开始配置" call-to-action button.
 */
import { Motion } from 'motion-v';
import { ShinyText } from '../../../presets';
import { APP_NAME } from '../../../utils/appInfo';
import { useI18n } from '../../../i18n';

defineEmits<{
  (e: 'start'): void;
}>();

const { t } = useI18n();
</script>

<template>
  <Motion
    as="div"
    class="welcome"
    :initial="{ opacity: 0 }"
    :animate="{ opacity: 1 }"
    :exit="{ opacity: 0, scale: 0.96 }"
    :transition="{ duration: 0.5, ease: 'easeOut' }"
  >
    <!-- Greeting: 你好！欢迎使用 + shimmering app name -->
    <Motion
      as="div"
      class="welcome__greeting"
      :initial="{ y: 30, opacity: 0 }"
      :animate="{ y: 0, opacity: 1 }"
      :transition="{ delay: 0.2, duration: 0.6, ease: 'easeOut' }"
    >
      <span class="welcome__greeting-text">{{ t('onboarding.welcome.greeting') }}</span>
      <ShinyText
        :text="APP_NAME"
        :speed="4"
        :spread="80"
        color="rgba(255, 255, 255, 0.25)"
        shine-color="#ffffff"
        class="welcome__greeting-shimmer"
      />
    </Motion>

    <!-- Description -->
    <Motion
      as="p"
      class="welcome__desc"
      :initial="{ y: 20, opacity: 0 }"
      :animate="{ y: 0, opacity: 1 }"
      :transition="{ delay: 0.4, duration: 0.6, ease: 'easeOut' }"
    >
      {{ t('onboarding.welcome.description') }}
    </Motion>

    <!-- CTA button -->
    <Motion
      as="button"
      class="welcome__cta"
      :initial="{ y: 20, opacity: 0 }"
      :animate="{ y: 0, opacity: 1 }"
      :transition="{ delay: 0.55, duration: 0.6, ease: 'easeOut' }"
      @click="$emit('start')"
    >
      {{ t('onboarding.welcome.startButton') }}
      <svg
        class="welcome__cta-arrow"
        viewBox="0 0 24 24"
        width="18"
        height="18"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path d="M5 12h14" />
        <path d="m12 5 7 7-7 7" />
      </svg>
    </Motion>
  </Motion>
</template>

<style scoped>
.welcome {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  padding: 2rem;
  max-width: 560px;
}

/* ── Greeting ─────────────────────────────────────────────────────────── */
/* 两行排版：第一行问候语，第二行闪烁的应用名 */
.welcome__greeting {
  display: flex;
  flex-direction: column;
  align-items: center;
  margin-bottom: 1.5rem;
  font-family: var(--fluen-font-sans);
  font-size: clamp(2.5rem, 6vw, 4rem);
  font-weight: 600;
  line-height: 1.1;
  letter-spacing: -0.03em;
}

.welcome__greeting-text {
  background: linear-gradient(135deg, var(--fluen-on-dark) 0%, var(--fluen-muted) 100%);
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
}

.welcome__greeting-shimmer {
  font-size: clamp(2.5rem, 6vw, 4rem);
  font-weight: 600;
  letter-spacing: -0.02em;
}

/* ── Description ──────────────────────────────────────────────────────── */
.welcome__desc {
  margin: 0 0 2.5rem;
  font-family: var(--fluen-font-sans);
  font-size: 0.85rem;
  font-weight: 400;
  color: var(--fluen-stone);
}

/* ── CTA button ───────────────────────────────────────────────────────── */
.welcome__cta {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  padding: 12px 28px;
  border: none;
  border-radius: 9999px;
  background: var(--fluen-primary);
  color: var(--fluen-on-primary);
  font-family: var(--fluen-font-sans);
  font-size: 0.9rem;
  font-weight: 600;
  cursor: pointer;
  transition: transform 0.2s ease, box-shadow 0.2s ease;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.3);
}

.welcome__cta:hover {
  transform: translateY(-1px);
  box-shadow: 0 6px 28px rgba(0, 0, 0, 0.4);
}

.welcome__cta:active {
  transform: translateY(0);
}

.welcome__cta-arrow {
  transition: transform 0.2s ease;
}

.welcome__cta:hover .welcome__cta-arrow {
  transform: translateX(3px);
}
</style>
