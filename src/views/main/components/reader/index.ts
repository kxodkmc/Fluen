/**
 * 文献阅读器模块导出。
 *
 * @module reader
 */

export { default as ReferenceReader } from './ReferenceReader.vue';
export { default as ReferenceToolbar } from './ReferenceToolbar.vue';
export { default as ReferenceContent } from './ReferenceContent.vue';
export { default as MarksPanel } from './MarksPanel.vue';
export { renderMarkdown, collectImageSrcs } from './MarkdownRenderer';
export type { RenderResult } from './MarkdownRenderer';
export { buildReaderCss } from './readerTheme';
