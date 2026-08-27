/**
 * 数学行内语法扩展：`$E=mc^2$`。
 *
 * 设计遵循本项目已验证的语法扩展模式（见 footnoteSyntax.ts）：
 *   - 字符级线性扫描识别，不写嵌套正则（防灾难回溯）
 *   - 树外按需提取内容（装饰层从原文 slice，不在解析期缓存）
 *   - 内嵌 vitest 测试块（import.meta.vitest 守卫）
 *
 * 范围说明：
 *   - 本文件只负责**单美元符行内式** `$...$` 的语法树节点（InlineMath）。
 *     与光标揭示逻辑联动的场景全部经 InlineMath 节点驱动。
 *   - `$$...$$` 显示式（含多行围栏）由 decorations.ts 的行扫描规则处理，
 *     不经过本扩展——避免 BlockParser 任意前行走引入的状态机风险。
 *
 * 识别规则：
 *   - 同一行内闭合；内部非空且首尾无空白；
 *   - 起始符 `\X` 转义由 CommonMark 的 Escape 解析器先行消费（故置于其后再注册，
 *     绝不能 before: 'Escape'），扫描闭合时手动跳过转义对；
 *   - 「首尾无空白」防价格误判：「区间 $5 到 $6 内」因内部含空白不被认领。
 */

import {
  MarkdownConfig,
  InlineParser,
  InlineContext,
} from '@lezer/markdown';

const CHAR_DOLLAR = 36; // $
const CHAR_BACKSLASH = 92; // \
const CHAR_NEWLINE = 10; // \n

/** 校验数学内容：非空且首尾无空白。 */
export function isValidMathInterior(content: string): boolean {
  return content.length > 0 && content.trimStart() === content && content.trimEnd() === content;
}

/**
 * 从 `pos` 起在同一行内寻找下一个未转义的 `$`。返回其位置或 -1。
 * 线性扫描：遇反斜杠成对跳过（\$ 不作为闭合），遇换行终止。
 */
export function findClosingDollar(cx: InlineContext, pos: number, end: number): number {
  let i = pos;
  while (i < end) {
    const c = cx.char(i);
    if (c === CHAR_BACKSLASH && i + 1 < end) {
      i += 2; // 转义对整体跳过
      continue;
    }
    if (c === CHAR_NEWLINE) return -1; // 行内式不跨行
    if (c === CHAR_DOLLAR) return i;
    i++;
  }
  return -1;
}

/**
 * 尝试匹配一个完整行内式。
 * 返回元素区间 [元素起, 元素止)，或 null 表示不认领。
 */
function matchInlineMath(
  cx: InlineContext,
  pos: number,
): readonly [number, number] | null {
  const end = cx.end;
  const innerFrom = pos + 1;
  if (innerFrom >= end) return null;

  // 内容首字符是空白直接拒绝（不允许 `$ x$` 宽松形态）
  const first = cx.char(innerFrom);
  if (first === CHAR_NEWLINE || first === 32 || first === 9) return null;

  const closePos = findClosingDollar(cx, innerFrom, end);
  if (closePos === -1) return null;

  if (!isValidMathInterior(cx.slice(innerFrom, closePos))) return null;
  return [pos, closePos + 1];
}

const mathInlineParser: InlineParser = {
  name: 'FluenInlineMath',
  // 置于 Emphasis 等标准解析器之前认领；但绝不能抢在 Escape 之前——
  // Escape 先消费 \$ 才能让转义美元符不触发起始判断
  before: 'Emphasis',
  parse(cx: InlineContext, _next: number, pos: number): number {
    if (cx.char(pos) !== CHAR_DOLLAR) return -1;
    const span = matchInlineMath(cx, pos);
    if (!span) return -1;
    return cx.addElement(cx.elt('InlineMath', span[0], span[1]));
  },
};

/**
 * 数学行内语法扩展。装配方式：`markdown({ extensions: [fluenMathExtension] })`。
 */
export const fluenMathExtension: MarkdownConfig = {
  defineNodes: [{ name: 'InlineMath' }],
  parseInline: [mathInlineParser],
};

// ===== 单元测试（守卫：仅在 vitest 环境运行，生产构建被 define 树摇）=====

if (import.meta.vitest) {
  const { describe, it, expect } = import.meta.vitest;
  const { EditorState } = await import('@codemirror/state');
  const { syntaxTree } = await import('@codemirror/language');
  const { markdown } = await import('@codemirror/lang-markdown');

  interface NodeRange {
    name: string;
    from: number;
    to: number;
  }

  function parseDoc(md: string): { md: string; nodes: NodeRange[] } {
    const state = EditorState.create({
      doc: md,
      extensions: [markdown({ extensions: [fluenMathExtension] })],
    });
    const nodes: NodeRange[] = [];
    syntaxTree(state).iterate({
      enter(node) {
        nodes.push({ name: node.name, from: node.from, to: node.to });
      },
    });
    return { md, nodes };
  }

  describe('mathSyntax: 行内式 $...$', () => {
    it('基本识别', () => {
      const md = '质能方程 $E=mc^2$ 很有名。';
      const { md: src, nodes } = parseDoc(md);
      const m = nodes.find((n) => n.name === 'InlineMath');
      expect(m).toBeDefined();
      expect(src.slice(m!.from, m!.to)).toBe('$E=mc^2$');
    });

    it('多个行内式各自识别', () => {
      const { md: src, nodes } = parseDoc('$a+b$ 与 $c^2$');
      const maths = nodes.filter((n) => n.name === 'InlineMath');
      expect(maths.length).toBe(2);
      expect(src.slice(maths[0].from, maths[0].to)).toBe('$a+b$');
      expect(src.slice(maths[1].from, maths[1].to)).toBe('$c^2$');
    });

    it('含嵌套方括号/花括号的 LaTeX', () => {
      const md = '$\\frac{a}{b}_{n}$';
      const { md: src, nodes } = parseDoc(md);
      const m = nodes.find((n) => n.name === 'InlineMath');
      expect(m).toBeDefined();
      expect(src.slice(m!.from, m!.to)).toBe(md);
    });

    it('内部首尾空白不识别（防价格误判）', () => {
      const { nodes } = parseDoc('区间 $5 到 $6 内');
      expect(nodes.some((n) => n.name === 'InlineMath')).toBe(false);
    });

    it('空式不识别', () => {
      const { nodes } = parseDoc('a $ b');
      expect(nodes.some((n) => n.name === 'InlineMath')).toBe(false);
    });

    it('不跨行匹配', () => {
      const { nodes } = parseDoc('$a\nb$');
      expect(nodes.some((n) => n.name === 'InlineMath')).toBe(false);
    });

    it('转义 \\$ 不触发', () => {
      const { nodes } = parseDoc('成本 \\$5 以上');
      expect(nodes.some((n) => n.name === 'InlineMath')).toBe(false);
    });

    it('孤立 $ 不受影响', () => {
      const { nodes } = parseDoc('一段没有数学的文本 $ 。');
      expect(nodes.some((n) => n.name === 'InlineMath')).toBe(false);
    });
  });
}
