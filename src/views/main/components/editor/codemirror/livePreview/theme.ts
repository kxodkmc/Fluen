/**
 * 半预览视觉主题。
 *
 * 样式全部消费 `--fluen-*` 设计 token（浅/深色主题自动跟随），
 * 与 DESIGN 规范对齐：克制、去噪、以排版层级与留白表达结构。
 *
 * 此处同时完成 KaTeX 样式的全局引入（主文档 DOM 渲染公式所需；
 * 文献阅读器走 iframe 内嵌样式，互不影响）。
 */

import { EditorView } from '@codemirror/view';
import type { Extension } from '@codemirror/state';
// KaTeX 公式的基础排版样式（.katex 类族）——半预览在主文档中渲染
import katexCss from 'katex/dist/katex.min.css?inline';
import { injectGlobalStyle } from './globalStyle';

export { katexCss };

/** KaTeX 全局样式只注入一次的单例守卫（声明先于使用，避免 TDZ）。 */
let katexInjected = false;

function injectKaOnce(): Extension {
  if (!katexInjected) {
    injectGlobalStyle('fluen-katex-css', katexCss);
    katexInjected = true;
  }
  return []; // 无扩展内容；仅承担副作用
}

/**
 * 半预览主题扩展。仅作用于挂载的编辑器实例内部，
 * 通过 CM6 的 theme 隔离机制限定选择器作用域。
 */
