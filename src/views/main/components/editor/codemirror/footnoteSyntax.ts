/**
 * 脚注语法扩展（生产化版本）。
 *
 * 基于 spike `spike/ftag-syntax/src/footnoteSyntax.ts` 生产化，关键改动：
 *  1. **移除模块级 `footnoteDefIdTable` / `footnoteRefIdTable` side-table**
 *     id 提取改用树外按需解析（调用方从原文 `doc.sliceString(node.from, node.to)`
 *     重新正则提取 `[^\]]+`）。
 *  2. **多段落续行判定改用 `line.indent - line.baseIndent >= 4`**
 *     替换 spike 的 top-level `isContinuation`（仅识别行首 4 空格/tab），
 *     支持 blockquote/list 等 composite 上下文内的相对缩进续行。
 *
 * 识别：
 *  - `[^id]: ...` 定义块（FootnoteDefinition），含多段落缩进续行
 *  - `[^id]` 行内引用（FootnoteRef）
 */

import {
  MarkdownConfig,
  BlockContext,
  Line,
  BlockParser,
  InlineParser,
  InlineContext,
} from '@lezer/markdown';

/**
 * 续行判定：相对缩进 >= 4。
 *
 * spike 用 `isContinuation(text) = /^\t|^    /` 仅识别 top-level 行首缩进，
 * 在 blockquote/list 等 composite 上下文内会失效（baseIndent 已被前缀占用）。
 *
 * 生产改用 `line.indent - line.baseIndent >= 4`：
 *  - `line.indent`：行内下一个非空白字符的列号
 *  - `line.baseIndent`：composite 上下文已处理的 base 缩进（如 blockquote 的 `> ` 前缀）
 *  - 差值即为"内容相对缩进"，>= 4 表示续行
 *
 * 注意：本判定必须使用 `line.indent - line.baseIndent >= 4` 形式（grep 验证）。
 */
function isContinuation(line: Line): boolean {
  return line.indent - line.baseIndent >= 4;
}

// ===== 定义块识别（单行 + 多段落续行）=====

const footnoteDefBlockParser: BlockParser = {
  name: 'FootnoteDefinition',
  before: 'LinkReference', // [^id]: 形似 [ref]:，抢在 LinkReference 之前认领
  parse(cx: BlockContext, line: Line): boolean {
    const rest = line.text.slice(line.pos);
    // 匹配 [^id]: 后紧跟空格或 tab（spec 规范要求冒号后有空格）
    const m = /^\[\^([^\]]+)\]:[\t ]/.exec(rest);
    if (!m) return false;

    const from = cx.lineStart + line.pos;

    // 消费第一行（定义起始行）
    cx.nextLine();
    let to = cx.prevLineEnd(); // 第一行内容末尾（不含 \n）

    // 扫描续行：相对缩进 >= 4 的段落归入同一脚注定义块。
    // 策略：检查当前行（line.text，已由 nextLine 读入）。
    //  - 当前行空：peek 下一行；若缩进，消费"当前空行 + 缩进行"；否则停止。
    //  - 当前行缩进（相对 baseIndent >= 4）：直接消费。
    //  - 当前行非空非缩进：停止，不消费（让出给后续 block parser）。
    while (true) {
      if (line.text.trim() === '') {
        // 当前行空。peek 下一行决定是否续行。
        const peeked = cx.peekLine();
        if (peeked === '' || !isContinuationLine(peeked)) {
          // 下一行空/EOF/非缩进：停止。不消费当前空行（让出给后续 parser）。
          break;
        }
        // 下一行缩进：消费"当前空行 + 缩进行"
        cx.nextLine(); // 消费空行，前进到缩进行
        cx.nextLine(); // 消费缩进行，前进到其后
        to = cx.prevLineEnd(); // 缩进行末尾
        continue;
      }

      if (isContinuation(line)) {
        // 当前行相对缩进 >= 4：直接消费
        cx.nextLine();
        to = cx.prevLineEnd();
        continue;
      }

      // 当前行非空非缩进：停止，不消费
      break;
    }

    cx.addElement(cx.elt('FootnoteDefinition', from, to, []));
    return true;
  },
};

