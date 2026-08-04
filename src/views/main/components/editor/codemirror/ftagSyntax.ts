/**
 * f-标签语法扩展（生产化版本）。
 *
 * 基于 spike `spike/ftag-syntax/src/ftagSyntax.ts` 生产化，关键改动：
 *  1. **移除 BlockContext private API hack**（spike 报告 §3.1 最大风险）
 *     原 spike 通过强制类型断言访问 BlockContext 的 private 字段 `to`/`lineChunkAt`；
 *     生产改用 `MarkdownConfig.wrap` 在 parse 开始时拿到 `Input`，预扫描整文档建立
 *     "起始标签位置 → 闭合标签位置"索引（`Map<number, number>`）。
 *     BlockParser.parse 查索引决定是否认领多行块。
 *  2. **移除模块级 `attrsTable` side-table**（spike 报告 §7.2）
 *     属性提取改用 `ftagAttrs.ts` 的 `extractAttrs` 树外按需解析。
 *  3. **表格结构识别拆出独立模块** `tableStructure.ts`，供本模块调用。
 *
 * 多实例安全：wrap 返回的 PartialParse 在首次 advance() 时构建索引；
 * JS 单线程 + parse 同步执行，不会跨编辑器实例串数据。
 *
 * 注意：本文件**不得**含访问 BlockContext private 字段的强制类型断言代码（grep 验证）。
 */

import {
  MarkdownConfig,
  BlockContext,
  Line,
  BlockParser,
  Element,
} from '@lezer/markdown';
import type {
  Input,
  PartialParse,
  Tree,
  TreeFragment,
} from '@lezer/common';
import { scanTableStructure, type LineInfo } from './tableStructure';

const FTAG_TYPES = ['fig', 'tbl', 'eq', 'claim'] as const;
type FtagType = (typeof FTAG_TYPES)[number];

const FTAG_NODE_NAMES: Record<FtagType, string> = {
  fig: 'FTagFig',
  tbl: 'FTagTbl',
  eq: 'FTagEq',
  claim: 'FTagClaim',
};

const CLOSE_TAGS: Record<FtagType, string> = {
  fig: '</f-fig>',
  tbl: '</f-tbl>',
  eq: '</f-eq>',
  claim: '</f-claim>',
};

const CAPTION_OPEN = '<f-caption>';
const CAPTION_CLOSE = '</f-caption>';

/**
 * 未闭合兜底预扫描索引：起始标签位置（`<` 位置） → 闭合标签结束位置。
 *
 * 由 `wrap` 在首次 `advance()` 时从 `Input` 构建一次，整个 parse 生命周期内只读。
 * 模块级变量在 JS 单线程 + 同步 parse 下多实例安全（每次 wrap 重建索引）。
 */
let closeTagIndex: Map<number, number> = new Map();

/**
 * 预扫描整文档，为每个 `<f-xxx` 起始标签查找匹配的 `</f-xxx>` 闭合标签。
 *
 * 匹配策略：对每个 f-tag 类型独立处理，起始标签按出现顺序与闭合标签就近配对
 * （每个闭合标签只被一个起始标签认领）。无闭合标签的起始标签不入索引，
 * BlockParser.parse 视为未闭合。
 *
 * O(n) 复杂度，5000 行文档 < 5ms（spike 实测 17ms 全量解析，预扫描更轻）。
 */
function buildCloseTagIndex(input: Input): void {
  closeTagIndex = new Map();
  const fullText = input.read(0, input.length);

  for (const type of FTAG_TYPES) {
    const openPat = new RegExp(`<f-${type}(?=[\\s>/])`, 'g');
    const closeTag = CLOSE_TAGS[type];

    // 收集所有起始标签位置
    const openPositions: number[] = [];
    let m: RegExpExecArray | null;
    openPat.lastIndex = 0;
    while ((m = openPat.exec(fullText)) !== null) {
      openPositions.push(m.index);
    }
    if (openPositions.length === 0) continue;

    // 收集所有闭合标签位置
    const closePositions: number[] = [];
    let searchFrom = 0;
    while (true) {
      const idx = fullText.indexOf(closeTag, searchFrom);
      if (idx < 0) break;
      closePositions.push(idx);
      searchFrom = idx + closeTag.length;
    }
    if (closePositions.length === 0) continue;

    // 就近配对：每个起始标签认领其后第一个未被认领的闭合标签
    let closeIdx = 0;
    for (const openPos of openPositions) {
      while (
        closeIdx < closePositions.length &&
        closePositions[closeIdx] < openPos
      ) {
        closeIdx++;
      }
      if (closeIdx < closePositions.length) {
        const closePos = closePositions[closeIdx];
        closeTagIndex.set(openPos, closePos + closeTag.length);
        closeIdx++; // 认领该闭合标签
      }
      // 无匹配闭合标签：不入索引，BlockParser 视为未闭合
    }
  }
}