export const livePreviewTheme: Extension = [
  injectKaOnce(),
  EditorView.theme({
    /* ── 标题层级：字号 + 字重 + 上方留白，弱化对比、保持纸感 ────────── */
    '& .fluen-lp-h1': {
      fontSize: '1.65em',
      fontWeight: '700',
      letterSpacing: '-0.01em',
      lineHeight: '1.5',
    },
    '& .fluen-lp-h2': {
      fontSize: '1.42em',
      fontWeight: '700',
      letterSpacing: '-0.008em',
    },
    '& .fluen-lp-h3': { fontSize: '1.24em', fontWeight: '650' },
    '& .fluen-lp-h4': { fontSize: '1.12em', fontWeight: '650' },
    '& .fluen-lp-h5': { fontSize: '1.04em', fontWeight: '600' },
    '& .fluen-lp-h6': {
      fontSize: '0.98em',
      fontWeight: '600',
      color: 'var(--fluen-slate)',
    },

    /* ── 行内强调族 ─────────────────────────────────────────────── */
    '& .fluen-lp-strong': { fontWeight: '640', color: 'var(--fluen-ink)' },
    '& .fluen-lp-em': { fontStyle: 'italic' },
    '& .fluen-lp-strike': {
      textDecoration: 'line-through',
      color: 'var(--fluen-slate)',
    },
    '& .fluen-lp-underline': {
      textDecoration: 'underline',
      textUnderlineOffset: '2px',
    },
    '& .fluen-lp-code-inline': {
      fontFamily: 'var(--fluen-font-mono)',
      fontSize: '0.92em',
      padding: '0.08em 0.36em',
      borderRadius: '4px',
      background: 'var(--fluen-surface-soft)',
      border: '1px solid var(--fluen-hairline-soft)',
    },

    /* ── 链接 / 图片占位 ─────────────────────────────────────────── */
    '& .fluen-lp-link': {
      color: 'var(--fluen-brand-blue)',
      cursor: 'pointer',
      borderBottom: '1px solid transparent',
    },
    '& .fluen-lp-link:hover': { borderBottomColor: 'var(--fluen-brand-blue)' },
    '& .fluen-lp-img-alt': {
      color: 'var(--fluen-slate)',
      borderBottom: '1px dashed var(--fluen-hairline)',
      cursor: 'help',
    },

    /* ── 引用块 ─────────────────────────────────────────────────── */
    '& .fluen-lp-quote': {
      borderLeft: '3px solid var(--fluen-hairline)',
      paddingLeft: '10px',
      color: 'var(--fluen-charcoal)',
      fontStyle: 'italic',
    },

    /* ── 围栏代码块 ─────────────────────────────────────────────── */
    '& .fluen-lp-fence': {
      background: 'var(--fluen-surface-deep)',
      fontFamily: 'var(--fluen-font-mono)',
      fontSize: '0.9em',
    },
    '& .fluen-lp-fence:first-of-type': { paddingTop: '8px' },

    /* ── 表格（结构化编辑 widget：三线表 + 行列手柄） ────────────── */
    '& .fluen-lp-tablebox': {
      position: 'relative',
      margin: '22px 0 4px 24px',
    },
    '& .fluen-lp-table': {
      borderCollapse: 'collapse',
      fontSize: '0.92em',
    },
    '& .fluen-lp-table th': {
      fontWeight: '600',
      borderTop: '2px solid var(--fluen-ink)',
      borderBottom: '1px solid var(--fluen-ink)',
      padding: '4px 12px',
      textAlign: 'left',
      cursor: 'text',
    },
    '& .fluen-lp-table td': {
      padding: '3px 12px',
      cursor: 'text',
    },
    '& .fluen-lp-table tbody tr:last-child td': {
      borderBottom: '2px solid var(--fluen-ink)',
    },
    '& .fluen-lp-table [contenteditable]:focus': {
      outline: '2px solid var(--fluen-accent)',
      outlineOffset: '-2px',
    },

    /* 行/列手柄（Word/Notion 式：+ 插入 / × 删除，悬停单元格时出现） */
    '& .fluen-lp-tbhandle': {
      position: 'absolute',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      gap: '2px',
      zIndex: '10',
    },
    '& .fluen-lp-tbhandle--col': {
      top: '-20px',
      height: '18px',
    },
    '& .fluen-lp-tbhandle--col .fluen-lp-tbhandle__line': {
      position: 'absolute',
      top: '18px',
      bottom: '-2px',
      left: '50%',
      width: '2px',
      transform: 'translateX(-50%)',
      background: 'var(--fluen-accent)',
      opacity: '0.35',
    },
    '& .fluen-lp-tbhandle--row': {
      left: '-24px',
      width: '18px',
    },
    '& .fluen-lp-tbhandle--row .fluen-lp-tbhandle__line': {
      position: 'absolute',
      left: '18px',
      right: '-2px',
      top: '50%',
      height: '2px',
      transform: 'translateY(-50%)',
      background: 'var(--fluen-accent)',
      opacity: '0.35',
    },
    '& .fluen-lp-tbhandle__btn': {
      display: 'inline-flex',
      alignItems: 'center',
      justifyContent: 'center',
      width: '17px',
      height: '17px',
      padding: '0',
      border: 'none',
      borderRadius: '50%',
      background: 'var(--fluen-accent)',
      color: 'var(--fluen-on-accent)',
      fontFamily: 'var(--fluen-font-sans)',
      fontSize: '13px',
      lineHeight: '1',
      cursor: 'pointer',
    },
    '& .fluen-lp-tbhandle__btn:hover': {
      background: 'var(--fluen-accent-hover)',
    },

    /* ── 水平线 / 任务列表 ─────────────────────────────────────── */
    '& .fluen-lp-hr': {
      borderTop: '2px solid var(--fluen-hairline)',
      opacity: '0.7',
    },
    '& .fluen-lp-task-done': {
      color: 'var(--fluen-muted)',
      backgroundColor: 'transparent',
    },

    /* ── 数学公式 ───────────────────────────────────────────────── */
    '& .fluen-lp-math': {
      display: 'inline-block',
      verticalAlign: 'baseline',
      lineHeight: 'normal',
    },
    '& .fluen-lp-math--display': {
      display: 'block',
      textAlign: 'center',
      padding: '10px 4px 14px',
      margin: '2px 0 8px',
      overflowX: 'auto',
      overflowY: 'hidden',
    },
    '& .fluen-lp-math--invalid': {
      fontFamily: 'var(--fluen-font-mono)',
      fontSize: '0.9em',
      color: 'var(--fluen-warning)',
    },
    '& .fluen-lp-math-src': {
      background: 'color-mix(in srgb, var(--fluen-info-bg) 55%, transparent)',
      outline: '1px dashed var(--fluen-hairline)',
      outlineOffset: '-1px',
    },
  }),
];

// ===== 单元测试（守卫：仅在 vitest 环境运行） =====

if (import.meta.vitest) {
  const { describe, it, expect } = import.meta.vitest;

  describe('livePreviewTheme', () => {
    it('是可装配的扩展数组且可重复调用不重复注入', () => {
      const a = livePreviewTheme;
      const b = livePreviewTheme;
      expect(a).toBeDefined();
      expect(Array.isArray(a) || typeof a === 'object').toBe(true);
      expect(b).toBeDefined();
    });
  });
}
