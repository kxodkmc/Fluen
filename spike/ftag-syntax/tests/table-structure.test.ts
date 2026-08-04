// 表格结构节点精细化识别 spike 测试 —— 覆盖 spec.md 三个 Scenario
// 形态 C（内嵌 HTML）+ 形态 B（内嵌 MD 表）+ caption 区分

import { describe, it, expect, beforeEach } from 'vitest';
import { EditorState } from '@codemirror/state';
import { syntaxTree } from '@codemirror/language';
import { markdown } from '@codemirror/lang-markdown';
import { Tree } from '@lezer/common';
import { ftagExtension, getTableAttrs, clearFtagAttrs } from '../src/ftagSyntax';

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

describe('Scenario 1: 形态 C 表格结构节点识别', () => {
  it('每个 th/td 标签本身与内容文本均可独立提取，rowspan 属性可读', () => {
    const md =
      `<f-tbl id="tbl:x"><table><thead><tr><th>方法</th><th>精度</th></tr></thead>` +
      `<tbody><tr><td rowspan="2">Ours</td><td>95</td></tr><tr><td>93</td></tr>` +
      `</tbody></table></f-tbl>`;
    const tree = parse(md);
    const nodes = collectNodes(tree);

    // FTagTbl 块存在
    const tbls = filterByName(nodes, 'FTagTbl');
    expect(tbls.length).toBe(1);

    // FTagTable / THead / TBody / TR 结构节点存在
    expect(filterByName(nodes, 'FTagTable').length).toBe(1);
    expect(filterByName(nodes, 'FTagTHead').length).toBe(1);
    expect(filterByName(nodes, 'FTagTBody').length).toBe(1);
    expect(filterByName(nodes, 'FTagTR').length).toBe(3); // thead 1 + tbody 2

    // th 标签整体区间可独立提取
    const ths = filterByName(nodes, 'FTagTH');
    expect(ths.length).toBe(2);
    expect(md.slice(ths[0].from, ths[0].to)).toBe('<th>方法</th>');
    expect(md.slice(ths[1].from, ths[1].to)).toBe('<th>精度</th>');

    // th 内容文本区间可独立提取，与标签区间不重叠（内容严格在标签内）
    const thContents = filterByName(nodes, 'FTagTHContent');
    expect(thContents.length).toBe(2);
    expect(md.slice(thContents[0].from, thContents[0].to)).toBe('方法');
    expect(md.slice(thContents[1].from, thContents[1].to)).toBe('精度');
    for (let i = 0; i < ths.length; i++) {
      expect(thContents[i].from).toBeGreaterThan(ths[i].from);
      expect(thContents[i].to).toBeLessThan(ths[i].to);
    }

    // td 标签整体区间可独立提取
    const tds = filterByName(nodes, 'FTagTD');
    expect(tds.length).toBe(3);
    expect(md.slice(tds[0].from, tds[0].to)).toBe('<td rowspan="2">Ours</td>');
    expect(md.slice(tds[1].from, tds[1].to)).toBe('<td>95</td>');
    expect(md.slice(tds[2].from, tds[2].to)).toBe('<td>93</td>');

    // td 内容文本区间可独立提取
    const tdContents = filterByName(nodes, 'FTagTDContent');
    expect(tdContents.length).toBe(3);
    expect(md.slice(tdContents[0].from, tdContents[0].to)).toBe('Ours');
    expect(md.slice(tdContents[1].from, tdContents[1].to)).toBe('95');
    expect(md.slice(tdContents[2].from, tdContents[2].to)).toBe('93');

    // rowspan 属性可从对应 td 节点读出
    const tdWithRowspan = tds.find((td) =>
      md.slice(td.from, td.to).includes('rowspan'),
    );
    expect(tdWithRowspan).toBeDefined();
    const attrs = getTableAttrs(tdWithRowspan!.from);
    expect(attrs).toBeDefined();
    expect(attrs?.rowspan).toBe('2');
  });

  it('colspan 属性同样可读', () => {
    const md =
      `<f-tbl id="tbl:col"><table><tbody>` +
      `<tr><td colspan="3">合并</td></tr>` +
      `</tbody></table></f-tbl>`;
    const tree = parse(md);
    const nodes = collectNodes(tree);

    const tds = filterByName(nodes, 'FTagTD');
    expect(tds.length).toBe(1);
    const attrs = getTableAttrs(tds[0].from);
    expect(attrs).toBeDefined();
    expect(attrs?.colspan).toBe('3');
    expect(md.slice(tds[0].from, tds[0].to)).toBe('<td colspan="3">合并</td>');

    const tdContents = filterByName(nodes, 'FTagTDContent');
    expect(tdContents.length).toBe(1);
    expect(md.slice(tdContents[0].from, tdContents[0].to)).toBe('合并');
  });
});

