/**
 * 半预览（Live Preview）扩展的统一出口。
 *
 * 装配方式：把 `livePreviewExtension` 追加进编辑器 extensions 即可，
 * 之后经 `setLivePreviewEffect` / `isLivePreview` 动态开关——
 * 不重建 EditorState，光标与撤销历史全保留。
 */

export { livePreviewField, setLivePreviewEffect, isLivePreview } from './state';
export { livePreviewCore } from './plugin';
export { livePreviewTheme } from './theme';
export {
  buildLivePreviewDecorations,
  selectionTouches,
  type DecorationBuildResult,
  type SelInfo,
} from './decorations';
export { fluenMathExtension } from './mathSyntax';
export { scanMathRegions, type MathRegion } from './mathDisplayScan';
export { isSafeExternalHref } from './linkOpen';

import type { Extension } from '@codemirror/state';
import { livePreviewCore } from './plugin';
import { livePreviewTheme } from './theme';

/** 半预览完整扩展包：状态 + 插件 + 主题。追加进 extensions 一次即可。 */
export const livePreviewExtension: Extension[] = [
  ...livePreviewCore,
  livePreviewTheme,
];
