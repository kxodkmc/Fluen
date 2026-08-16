import { defineConfig } from 'vitest/config';
import vue from '@vitejs/plugin-vue';

export default defineConfig({
  plugins: [vue()],
  test: {
    include: ['src/**/*.{test,spec}.ts'],
    includeSource: ['src/views/main/components/editor/codemirror/**/*.ts', 'src/shortcuts/**/*.ts'],
    environment: 'jsdom',
    globals: false,
  },
});
