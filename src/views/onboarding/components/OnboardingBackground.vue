<script setup lang="ts">
/**
 * OnboardingBackground — wraps the DarkVeil animated background preset at a
 * fixed speed of 2 and layers a subtle vignette / content overlay on top.
 *
 * This keeps DarkVeil usage consistent across the onboarding flow and
 * isolates all background-related styling in one place.
 */
import { DarkVeil } from '../../../presets';
</script>

<template>
  <div class="ob-bg">
    <!-- Animated WebGL background (speed = 2) -->
    <DarkVeil :speed="2" class="ob-bg__canvas" />

    <!-- Vignette + gradient overlay for text legibility -->
    <div class="ob-bg__overlay" />

    <!-- Window drag region — transparent top bar, consistent with TitleBar -->
    <div class="ob-bg__drag" data-tauri-drag-region />

    <!-- Foreground content slot -->
    <div class="ob-bg__content">
      <slot />
    </div>
  </div>
</template>

<style scoped>
.ob-bg {
  position: relative;
  width: 100%;
  height: 100%;
  overflow: hidden;
}

.ob-bg__canvas {
  position: absolute;
  inset: 0;
  z-index: 0;
}

.ob-bg__overlay {
  position: absolute;
  inset: 0;
  z-index: 1;
  pointer-events: none;
  background:
    radial-gradient(ellipse at center, transparent 0%, rgba(0, 0, 0, 0.45) 100%),
    linear-gradient(180deg, rgba(0, 0, 0, 0.2) 0%, transparent 30%, transparent 70%, rgba(0, 0, 0, 0.3) 100%);
}

/* ── 窗口拖拽区域 ─────────────────────────────────────────────────────── */
/* 透明顶部条，高度与主界面 TitleBar（44px）一致，                   */
/* 内容垂直居中，顶部区域通常无交互元素，不会产生冲突。               */
.ob-bg__drag {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 44px;
  z-index: 3;
  -webkit-app-region: drag;
}

.ob-bg__content {
  position: relative;
  z-index: 2;
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
}
</style>