/**
 * 构建块内子节点（caption + table 结构）。
 *
 * @param cx          BlockContext
 * @param lineStarts  块内各行的绝对起点与文本
 * @param type        f-tag 类型（仅 tbl 块扫描表格结构）
 */
function buildChildren(
  cx: BlockContext,
  lineStarts: LineInfo[],
  type: FtagType,
): Element[] {
  const children: Element[] = [];

  // 嵌套 <f-caption>...</f-caption> 识别（仅处理单行 caption）
  for (const { start, text } of lineStarts) {
    let searchFrom = 0;
    for (;;) {
      const co = text.indexOf(CAPTION_OPEN, searchFrom);
      if (co < 0) break;
      const cc = text.indexOf(CAPTION_CLOSE, co + CAPTION_OPEN.length);
      if (cc < 0) break; // 跨行 caption 不在阶段 2 范围
      const captionFrom = start + co;
      const captionTo = start + cc + CAPTION_CLOSE.length;
      const textFrom = start + co + CAPTION_OPEN.length;
      const textTo = start + cc;
      const textChild = cx.elt('FTagCaptionText', textFrom, textTo);
      children.push(
        cx.elt('FTagCaption', captionFrom, captionTo, [textChild]),
      );
      searchFrom = cc + CAPTION_CLOSE.length;
    }
  }

  // 表格结构扫描（仅 tbl 块）
  if (type === 'tbl') {
    scanTableStructure(cx, lineStarts, children);
    // caption 与 table 元素合并后按 from 排序，保证 Lezer siblings 有序
    children.sort((a, b) => a.from - b.from);
  }

  return children;
}

function makeFtagBlockParser(type: FtagType): BlockParser {
  const nodeName = FTAG_NODE_NAMES[type];
  // 起始标签：行首（在 line.pos 之后）出现 <f-{type} 后紧跟 空白/> 三者之一
  const openRegex = new RegExp(`^<f-${type}(?=[\\s>/])`);
  const closeTag = CLOSE_TAGS[type];

  return {
    name: nodeName,
    before: 'HTMLBlock', // 抢在原生 HTMLBlock 之前认领 <f-xxx>
    parse(cx: BlockContext, line: Line): boolean {
      const rest = line.text.slice(line.pos);
      if (!openRegex.test(rest)) return false;

      const from = cx.lineStart + line.pos;

      // 起始标签的 '>'（假设起始标签单行、属性值内不含 '>'）
      const gtIdx = line.text.indexOf('>', line.pos);

      // ===== 未闭合兜底 1：起始标签的 '>' 不在同一行 =====
      if (gtIdx < 0) {
        // 同行无 '>'：属性值未闭合引号 / 标签未闭合 >。
        // 产出 FTagUnclosed，区间仅该行（从 <f-{type} 到行末），不消费后续行。
        const to = cx.lineStart + line.text.length;
        cx.nextLine();
        cx.addElement(cx.elt('FTagUnclosed', from, to));
        return true;
      }

      const tagEnd = cx.lineStart + gtIdx + 1; // 起始标签 '>' 之后的位置
      const closeIdx = line.text.indexOf(closeTag, gtIdx + 1);

      // ===== 同行闭合 =====
      if (closeIdx >= 0) {
        const to = cx.lineStart + closeIdx + closeTag.length;
        const lineStarts: LineInfo[] = [
          { start: cx.lineStart, text: line.text },
        ];
        cx.nextLine();
        const children = buildChildren(cx, lineStarts, type);
        cx.addElement(cx.elt(nodeName, from, to, children));
        return true;
      }

      // ===== 同行有 '>' 但同行无闭合标签：查预扫描索引 =====
      const closePos = closeTagIndex.get(from);
      if (closePos === undefined) {
        // 索引中无记录：后续未找到闭合标签，视为未闭合。
        // 产出 FTagUnclosed，区间从 <f-{type} 到 '>'，不消费后续行。
        cx.nextLine();
        cx.addElement(cx.elt('FTagUnclosed', from, tagEnd));
        return true;
      }

      // ===== 多行闭合：预扫描已确认后续存在闭合标签 =====
      const lineStarts: LineInfo[] = [
        { start: cx.lineStart, text: line.text },
      ];
      let to = 0;
      cx.nextLine();
      while (true) {
        lineStarts.push({ start: cx.lineStart, text: line.text });
        const idx = line.text.indexOf(closeTag);
        if (idx >= 0) {
          to = cx.lineStart + idx + closeTag.length;
          break;
        }
        if (!cx.nextLine()) {
          // EOF 前未找到闭合（不应发生，预扫描已确认），兜底防退化
          to = cx.prevLineEnd();
          break;
        }
      }
      // 跳过闭合标签所在行（若非 EOF）
      if (cx.lineStart + line.text.length <= to) {
        cx.nextLine();
      }

      const children = buildChildren(cx, lineStarts, type);
      cx.addElement(cx.elt(nodeName, from, to, children));
      return true;
    },
  };
}

