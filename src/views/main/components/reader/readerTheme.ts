/**
 * 阅读器主题 CSS 生成器。
 *
 * 根据 ReaderOptions 生成完整 CSS，注入到 iframe 的 <style> 中。
 * 深色/浅色模式通过 CSS 变量切换，遵循 DESIGN.md 设计令牌。
 *
 * @module reader/readerTheme
 */

import type { ReaderOptions } from '../../../../types/reader';

/**
 * 生成阅读器完整 CSS。
 *
 * @param options 主题模式、字号、行高
 * @returns 完整 CSS 字符串（含 :root 变量 + .fluen-reader 正文样式）
 */
export function buildReaderCss(options: ReaderOptions): string {
  const isDark = options.themeMode === 'dark';
  const palette = isDark ? DARK_PALETTE : LIGHT_PALETTE;

  return `
:root {
  --reader-font-size: ${options.fontSize}px;
  --reader-line-height: ${options.lineHeight};
  --reader-text: ${palette.text};
  --reader-text-muted: ${palette.textMuted};
  --reader-bg: ${palette.bg};
  --reader-bg-alt: ${palette.bgAlt};
  --reader-border: ${palette.border};
  --reader-link: ${palette.link};
  --reader-code-bg: ${palette.codeBg};
  --reader-code-text: ${palette.codeText};
  --reader-quote-border: ${palette.quoteBorder};
  --reader-quote-bg: ${palette.quoteBg};
  --reader-table-header-bg: ${palette.tableHeaderBg};
  --reader-img-failed-bg: ${palette.imgFailedBg};
  --reader-img-failed-border: ${palette.imgFailedBorder};
  --reader-img-failed-text: ${palette.imgFailedText};
  --reader-selection-bg: ${palette.selectionBg};
}

* { box-sizing: border-box; }

html, body {
  margin: 0;
  padding: 0;
  background: var(--reader-bg);
  color: var(--reader-text);
  font-family: 'DM Sans', 'PingFang SC', 'Microsoft YaHei', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  font-size: var(--reader-font-size);
  line-height: var(--reader-line-height);
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

.fluen-reader {
  max-width: 820px;
  margin: 0 auto;
  padding: 48px 32px 120px;
}

/* ── 标题 ── */
.fluen-reader h1,
.fluen-reader h2,
.fluen-reader h3,
.fluen-reader h4,
.fluen-reader h5,
.fluen-reader h6 {
  margin: 1.8em 0 0.6em;
  font-weight: 600;
  line-height: 1.3;
  color: var(--reader-text);
}
.fluen-reader h1 { font-size: 1.9em; margin-top: 0; }
.fluen-reader h2 { font-size: 1.55em; border-bottom: 1px solid var(--reader-border); padding-bottom: 0.3em; }
.fluen-reader h3 { font-size: 1.3em; }
.fluen-reader h4 { font-size: 1.15em; }
.fluen-reader h5,
.fluen-reader h6 { font-size: 1em; color: var(--reader-text-muted); }

/* ── 段落 ── */
.fluen-reader p {
  margin: 0 0 1em;
  text-align: justify;
  word-break: break-word;
  overflow-wrap: break-word;
}

/* ── 链接 ── */
.fluen-reader a {
  color: var(--reader-link);
  text-decoration: none;
  border-bottom: 1px solid transparent;
  transition: border-color 0.15s;
}
.fluen-reader a:hover { border-bottom-color: var(--reader-link); }

/* ── 强调 ── */
.fluen-reader strong { font-weight: 600; }
.fluen-reader em { font-style: italic; }
.fluen-reader del { color: var(--reader-text-muted); }

/* ── 列表 ── */
.fluen-reader ul,
.fluen-reader ol {
  margin: 0 0 1em;
  padding-left: 1.8em;
}
.fluen-reader li { margin: 0.25em 0; }
.fluen-reader li > ul,
.fluen-reader li > ol { margin: 0.25em 0; }

/* ── 代码 ── */
.fluen-reader code {
  font-family: 'JetBrains Mono', 'Fira Code', Consolas, 'Courier New', monospace;
  font-size: 0.9em;
  background: var(--reader-code-bg);
  padding: 0.15em 0.4em;
  border-radius: 4px;
}
.fluen-reader pre {
  margin: 0 0 1em;
  padding: 16px 20px;
  background: var(--reader-code-bg);
  border-radius: 8px;
  overflow-x: auto;
  line-height: 1.5;
}
.fluen-reader pre code {
  background: none;
  padding: 0;
  font-size: 0.875em;
}

/* ── 引用 ── */
.fluen-reader blockquote {
  margin: 0 0 1em;
  padding: 8px 20px;
  border-left: 3px solid var(--reader-quote-border);
  background: var(--reader-quote-bg);
  color: var(--reader-text-muted);
  border-radius: 0 6px 6px 0;
}
.fluen-reader blockquote p:last-child { margin-bottom: 0; }

/* ── 分割线 ── */
.fluen-reader hr {
  margin: 2em 0;
  border: none;
  border-top: 1px solid var(--reader-border);
}

/* ── 表格 ── */
.fluen-reader table {
  margin: 0 0 1em;
  border-collapse: collapse;
  width: 100%;
  font-size: 0.95em;
}
.fluen-reader th,
.fluen-reader td {
  padding: 8px 12px;
  border: 1px solid var(--reader-border);
  text-align: left;
}
.fluen-reader th {
  background: var(--reader-table-header-bg);
  font-weight: 600;
}
.fluen-reader tr:nth-child(even) td { background: var(--reader-bg-alt); }

/* ── 图片 ── */
.fluen-reader img {
  max-width: 100%;
  height: auto;
  border-radius: 6px;
  margin: 0.5em 0;
}
.fluen-reader img[data-failed-src] {
  display: block;
  min-height: 56px;
  padding: 12px 16px;
  background: var(--reader-img-failed-bg);
  border: 1px dashed var(--reader-img-failed-border);
  border-radius: 6px;
  color: var(--reader-img-failed-text);
  font-size: 0.85em;
  font-style: italic;
}
.fluen-reader img[data-failed-src]::after {
  content: '⚠ ' attr(data-failed-src);
  display: block;
}

/* ── 选区高亮（为阶段二标记预留）── */
.fluen-reader ::selection {
  background: var(--reader-selection-bg);
}

/* ── data-block-key 元素无默认样式（标记/翻译通过 JS 操作）── */
.fluen-reader [data-block-key] { position: relative; }

/* ── 标记高亮 ── */
.fluen-mark {
  border-radius: 2px;
  padding: 0 1px;
  cursor: pointer;
  transition: box-shadow 0.3s;
}
.fluen-mark--yellow { background: rgba(250, 204, 21, 0.4); }
.fluen-mark--green { background: rgba(34, 197, 94, 0.3); }
.fluen-mark--blue { background: rgba(59, 130, 246, 0.3); }
.fluen-mark--pink { background: rgba(236, 72, 153, 0.3); }

/* ── 滚动条 ── */
::-webkit-scrollbar { width: 10px; height: 10px; }
::-webkit-scrollbar-track { background: var(--reader-bg); }
::-webkit-scrollbar-thumb {
  background: var(--reader-border);
  border-radius: 5px;
}
::-webkit-scrollbar-thumb:hover { background: var(--reader-text-muted); }
`;
}