/**
 * 从 peekLine 返回的纯文本判断是否为续行。
 *
 * `peekLine()` 返回下一行文本（不含 \n），但无 Line 对象的 baseIndent/indent 信息。
 * 对于 top-level 上下文，等价于行首 4 空格/tab。对于 composite 上下文，peekLine
 * 返回的是已剥离前缀的行文本（@lezer/markdown 内部处理），所以行首缩进即相对缩进。
 *
 * 此函数仅用于"当前空行 + peek 下一行"决策；非空行的续行判定走 `isContinuation(line)`。
 */
function isContinuationLine(text: string): boolean {
  return /^\t|^ {4,}/.test(text);
}

// ===== 行内引用识别 =====

const footnoteRefInlineParser: InlineParser = {
  name: 'FootnoteRef',
  before: 'Link', // [^id] 形似 [text]，抢在标准 Link parser 之前认领
  parse(cx: InlineContext, next: number, pos: number): number {
    if (next !== 91 /* '[' */) return -1;
    if (cx.char(pos + 1) !== 94 /* '^' */) return -1;
    // 找闭合 ]，不允许跨行
    let end = pos + 2;
    while (end < cx.end) {
      const c = cx.char(end);
      if (c === 93 /* ']' */) break;
      if (c === 10 /* \n */ || c === 13 /* \r */) return -1;
      end++;
    }
    if (end >= cx.end || cx.char(end) !== 93) return -1;
    const id = cx.slice(pos + 2, end);
    if (!id) return -1; // [^] 空 id 不认领
    // id 提取改用树外按需解析：调用方从 doc.sliceString(node.from, node.to) 重新提取
    // 区间仅覆盖 [^id] 本身（pos 到 end+1，end 是 ] 的位置）
    return cx.addElement(cx.elt('FootnoteRef', pos, end + 1));
  },
};

/**
 * 脚注语法扩展。装配方式：`markdown({ extensions: [footnoteExtension] })`。
 */
export const footnoteExtension: MarkdownConfig = {
  defineNodes: [
    { name: 'FootnoteDefinition', block: true },
    { name: 'FootnoteRef' }, // 行内节点，非 block
  ],
  parseBlock: [footnoteDefBlockParser],
  parseInline: [footnoteRefInlineParser],
};

