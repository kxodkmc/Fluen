/**
 * Curated Aurora color presets.
 *
 * Each preset defines the three `colorStops` passed to the Aurora shader.
 * Color stops at index 0 and 2 act as "edges" — typically a dark tone that
 * fades the aurora into the background — while index 1 is the main accent.
 */

export interface AuroraPreset {
  /** Human-readable name shown in the controller. */
  name: string;
  /** Three hex colors: [leftEdge, accent, rightEdge]. */
  colorStops: [string, string, string];
}

export const auroraPresets: readonly AuroraPreset[] = [
  { name: 'Coral',    colorStops: ['#121212', '#ff5530', '#121212'] },
  { name: 'Magenta',  colorStops: ['#121212', '#ea5ec1', '#121212'] },
  { name: 'Ocean',    colorStops: ['#121212', '#3b82f6', '#121212'] },
  { name: 'Purple',   colorStops: ['#121212', '#a855f7', '#121212'] },
  { name: 'Cyan',     colorStops: ['#121212', '#3daeff', '#121212'] },
  { name: 'Sunset',   colorStops: ['#3A29FF', '#FF94B4', '#FF3232'] },
  { name: 'Twilight', colorStops: ['#0a0a0a', '#6366f1', '#0a0a0a'] },
] as const;
