/**
 * 下划线行内语法扩展：`++文本++`（ins 扩展语法，与 Pandoc / markdown-it-ins 约定一致）。
 *
 * 采用 @lezer/markdown 的 delimiter 机制（与内置 GFM 删除线同构）：
 *   - 标记配对、左右侧翼判定由解析器统一解决，嵌套强调（`**粗 ++下划++ 粗**`）自然工作
 *   - 未配对的孤立 `++` 保持原样显示为纯文本
 *   - `\+` 由内置 Escape 解析器先行消费，不会触发起始判断（解析器仅认领 `+` 字符，
 *     与 Escape 无同字符竞争，无需 before/after 排序）
 *
 * 内嵌 vitest 测试块（import.meta.vitest 守卫）。
 */

import { MarkdownConfig, DelimiterType, InlineContext, InlineParser } from '@lezer/markdown';

const CHAR_PLUS = 43; // +

/** 下划线定界符：解析为 Underline 节点，两侧 `++` 记号为 UnderlineMark。 */
const UnderlineDelim: DelimiterType = { resolve: 'Underline', mark: 'UnderlineMark' };

const underlineParser: InlineParser = {
  name: 'Underline',
  parse(cx: InlineContext, next: number, pos: number): number {
    if (next !== CHAR_PLUS || cx.char(pos + 1) !== CHAR_PLUS) return -1;
    return cx.addDelimiter(UnderlineDelim, pos, pos + 2, true, true);
  },
};

/**
 * 下划线语法扩展。装配方式：`markdown({ extensions: [underlineExtension] })`。
 */
export const underlineExtension: MarkdownConfig = {
  defineNodes: [{ name: 'Underline' }, { name: 'UnderlineMark' }],
  parseInline: [underlineParser],
};

// ===== 单元测试（守卫：仅在 vitest 环境运行，生产构建被 define 树摇）=====

if (import.meta.vitest) {
  const { describe, it, expect } = import.meta.vitest;
  const { EditorState } = await import('@codemirror/state');
  const { syntaxTree } = await import('@codemirror/language');
  const { markdown, markdownLanguage } = await import('@codemirror/lang-markdown');

  interface NodeRange {
    name: string;
    from: number;
    to: number;
  }

  function parseDoc(md: string): { md: string; nodes: NodeRange[] } {
    const state = EditorState.create({
      doc: md,
      // 与生产 setup.ts 一致的方言组合（GFM base + 本扩展），验证共存行为
      extensions: [markdown({ base: markdownLanguage, extensions: [underlineExtension], addKeymap: false })],
    });
    const nodes: NodeRange[] = [];
    syntaxTree(state).iterate({
      enter(node) {
        nodes.push({ name: node.name, from: node.from, to: node.to });
      },
    });
    return { md, nodes };
  }

  describe('underlineSyntax: 行内 ++文本++', () => {
    it('基本识别，节点内含两侧 UnderlineMark', () => {
      const md = '这是 ++下划线++ 文本';
      const { md: src, nodes } = parseDoc(md);
      const u = nodes.find((n) => n.name === 'Underline');
      expect(u).toBeDefined();
      expect(src.slice(u!.from, u!.to)).toBe('++下划线++');
      const marks = nodes.filter((n) => n.name === 'UnderlineMark');
      expect(marks.length).toBe(2);
      expect(src.slice(marks[0].from, marks[0].to)).toBe('++');
    });

    it('嵌套加粗正常解析', () => {
      const md = '++**粗且下划**++';
      const { nodes } = parseDoc(md);
      expect(nodes.some((n) => n.name === 'Underline')).toBe(true);
      expect(nodes.some((n) => n.name === 'StrongEmphasis')).toBe(true);
    });

    it('未配对的孤立 ++ 不识别', () => {
      const { nodes } = parseDoc('a ++ b');
      expect(nodes.some((n) => n.name === 'Underline')).toBe(false);
    });

    it('转义 \\+ 不触发', () => {
      const { nodes } = parseDoc('C\\+\\+ 是语言');
      expect(nodes.some((n) => n.name === 'Underline')).toBe(false);
    });

    it('单侧 ++ 不与 GFM 删除线 / 数学语法冲突', () => {
      const { md: src, nodes } = parseDoc('~~删除~~ 与 ++下划++');
      expect(nodes.some((n) => n.name === 'Strikethrough')).toBe(true);
      const u = nodes.find((n) => n.name === 'Underline');
      expect(src.slice(u!.from, u!.to)).toBe('++下划++');
    });
  });
}