// ===== 单元测试（守卫：仅在 vitest 环境运行，生产构建被 vite.config.ts define 树摇）=====

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

  interface ParseResult {
    doc: ReturnType<typeof EditorState.create>['doc'];
    nodes: NodeRange[];
  }

  function parseDoc(md: string): ParseResult {
    const state = EditorState.create({
      doc: md,
      extensions: [markdown({ extensions: [footnoteExtension] })],
    });
    const nodes: NodeRange[] = [];
    syntaxTree(state).iterate({
      enter(node) {
        nodes.push({ name: node.name, from: node.from, to: node.to });
      },
    });
    return { doc: state.doc, nodes };
  }

  function findFirst(
    nodes: readonly NodeRange[],
    name: string,
  ): NodeRange | undefined {
    return nodes.find((n) => n.name === name);
  }

  describe('footnoteSyntax: 单行定义识别', () => {
    it('[^id]: 定义块识别', () => {
      const md = `[^1]: 单行脚注。`;
      const { doc, nodes } = parseDoc(md);
      const def = findFirst(nodes, 'FootnoteDefinition');
      expect(def).toBeDefined();
      expect(doc.sliceString(def!.from, def!.to)).toBe(md);
    });

    it('id 含字母与数字', () => {
      const md = `[^note42]: 脚注内容。`;
      const { nodes } = parseDoc(md);
      expect(findFirst(nodes, 'FootnoteDefinition')).toBeDefined();
    });

    it('冒号后必须有空格（无空格不认领）', () => {
      const md = `[^1]:无空格`;
      const { nodes } = parseDoc(md);
      expect(findFirst(nodes, 'FootnoteDefinition')).toBeUndefined();
    });
  });

  describe('footnoteSyntax: 多段落定义识别', () => {
    it('4 空格缩进续行归入同一脚注定义', () => {
      const md = `[^1]: 第一段。

    第二段缩进续行。`;
      const { doc, nodes } = parseDoc(md);
      const def = findFirst(nodes, 'FootnoteDefinition');
      expect(def).toBeDefined();
      // 区间应覆盖到第二段末尾
      expect(doc.sliceString(def!.from, def!.to)).toContain('第一段');
      expect(doc.sliceString(def!.from, def!.to)).toContain('第二段缩进续行');
      // 不应存在独立 Paragraph 节点表示第二段（它归入 FootnoteDefinition）
      const paragraphs = nodes.filter((n) => n.name === 'Paragraph');
      expect(paragraphs.length).toBe(0);
    });

    it('tab 缩进续行也归入同一脚注定义', () => {
      const md = `[^1]: 第一段。\n\t第二段 tab 续行。`;
      const { doc, nodes } = parseDoc(md);
      const def = findFirst(nodes, 'FootnoteDefinition');
      expect(def).toBeDefined();
      expect(doc.sliceString(def!.from, def!.to)).toContain('tab 续行');
    });

    it('非缩进行终止脚注定义', () => {
      const md = `[^1]: 脚注内容。
后续段落（无缩进）。`;
      const { nodes } = parseDoc(md);
      const def = findFirst(nodes, 'FootnoteDefinition');
      expect(def).toBeDefined();
      // 后续应被识别为独立 Paragraph
      const paragraphs = nodes.filter((n) => n.name === 'Paragraph');
      expect(paragraphs.length).toBe(1);
    });
  });

  describe('footnoteSyntax: blockquote 内续行', () => {
    it('blockquote 内 4 相对缩进续行归入脚注定义', () => {
      // blockquote 内：> [^1]: ...
      //          续行：>     ...（> 后 4 空格，相对 baseIndent 缩进 4）
      const md = `> [^1]: blockquote 内脚注。
>     续行（相对缩进 4）。`;
      const { doc, nodes } = parseDoc(md);
      const def = findFirst(nodes, 'FootnoteDefinition');
      expect(def).toBeDefined();
      expect(doc.sliceString(def!.from, def!.to)).toContain('blockquote 内脚注');
      expect(doc.sliceString(def!.from, def!.to)).toContain('续行');
    });
  });

  describe('footnoteSyntax: 行内引用识别', () => {
    it('正文中 [^id] 识别为 FootnoteRef', () => {
      const md = `正文[^1]继续`;
      const { doc, nodes } = parseDoc(md);
      const ref = findFirst(nodes, 'FootnoteRef');
      expect(ref).toBeDefined();
      expect(doc.sliceString(ref!.from, ref!.to)).toBe('[^1]');
    });

    it('空 [^] 不识别', () => {
      const md = `正文[^]继续`;
      const { nodes } = parseDoc(md);
      expect(findFirst(nodes, 'FootnoteRef')).toBeUndefined();
    });

    it('跨行 [^id\n] 不识别', () => {
      const md = `正文[^1\n]继续`;
      const { nodes } = parseDoc(md);
      expect(findFirst(nodes, 'FootnoteRef')).toBeUndefined();
    });

    it('多个行内引用各自识别', () => {
      const md = `第一[^1]第二[^2]第三`;
      const { nodes } = parseDoc(md);
      const refs = nodes.filter((n) => n.name === 'FootnoteRef');
      expect(refs.length).toBe(2);
    });
  });

  describe('footnoteSyntax: id 树外提取', () => {
    it('从 FootnoteDefinition 节点原文提取 id', () => {
      const md = `[^myNote]: 定义内容。`;
      const { doc, nodes } = parseDoc(md);
      const def = findFirst(nodes, 'FootnoteDefinition');
      expect(def).toBeDefined();
      const text = doc.sliceString(def!.from, def!.to);
      const m = /^\[\^([^\]]+)\]:/.exec(text);
      expect(m).not.toBeNull();
      expect(m![1]).toBe('myNote');
    });

    it('从 FootnoteRef 节点原文提取 id', () => {
      const md = `正文[^ref42]继续`;
      const { doc, nodes } = parseDoc(md);
      const ref = findFirst(nodes, 'FootnoteRef');
      expect(ref).toBeDefined();
      const text = doc.sliceString(ref!.from, ref!.to);
      const m = /^\[\^([^\]]+)\]$/.exec(text);
      expect(m).not.toBeNull();
      expect(m![1]).toBe('ref42');
    });
  });
}