// ---------------------------------------------------------------------------
// 色板
// ---------------------------------------------------------------------------

interface Palette {
  text: string;
  textMuted: string;
  bg: string;
  bgAlt: string;
  border: string;
  link: string;
  codeBg: string;
  codeText: string;
  quoteBorder: string;
  quoteBg: string;
  tableHeaderBg: string;
  imgFailedBg: string;
  imgFailedBorder: string;
  imgFailedText: string;
  selectionBg: string;
}

const LIGHT_PALETTE: Palette = {
  text: '#1a1a2e',
  textMuted: '#6b7280',
  bg: '#ffffff',
  bgAlt: '#f9fafb',
  border: '#e5e7eb',
  link: '#2563eb',
  codeBg: '#f3f4f6',
  codeText: '#1a1a2e',
  quoteBorder: '#d1d5db',
  quoteBg: '#f9fafb',
  tableHeaderBg: '#f3f4f6',
  imgFailedBg: '#fef2f2',
  imgFailedBorder: '#fecaca',
  imgFailedText: '#b91c1c',
  selectionBg: '#bfdbfe',
};

const DARK_PALETTE: Palette = {
  text: '#e5e7eb',
  textMuted: '#9ca3af',
  bg: '#1a1a2e',
  bgAlt: '#242442',
  border: '#374151',
  link: '#60a5fa',
  codeBg: '#242442',
  codeText: '#e5e7eb',
  quoteBorder: '#4b5563',
  quoteBg: '#242442',
  tableHeaderBg: '#242442',
  imgFailedBg: '#3b1d1d',
  imgFailedBorder: '#7f1d1d',
  imgFailedText: '#fca5a5',
  selectionBg: '#1e3a8a',
};
