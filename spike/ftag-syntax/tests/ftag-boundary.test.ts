// f-标签边界识别 spike 测试 —— 覆盖 spec.md 三个 Scenario
// 路径选型注记：本 spike 优先采用 MarkdownConfig（defineNodes + parseBlock）
// 路径。若改用 @lezer/generator .grammar，本测试文件无需改动——
// 测试只依赖 syntaxTree 的节点结构，与扩展实现路径解耦。

import { describe, it, expect, beforeEach } from 'vitest';
import { EditorState } from '@codemirror/state';
import { syntaxTree } from '@codemirror/language';
import { markdown } from '@codemirror/lang-markdown';
import { Tree } from '@lezer/common';
import { ftagExtension, getFtagAttrs, clearFtagAttrs } from '../src/ftagSyntax';

function parse(md: string): Tree {
  const state = EditorState.create({
    doc: md,
    extensions: [markdown({ extensions: [ftagExtension] })],
  });
  return syntaxTree(state);
}

interface NodeInfo {
  name: string;
  from: number;
  to: number;
}

function collectNodes(tree: Tree): NodeInfo[] {
  const nodes: NodeInfo[] = [];
  tree.iterate({
    enter(node) {
      nodes.push({ name: node.name, from: node.from, to: node.to });
    },
  });
  return nodes;
}

function filterByName(nodes: NodeInfo[], name: string): NodeInfo[] {
  return nodes.filter((n) => n.name === name);
}

beforeEach(() => {
  clearFtagAttrs();
});

describe('Scenario 1: 完整闭合标签识别', () => {
  it('FTagFig 区间精确覆盖 <f-fig>...</f-fig>，且属性可提取', () => {
    const md = `<f-fig id="fig:overview" src="assets/a.png" alt="A"><f-caption>图</f-caption></f-fig>`;
    const tree = parse(md);
    const nodes = collectNodes(tree);

    const figs = filterByName(nodes, 'FTagFig');
    expect(figs.length).toBe(1);

    const fig = figs[0];
    // from/to 精确覆盖从 <f-fig 起始到 </f-fig> 结束（含闭合）
    expect(fig.from).toBe(0);
    expect(fig.to).toBe(md.length);
    expect(md.slice(fig.from, fig.to)).toBe(md);

    // 属性解析
    const attrs = getFtagAttrs(fig.from);
    expect(attrs).toBeDefined();
    expect(attrs?.id).toBe('fig:overview');
    expect(attrs?.src).toBe('assets/a.png');
    expect(attrs?.alt).toBe('A');
  });
});

describe('Scenario 2: 嵌套 caption 边界识别', () => {
  it('FTagFig 包含 FTagCaption 子节点，caption 文本作为独立子节点', () => {
    const md = `<f-fig id="fig:x"><f-caption>这是说明文字</f-caption></f-fig>`;
    const tree = parse(md);
    const nodes = collectNodes(tree);

    const figs = filterByName(nodes, 'FTagFig');
    const captions = filterByName(nodes, 'FTagCaption');
    const captionTexts = filterByName(nodes, 'FTagCaptionText');

    expect(figs.length).toBe(1);
    expect(captions.length).toBe(1);
    expect(captionTexts.length).toBe(1);

    const fig = figs[0];
    const caption = captions[0];
    const captionText = captionTexts[0];

    // 外层 FTagFig 覆盖整个外标签
    expect(md.slice(fig.from, fig.to)).toBe(md);
    expect(fig.from).toBe(0);
    expect(fig.to).toBe(md.length);

    // 内层 FTagCaption 覆盖 <f-caption>...</f-caption>
    expect(md.slice(caption.from, caption.to)).toBe(
      '<f-caption>这是说明文字</f-caption>',
    );

    // caption 文本作为独立子节点可提取
    expect(md.slice(captionText.from, captionText.to)).toBe('这是说明文字');

    // caption 完全落在 fig 区间内
    expect(caption.from).toBeGreaterThanOrEqual(fig.from);
    expect(caption.to).toBeLessThanOrEqual(fig.to);
    expect(captionText.from).toBeGreaterThanOrEqual(caption.from);
    expect(captionText.to).toBeLessThanOrEqual(caption.to);
  });
});

