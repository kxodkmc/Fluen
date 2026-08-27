import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import pkg from "./package.json";
// @ts-expect-error type error without @types/node package
import process from "node:process";
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [vue()],

  // Tree-shake inline test guards (`if (import.meta.vitest)`) out of production builds.
  // vitest.config.ts provides the truthy value at test time; vite.config.ts is not
  // consulted by vitest when a dedicated vitest.config.ts exists.
  define: {
    'import.meta.vitest': 'false',
    // “关于”对话框的版本信息来源：package.json 是唯一维护点，构建期注入。
    __APP_VERSION__: JSON.stringify(pkg.version),
    __APP_LIB_VERSIONS__: JSON.stringify(pkg.dependencies),
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri` and `target` (Rust build output)
      ignored: ["**/src-tauri/**", "**/target/**"],
    },
  },
}));
