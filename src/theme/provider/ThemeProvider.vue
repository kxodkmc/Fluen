<script setup lang="ts">
/**
 * ThemeProvider — 主题提供者根组件。
 *
 * 挂载于应用根部（App.vue），负责：
 *   - 应用启动时调用 useTheme().initTheme() 从 AppConfig 加载主题
 *   - 渲染 slot 内容（不添加任何包装 DOM，避免破坏布局）
 *
 * 主题状态通过 useTheme 单例共享，后代组件无需 inject 即可消费。
 */
import { onMounted } from 'vue';
import { useTheme } from '../composables/useTheme';

const { initTheme, initialized } = useTheme();

onMounted(() => {
  void initTheme();
});
</script>

<template>
  <slot :initialized="initialized" />
</template>
