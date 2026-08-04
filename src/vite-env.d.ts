/// <reference types="vite/client" />
/// <reference types="vitest/globals" />

declare module "*.vue" {
  import type { DefineComponent } from "vue";
  const component: DefineComponent<{}, {}, any>;
  export default component;
}

/**
 * vitest in-source testing augmentation.
 *
 * `import.meta.vitest` is truthy at test time (vitest config), falsy in
 * production (vite.config.ts `define: { 'import.meta.vitest': 'false' }`).
 * Source files guard inline tests with `if (import.meta.vitest) { ... }` so
 * the production bundle tree-shakes them out.
 */
interface ImportMeta {
  readonly vitest?: typeof import('vitest');
}
