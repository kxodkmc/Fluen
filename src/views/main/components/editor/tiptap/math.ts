/**
 * TipTap 数学公式节点 — 行内/块级 KaTeX 渲染 + 自定义 markdown tokenizer。
 *
 * - 解析：`$...$`（行内）/ `$$...$$`（块级）经 marked 自定义 tokenizer 进入
 *   节点（`latex` 属性保存定界符之间的源文本）
 * - 渲染：NodeView 调用 `katexMathHtml`（trust:false，失败回退转义原文，见
 *   codemirror/livePreview/katexRender.ts 的安全约定）
 * - 序列化：还原为 `$latex$` / `$$\nlatex\n$$`，往返无损（替代早期字面文本
 *   方案的转义缺陷）
 *
 * 交互约定：点击选中节点（NodeSelection），工具栏公式面板的符号/结构插入
 * 会追加到选中节点的 latex（见 tiptap/commands.ts）。
 */

import { Node, mergeAttributes } from '@tiptap/core';
import { katexMathHtml } from '../codemirror/livePreview/katexRender';

/** 块级公式节点（`$$...$$`，独立成段，KaTeX displayMode 排版）。 */
export const BlockMath = Node.create({
  name: 'blockMath',
  group: 'block',
  atom: true,
  selectable: true,

  addAttributes() {
    return {
      latex: { default: '' },
    };
  },

  parseHTML() {
    return [{ tag: 'div[data-fluen-math-block]' }];
  },

  renderHTML({ node }) {
    return ['div', mergeAttributes({ 'data-fluen-math-block': '', 'data-latex': node.attrs.latex })];
  },

  markdownTokenName: 'fluenBlockMath',

  parseMarkdown: (token) => ({
    type: 'blockMath',
    attrs: { latex: String(token.text ?? '').trim() },
  }),

  renderMarkdown: (node) => `$$\n${node.attrs?.latex ?? ''}\n$$`,

  markdownTokenizer: {
    name: 'fluenBlockMath',
    level: 'block' as const,
    start: (src: string) => src.indexOf('$$'),
    tokenize(src: string) {
      const m = /^\$\$([ \t]*\r?\n)?([\s\S]+?)\$\$/.exec(src);
      if (!m) return undefined;
      return { type: 'fluenBlockMath', raw: m[0], text: m[2] };
    },
  },

  addNodeView() {
    return ({ node, editor, getPos }) => {
      const dom = document.createElement('div');
      dom.classList.add('fluen-math-block');
      const render = (n: typeof node): void => {
        const latex = String(n.attrs.latex ?? '');
        const { html } = katexMathHtml(latex, true);
        dom.innerHTML = html;
        dom.classList.toggle('fluen-math-empty', latex.trim().length === 0);
      };
      render(node);
      // 点击选中节点：工具栏公式面板的符号插入据此追加 latex
      dom.addEventListener('click', () => {
        const pos = getPos();
        if (typeof pos === 'number') {
          editor.chain().setNodeSelection(pos).run();
        }
      });
      return {
        dom,
        update: (n) => {
          if (n.type.name !== 'blockMath') return false;
          node = n;
          render(n);
          return true;
        },
      };
    };
  },
});

/** 行内公式节点（`$...$`，KaTeX 行内排版）。 */
export const InlineMath = Node.create({
  name: 'inlineMath',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,

  addAttributes() {
    return {
      latex: { default: '' },
    };
  },

  parseHTML() {
    return [{ tag: 'span[data-fluen-math-inline]' }];
  },

  renderHTML({ node }) {
    return ['span', mergeAttributes({ 'data-fluen-math-inline': '', 'data-latex': node.attrs.latex })];
  },

  markdownTokenName: 'fluenInlineMath',

  parseMarkdown: (token) => ({
    type: 'inlineMath',
    attrs: { latex: String(token.text ?? '') },
  }),

  renderMarkdown: (node) => `$${node.attrs?.latex ?? ''}$`,

  markdownTokenizer: {
    name: 'fluenInlineMath',
    level: 'inline' as const,
    start: (src: string) => src.indexOf('$'),
    tokenize(src: string) {
      const m = /^\$([^$\n]+?)\$/.exec(src);
      if (!m) return undefined;
      return { type: 'fluenInlineMath', raw: m[0], text: m[1] };
    },
  },

  addNodeView() {
    return ({ node, editor, getPos }) => {
      const dom = document.createElement('span');
      dom.classList.add('fluen-math-inline');
      const render = (n: typeof node): void => {
        const latex = String(n.attrs.latex ?? '');
        const { html } = katexMathHtml(latex, false);
        dom.innerHTML = html;
        dom.classList.toggle('fluen-math-empty', latex.trim().length === 0);
      };
      render(node);
      dom.addEventListener('click', () => {
        const pos = getPos();
        if (typeof pos === 'number') {
          editor.chain().setNodeSelection(pos).run();
        }
      });
      return {
        dom,
        update: (n) => {
          if (n.type.name !== 'inlineMath') return false;
          node = n;
          render(n);
          return true;
        },
      };
    };
  },
});
