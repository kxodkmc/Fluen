// Reusable preset components.
// Each entry below is a self-contained, copy-friendly component intended to
// be lightweight, stable, and easy to compose across projects.
//
// Organization: components are grouped by category. To add a new component,
// drop it into the matching category folder and re-export from that folder's
// index.ts — this top-level file only re-exports categories.
//
//   presets/
//   ├─ backgrounds/ — full-screen animated backgrounds (aurora, gradients, ...)
//   ├─ layouts/     — cards, docks, surfaces, steppers (containers & structure)
//   ├─ movement/    — motion-driven decoration (parallax, ribbons, ...)
//   ├─ text/        — animated text/number, counters, typewriter, ...
//   ├─ effects/     — interaction feedback (click sparks, ripples, ...)
//   └─ controls/    — interactive input controls (sliders, toggles, ...)
//
// Each component is framework-light: no Tailwind required, scoped CSS only,
// so the preset library stays portable across projects.
export * from './backgrounds';
export * from './layouts';
export * from './movement';
export * from './text';
export * from './effects';
export * from './controls';