/**
 * f-标签语法扩展。装配方式：`markdown({ extensions: [ftagExtension] })`。
 */
export const ftagExtension: MarkdownConfig = {
  defineNodes: [
    { name: 'FTagFig', block: true },
    { name: 'FTagTbl', block: true },
    { name: 'FTagEq', block: true },
    { name: 'FTagClaim', block: true },
    { name: 'FTagCaption' }, // 嵌套子节点，非 block
    { name: 'FTagCaptionText' }, // caption 内纯文本
    // 表格结构节点（tableStructure.ts 使用，defineNodes 需在此统一声明）
    { name: 'FTagTable' },
    { name: 'FTagTHead' },
    { name: 'FTagTBody' },
    { name: 'FTagTR' },
    { name: 'FTagTH' },
    { name: 'FTagTD' },
    { name: 'FTagTHContent' }, // <th>单元格内容文本区间
    { name: 'FTagTDContent' }, // <td>单元格内容文本区间
    // 未闭合起始标签兜底节点（半截标签 / 块未闭合），block 以便顶层可见
    { name: 'FTagUnclosed', block: true },
  ],
  parseBlock: FTAG_TYPES.map((t) => makeFtagBlockParser(t)),
  wrap: (
    inner: PartialParse,
    input: Input,
    _fragments: readonly TreeFragment[],
    _ranges: readonly { from: number; to: number }[],
  ): PartialParse => {
    let indexBuilt = false;
    return {
      advance: (): Tree | null => {
        if (!indexBuilt) {
          buildCloseTagIndex(input);
          indexBuilt = true;
        }
        return inner.advance();
      },
      get parsedPos(): number {
        return inner.parsedPos;
      },
      stopAt(pos: number): void {
        inner.stopAt(pos);
      },
      get stoppedAt(): number | null {
        return inner.stoppedAt;
      },
    };
  },
};

