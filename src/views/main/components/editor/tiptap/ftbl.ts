/**
 * TipTap 题注表格节点 — `<f-tbl>` 规范块（规范 §5.3 / §6.2）。
 *
 * 存储规范（与 codemirror/tableModel.ts 的 buildTableBlock 一致）：
 *
 * ```
 * <f-tbl>
 *   <f-caption>表格题注</f-caption>
 *
 * | 列A | 列B |
 * | --- | --- |
 * | 1   | 2   |
 *
 * </f-tbl>
 * ```
 *
 * 结构：`ftbl`（包裹节点）= `tableCaption`（题注，可编辑文本）+ `table`
 * （TipTap 原生表格，单元格可直接编辑）。序列化时按上述规范还原，与
 * CM6 视图生成的格式严格一致，linter 的「恰为 1 个 f-caption」校验通过。
 */

import { Node, mergeAttributes } from '@tiptap/core';
import type { MarkdownToken } from '@tiptap/core';

/** 整块匹配：`<f-tbl>` 行 … `</f-tbl>` 行（内部为题注 + 空行 + GFM 表格）。 */
const FTBL_RE = /^[ \t]*<f-tbl>[ \t]*\r?\n([\s\S]*?)\r?\n[ \t]*<\/f-tbl>/;
const CAPTION_RE = /^[ \t]*<f-caption>([\s\S]*?)<\/f-caption>[ \t]*$/;

/** 题注节点（表格上方居中的说明文本，规范要求每表恰一个）。 */
export const TableCaption = Node.create({
  name: 'tableCaption',
  group: 'block',
  content: 'inline*',
  defining: true,

  parseHTML() {
    return [{ tag: 'div[data-fluen-caption]' }];
  },

  renderHTML() {
    return ['div', mergeAttributes({ 'data-fluen-caption': '', class: 'fluen-table-caption' }), 0];
  },

  markdownTokenName: 'fluenTableCaption',

  parseMarkdown: (token, helpers) => ({
    type: 'tableCaption',
    content: helpers.parseInline(token.tokens ?? []),
  }),

  renderMarkdown: (node, helpers) => `<f-caption>${helpers.renderChildren(node.content ?? [])}</f-caption>`,
});

/** f-tbl 包裹节点：题注 + 原生表格。 */
export const Ftbl = Node.create({
  name: 'ftbl',
  group: 'block',
  content: 'tableCaption table',
  defining: true,

  parseHTML() {
    return [{ tag: 'f-tbl' }];
  },

  renderHTML() {
    return ['div', mergeAttributes({ 'data-fluen-ftbl': '', class: 'fluen-ftbl' }), 0];
  },

  markdownTokenName: 'fluenFtbl',

  parseMarkdown: (token, helpers) => ({
    type: 'ftbl',
    content: helpers.parseChildren(token.tokens ?? []),
  }),

  renderMarkdown: (node, helpers) => {
    const caption = (node.content ?? []).find((c) => c.type === 'tableCaption');
    const table = (node.content ?? []).find((c) => c.type === 'table');
    const capMd = caption ? helpers.renderChildren([caption]) : '';
    const tableMd = table ? helpers.renderChildren([table]) : '';
    return `<f-tbl>\n  ${capMd}\n\n${tableMd}\n\n</f-tbl>`;
  },

  markdownTokenizer: {
    name: 'fluenFtbl',
    level: 'block' as const,
    start: (src: string) => src.indexOf('<f-tbl>'),
    tokenize(src: string, _tokens: MarkdownToken[], lexer) {
      const m = FTBL_RE.exec(src);
      if (!m) return undefined;
      const inner = m[1];
      const tokens: MarkdownToken[] = [];
      const rest = inner.replace(/^[ \t]*<f-caption>[\s\S]*?<\/f-caption>[ \t]*$/m, (capLine) => {
        const cap = CAPTION_RE.exec(capLine);
        if (cap) {
          tokens.push({
            type: 'fluenTableCaption',
            raw: capLine,
            text: cap[1],
            tokens: lexer.inlineTokens(cap[1]),
          } as MarkdownToken);
          return '';
        }
        return capLine;
      });
      tokens.push(...lexer.blockTokens(rest));
      return { type: 'fluenFtbl', raw: m[0], tokens } as MarkdownToken;
    },
  },
});