describe('Scenario 2: 形态 B（MD 表）等价结构', () => {
  it('MD 表行解析为虚拟 TR/TH/TD 节点，单元格内容可独立提取', () => {
    const md = [
      `<f-tbl id="tbl:y">`,
      `| 方法 | 精度 |`,
      `| --- | --- |`,
      `| Ours | 95 |`,
      `</f-tbl>`,
    ].join('\n');
    const tree = parse(md);
    const nodes = collectNodes(tree);

    // FTagTbl 块存在
    const tbls = filterByName(nodes, 'FTagTbl');
    expect(tbls.length).toBe(1);

    // 产出 FTagTable 包裹节点
    expect(filterByName(nodes, 'FTagTable').length).toBe(1);

    // 产出 TR 节点（header 1 + body 1 = 2 TRs，分隔行不产出）
    const trs = filterByName(nodes, 'FTagTR');
    expect(trs.length).toBe(2);

    // header TH cells
    const ths = filterByName(nodes, 'FTagTH');
    expect(ths.length).toBe(2);
    // body TD cells
    const tds = filterByName(nodes, 'FTagTD');
    expect(tds.length).toBe(2);

    // 单元格内容可独立提取
    const thContents = filterByName(nodes, 'FTagTHContent');
    expect(thContents.length).toBe(2);
    const thValues = thContents.map((c) => md.slice(c.from, c.to)).sort();
    expect(thValues).toEqual(['方法', '精度']);

    const tdContents = filterByName(nodes, 'FTagTDContent');
    expect(tdContents.length).toBe(2);
    const tdValues = tdContents.map((c) => md.slice(c.from, c.to)).sort();
    expect(tdValues).toEqual(['95', 'Ours']);

    // 内容区间不与标签区间相等（TH 区间含周围空格，THContent 仅含文本）
    for (let i = 0; i < ths.length; i++) {
      expect(ths[i].from).toBeLessThanOrEqual(thContents[i].from);
      expect(ths[i].to).toBeGreaterThanOrEqual(thContents[i].to);
    }
  });
});

describe('Scenario 3: 表格 caption 区分', () => {
  it('caption 内容与表格单元格内容可清晰区分，区间不重叠', () => {
    const md =
      `<f-tbl id="tbl:z"><f-caption>表说明</f-caption>` +
      `<table><thead><tr><th>A</th></tr></thead></table></f-tbl>`;
    const tree = parse(md);
    const nodes = collectNodes(tree);

    const captions = filterByName(nodes, 'FTagCaption');
    const captionTexts = filterByName(nodes, 'FTagCaptionText');
    const thContents = filterByName(nodes, 'FTagTHContent');
    const ths = filterByName(nodes, 'FTagTH');

    expect(captions.length).toBe(1);
    expect(captionTexts.length).toBe(1);
    expect(thContents.length).toBe(1);
    expect(ths.length).toBe(1);

    // caption 内容是"表说明"，不与单元格内容"A"混淆
    expect(md.slice(captionTexts[0].from, captionTexts[0].to)).toBe('表说明');
    expect(md.slice(thContents[0].from, thContents[0].to)).toBe('A');

    // caption 区间与表格结构节点区间不重叠：caption 在 th 之前
    expect(captions[0].to).toBeLessThanOrEqual(ths[0].from);

    // caption 内容与单元格内容区间不重叠
    expect(captionTexts[0].to).toBeLessThanOrEqual(thContents[0].from);
  });
});