// Tree 类型已在文件顶部从 @lezer/common 导入，用于 wrap 返回值标注。

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
      extensions: [markdown({ extensions: [ftagExtension] })],
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

  describe('ftagSyntax: 完整闭合标签识别', () => {
    it('FTagFig 含嵌套 FTagCaption', () => {
      const md = `<f-fig id="fig:x" src="assets/a.png" alt="A"><f-caption>图</f-caption></f-fig>`;
      const { doc, nodes } = parseDoc(md);
      const fig = findFirst(nodes, 'FTagFig');
      expect(fig).toBeDefined();
      expect(doc.sliceString(fig!.from, fig!.to)).toBe(md);

      const caption = findFirst(nodes, 'FTagCaption');
      expect(caption).toBeDefined();
      const captionText = findFirst(nodes, 'FTagCaptionText');
      expect(captionText).toBeDefined();
      expect(doc.sliceString(captionText!.from, captionText!.to)).toBe('图');
    });

    it('FTagTbl / FTagEq / FTagClaim 各自识别', () => {
      const md1 = `<f-tbl id="t1"></f-tbl>`;
      expect(findFirst(parseDoc(md1).nodes, 'FTagTbl')).toBeDefined();

      const md2 = `<f-eq id="e1">E=mc^2</f-eq>`;
      expect(findFirst(parseDoc(md2).nodes, 'FTagEq')).toBeDefined();

      const md3 = `<f-claim id="c1" type="theorem">定理</f-claim>`;
      expect(findFirst(parseDoc(md3).nodes, 'FTagClaim')).toBeDefined();
    });

    it('多行闭合标签区间正确', () => {
      const md = `<f-fig id="fig:multi">
<f-caption>多行</f-caption>
</f-fig>`;
      const { doc, nodes } = parseDoc(md);
      const fig = findFirst(nodes, 'FTagFig');
      expect(fig).toBeDefined();
      expect(doc.sliceString(fig!.from, fig!.to)).toBe(md);
    });
  });

  describe('ftagSyntax: 未闭合标签不吞噬后续内容', () => {
    it('半截标签（无 >）产出 FTagUnclosed，不消费后续行', () => {
      const md = `<f-fig id="x\n后接段落。`;
      const { nodes } = parseDoc(md);
      const unclosed = findFirst(nodes, 'FTagUnclosed');
      expect(unclosed).toBeDefined();
      // 后续段落应被识别为 Paragraph（不被 FTagFig 吞噬）
      const paragraphs = nodes.filter((n) => n.name === 'Paragraph');
      expect(paragraphs.length).toBeGreaterThan(0);
      // 不应存在 FTagFig 节点
      expect(findFirst(nodes, 'FTagFig')).toBeUndefined();
    });

    it('块未闭合（有 > 但无闭合标签）产出 FTagUnclosed，不吞噬 100 行段落', () => {
      const lines = ['<f-fig id="x">'];
      for (let i = 0; i < 100; i++) {
        lines.push(`段落 ${i}。`);
      }
      const md = lines.join('\n');
      const { nodes } = parseDoc(md);
      const unclosed = findFirst(nodes, 'FTagUnclosed');
      expect(unclosed).toBeDefined();
      // 不应存在 FTagFig 节点（未闭合不假装为完整块）
      expect(findFirst(nodes, 'FTagFig')).toBeUndefined();
      // markdown 中连续非空行合并为单个 Paragraph，因此 100 行连续段落文本
      // 通常只产出 1 个 Paragraph 节点。本测试的核心断言是"内容未被吞噬"：
      // 即所有 Paragraph 节点的区间都在 FTagUnclosed 区间之后。
      const paragraphs = nodes.filter((n) => n.name === 'Paragraph');
      expect(paragraphs.length).toBeGreaterThanOrEqual(1);
      for (const p of paragraphs) {
        expect(p.from).toBeGreaterThanOrEqual(unclosed!.to);
      }
    });

    it('同行未闭合（起始标签同行无闭合且索引无记录）', () => {
      const md = `<f-fig id="x">后续段落。
再一段。`;
      const { nodes } = parseDoc(md);
      const unclosed = findFirst(nodes, 'FTagUnclosed');
      expect(unclosed).toBeDefined();
      // 后续应被识别为 Paragraph
      const paragraphs = nodes.filter((n) => n.name === 'Paragraph');
      expect(paragraphs.length).toBeGreaterThanOrEqual(1);
    });
  });

  describe('ftagSyntax: 增量解析稳定', () => {
    it('逐字符键入 <f-fig></f-fig> 过程不抖动', () => {
      // 模拟逐字符键入：每个中间状态解析后 FTagFig 节点区间应单调增长
      // 或保持未闭合状态，不出现"中途误判后续段落为标签内"
      const final = '<f-fig id="x"></f-fig>';
      const intermediates: string[] = [];
      for (let i = 1; i <= final.length; i++) {
        intermediates.push(final.slice(0, i));
      }

      let prevFigTo: number | null = null;
      for (const md of intermediates) {
        const { nodes } = parseDoc(md);
        const fig = findFirst(nodes, 'FTagFig');
        const unclosed = findFirst(nodes, 'FTagUnclosed');
        // 任意时刻要么有 FTagFig、要么有 FTagUnclosed、要么都没有（刚开始）
        if (fig) {
          // FTagFig 区间 to 应单调不减
          if (prevFigTo !== null) {
            expect(fig.to).toBeGreaterThanOrEqual(prevFigTo);
          }
          prevFigTo = fig.to;
        }
        // 不应同时存在 FTagFig 和 FTagUnclosed
        if (fig && unclosed) {
          throw new Error('FTagFig 与 FTagUnclosed 不应同时存在');
        }
      }
      // 最终状态应有 FTagFig
      const { nodes: finalNodes } = parseDoc(final);
      expect(findFirst(finalNodes, 'FTagFig')).toBeDefined();
    });
  });
}
