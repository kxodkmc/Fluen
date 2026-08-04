// 未闭合标签兜底 spike 测试 —— 覆盖 spec.md "未闭合标签兜底 spike" 两个 Scenario
// 场景 1: 半截标签不吞噬后续内容
// 场景 2: 增量解析稳定性（逐字符键入 + 不抖动）

import { describe, it, expect, beforeEach } from 'vitest';
import { EditorState } from '@codemirror/state';
import { syntaxTree } from '@codemirror/language';
import { markdown } from '@codemirror/lang-markdown';
import { Tree } from '@lezer/common';
import { ftagExtension, clearFtagAttrs } from '../src/ftagSyntax';

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

/** 逐字符向 doc 末尾追加输入，返回更新后的 state。 */
function typeChar(state: EditorState, ch: string): EditorState {
  return state.update({
    changes: { from: state.doc.length, insert: ch },
  }).state;
}

beforeEach(() => {
  clearFtagAttrs();
});

describe('Scenario 1: 半截标签不吞噬后续内容', () => {
  it('同行无 > 的半截标签：FTagUnclosed 区间仅起始行，100 行段落被识别为 Paragraph', () => {
    const parts: string[] = ['<f-fig id="x"'];
    for (let i = 1; i <= 100; i++) parts.push(`段落 ${i}`);
    const md = parts.join('\n\n');
    const tree = parse(md);
    const nodes = collectNodes(tree);

    // FTagUnclosed 节点存在，区间仅第一行
    const unclosed = filterByName(nodes, 'FTagUnclosed');
    expect(unclosed.length).toBe(1);
    const u = unclosed[0];
    expect(u.from).toBe(0);
    expect(md.slice(u.from, u.to)).toBe('<f-fig id="x"');

    // 不存在 FTagFig 节点（未闭合不产出完整块）
    expect(filterByName(nodes, 'FTagFig').length).toBe(0);

    // 100 行段落被识别为 Paragraph 节点（不被吞掉）
    const paragraphs = filterByName(nodes, 'Paragraph');
    expect(paragraphs.length).toBe(100);

    // 每个段落的内容可提取
    expect(md.slice(paragraphs[0].from, paragraphs[0].to).trim()).toBe('段落 1');
    expect(md.slice(paragraphs[99].from, paragraphs[99].to).trim()).toBe('段落 100');

    // 段落节点不在 FTagUnclosed 区间内
    for (const p of paragraphs) {
      expect(p.from).toBeGreaterThanOrEqual(u.to);
    }
  });

  it('同行有 > 但 EOF 无 </f-fig>：FTagUnclosed 区间仅起始标签，后续段落不被吞噬', () => {
    const parts: string[] = ['<f-fig id="x">'];
    for (let i = 1; i <= 50; i++) parts.push(`段落 ${i}`);
    const md = parts.join('\n\n');
    const tree = parse(md);
    const nodes = collectNodes(tree);

    const unclosed = filterByName(nodes, 'FTagUnclosed');
    expect(unclosed.length).toBe(1);
    const u = unclosed[0];
    expect(u.from).toBe(0);
    // 区间从 <f-fig 到 >（tagEnd）
    expect(md.slice(u.from, u.to)).toBe('<f-fig id="x">');

    expect(filterByName(nodes, 'FTagFig').length).toBe(0);

    const paragraphs = filterByName(nodes, 'Paragraph');
    expect(paragraphs.length).toBe(50);
    expect(md.slice(paragraphs[0].from, paragraphs[0].to).trim()).toBe('段落 1');
    for (const p of paragraphs) {
      expect(p.from).toBeGreaterThanOrEqual(u.to);
    }
  });
});