describe('Scenario 3: 四类 f-标签统一识别', () => {
  it('FTagFig/Tbl/Eq/Claim 各产出一个块节点，互不嵌套冲突', () => {
    const md = [
      `<f-eq id="eq:einstein" label="(1)">`,
      `  E = mc^2`,
      `</f-eq>`,
      ``,
      `<f-fig id="fig:overview" src="a.png" alt="A">`,
      `  <f-caption>图说明</f-caption>`,
      `</f-fig>`,
      ``,
      `<f-tbl id="tbl:results">`,
      `  <f-caption>表说明</f-caption>`,
      `</f-tbl>`,
      ``,
      `<f-claim id="claim:th1" type="theorem">`,
      `  定理内容`,
      `</f-claim>`,
    ].join('\n');

    const tree = parse(md);
    const nodes = collectNodes(tree);

    const figs = filterByName(nodes, 'FTagFig');
    const tbls = filterByName(nodes, 'FTagTbl');
    const eqs = filterByName(nodes, 'FTagEq');
    const claims = filterByName(nodes, 'FTagClaim');

    expect(figs.length).toBe(1);
    expect(tbls.length).toBe(1);
    expect(eqs.length).toBe(1);
    expect(claims.length).toBe(1);

    const eq = eqs[0];
    const fig = figs[0];
    const tbl = tbls[0];
    const claim = claims[0];

    // 每个块节点区间精确覆盖各自标签对
    expect(md.slice(eq.from, eq.to)).toBe(
      [`<f-eq id="eq:einstein" label="(1)">`, `  E = mc^2`, `</f-eq>`].join(
        '\n',
      ),
    );
    expect(md.slice(fig.from, fig.to)).toBe(
      [
        `<f-fig id="fig:overview" src="a.png" alt="A">`,
        `  <f-caption>图说明</f-caption>`,
        `</f-fig>`,
      ].join('\n'),
    );
    expect(md.slice(tbl.from, tbl.to)).toBe(
      [`<f-tbl id="tbl:results">`, `  <f-caption>表说明</f-caption>`, `</f-tbl>`].join(
        '\n',
      ),
    );
    expect(md.slice(claim.from, claim.to)).toBe(
      [`<f-claim id="claim:th1" type="theorem">`, `  定理内容`, `</f-claim>`].join(
        '\n',
      ),
    );

    // 块节点之间互不重叠
    const blocks = [eq, fig, tbl, claim].sort((a, b) => a.from - b.from);
    for (let i = 0; i < blocks.length - 1; i++) {
      expect(blocks[i].to).toBeLessThanOrEqual(blocks[i + 1].from);
    }

    // 每类标签属性可提取
    expect(getFtagAttrs(eq.from)?.id).toBe('eq:einstein');
    expect(getFtagAttrs(eq.from)?.label).toBe('(1)');
    expect(getFtagAttrs(fig.from)?.src).toBe('a.png');
    expect(getFtagAttrs(tbl.from)?.id).toBe('tbl:results');
    expect(getFtagAttrs(claim.from)?.type).toBe('theorem');

    // 多行块内的 caption 仍可识别
    const captions = filterByName(nodes, 'FTagCaption');
    expect(captions.length).toBe(2); // fig 与 tbl 各一个
    const captionTexts = filterByName(nodes, 'FTagCaptionText');
    expect(captionTexts.length).toBe(2);
    const captionTextValues = captionTexts
      .map((c) => md.slice(c.from, c.to))
      .sort();
    expect(captionTextValues).toEqual(['图说明', '表说明']);
  });
});