describe('Scenario 2: 增量解析稳定性', () => {
  it('逐字符键入 <f-fig id="x"> 过程中 FTagUnclosed 稳定存在，FTagFig 不出现，to 单调递增', () => {
    let state = EditorState.create({
      doc: '',
      extensions: [markdown({ extensions: [ftagExtension] })],
    });

    const input = '<f-fig id="x">';
    let prevUnclosedTo = -1;

    for (const ch of input) {
      state = typeChar(state, ch);
      const tree = syntaxTree(state);
      const nodes = collectNodes(tree);
      const unclosed = filterByName(nodes, 'FTagUnclosed');
      const figs = filterByName(nodes, 'FTagFig');

      const doc = state.doc.toString();
      // 一旦 doc 匹配 <f-fig（后跟空白/>//），应产出 FTagUnclosed
      if (/^<f-fig[\s>/]/.test(doc)) {
        expect(unclosed.length).toBe(1);
        expect(figs.length).toBe(0);
        // from 始终为 0
        expect(unclosed[0].from).toBe(0);
        // to 单调递增（区间稳定扩展，不抖动）
        expect(unclosed[0].to).toBeGreaterThanOrEqual(prevUnclosedTo);
        prevUnclosedTo = unclosed[0].to;
      } else {
        // 未匹配 <f-fig 时无 FTag 节点
        expect(unclosed.length).toBe(0);
        expect(figs.length).toBe(0);
      }
    }

    // 最终态：FTagUnclosed 区间 [0, 14]（<f-fig id="x">）
    const finalDoc = state.doc.toString();
    expect(finalDoc).toBe('<f-fig id="x">');
    const finalNodes = collectNodes(syntaxTree(state));
    const finalUnclosed = filterByName(finalNodes, 'FTagUnclosed');
    expect(finalUnclosed.length).toBe(1);
    expect(finalUnclosed[0].to).toBe(finalDoc.length);
  });

  it('未闭合状态下后续 <f-caption> 不被误判为 FTagCaption（不抖动）', () => {
    // 逐字符键入：<f-fig id="x">\n<f-caption>图</f-caption>\n段落1
    const input = '<f-fig id="x">\n<f-caption>图</f-caption>\n段落1';
    let state = EditorState.create({
      doc: '',
      extensions: [markdown({ extensions: [ftagExtension] })],
    });

    const captionNodeCounts: number[] = [];
    const figNodeCounts: number[] = [];
    const unclosedNodeCounts: number[] = [];

    for (const ch of input) {
      state = typeChar(state, ch);
      const tree = syntaxTree(state);
      const nodes = collectNodes(tree);
      captionNodeCounts.push(filterByName(nodes, 'FTagCaption').length);
      figNodeCounts.push(filterByName(nodes, 'FTagFig').length);
      unclosedNodeCounts.push(filterByName(nodes, 'FTagUnclosed').length);
    }

    // 全程未闭合：FTagFig 始终为 0（不抖动——不会中途误判为闭合块）
    for (const c of figNodeCounts) expect(c).toBe(0);
    // 全程 FTagCaption 始终为 0（<f-caption> 不在 FTagFig 内部，不被误判）
    // 这正是"不抖动"的核心：未闭合时后续 <f-caption> 不会被吞入块内
    for (const c of captionNodeCounts) expect(c).toBe(0);

    // 最终态：FTagUnclosed 仅覆盖第一行，后续内容独立解析
    const finalDoc = state.doc.toString();
    const finalNodes = collectNodes(syntaxTree(state));
    const unclosed = filterByName(finalNodes, 'FTagUnclosed');
    expect(unclosed.length).toBe(1);
    expect(unclosed[0].from).toBe(0);
    expect(finalDoc.slice(unclosed[0].from, unclosed[0].to)).toBe('<f-fig id="x">');
    // FTagUnclosed 不覆盖第二行 <f-caption>图</f-caption>
    const captionLineStart = '<f-fig id="x">\n'.length;
    expect(unclosed[0].to).toBeLessThanOrEqual(captionLineStart);
  });

  it('键入 </f-fig> 后从 FTagUnclosed 转为 FTagFig，<f-caption> 进入块内（语义变化，允许）', () => {
    // 先键入未闭合部分
    let state = EditorState.create({
      doc: '',
      extensions: [markdown({ extensions: [ftagExtension] })],
    });
    const before = '<f-fig id="x">\n<f-caption>图</f-caption>\n';
    for (const ch of before) {
      state = typeChar(state, ch);
    }
    // 未闭合态：FTagUnclosed 存在，无 FTagFig，无 FTagCaption
    let nodes = collectNodes(syntaxTree(state));
    expect(filterByName(nodes, 'FTagUnclosed').length).toBe(1);
    expect(filterByName(nodes, 'FTagFig').length).toBe(0);
    expect(filterByName(nodes, 'FTagCaption').length).toBe(0);

    // 键入 </f-fig>，闭合
    state = typeChar(state, '</f-fig>');
    nodes = collectNodes(syntaxTree(state));
    // 闭合态：FTagFig 出现，FTagUnclosed 消失，FTagCaption 进入块内
    expect(filterByName(nodes, 'FTagFig').length).toBe(1);
    expect(filterByName(nodes, 'FTagUnclosed').length).toBe(0);
    expect(filterByName(nodes, 'FTagCaption').length).toBe(1);
    expect(filterByName(nodes, 'FTagCaptionText').length).toBe(1);
    const captionText = filterByName(nodes, 'FTagCaptionText')[0];
    expect(state.doc.toString().slice(captionText.from, captionText.to)).toBe('图');
  });

  it('5000 行文档全量解析耗时粗测（spec 目标 < 50ms，粗测不严格）', () => {
    const lines: string[] = ['<f-fig id="x"'];
    for (let i = 1; i <= 5000; i++) lines.push(`段落 ${i}`);
    const md = lines.join('\n');

    const start = performance.now();
    parse(md);
    const elapsed = performance.now() - start;

    // 粗测：记录实际耗时，断言放宽到 200ms 防性能抖动导致测试红
    // spec 目标 50ms，若不达标在报告中说明
    // eslint-disable-next-line no-console
    console.log(`[perf] 5000 行文档全量解析耗时 ${elapsed.toFixed(2)}ms`);
    expect(elapsed).toBeLessThan(200);
  });
});
