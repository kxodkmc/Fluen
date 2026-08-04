// 脚注语法识别 spike 测试 —— 覆盖 spec.md 三个 Scenario
// 行内引用 [^id] + 定义块 [^id]:（单行 / 多段落缩进续行）

import { describe, it, expect, beforeEach } from 'vitest';
import { EditorState } from '@codemirror/state';
import { syntaxTree } from '@codemirror/language';
import { markdown } from '@codemirror/lang-markdown';
import { Tree } from '@lezer/common';
import {
  footnoteExtension,
  getFootnoteDefId,
  getFootnoteRefId,
  clearFootnoteAttrs,
} from '../src/footnoteSyntax';

function parse(md: string): Tree {
  const state = EditorState.create({
    doc: md,
    extensions: [markdown({ extensions: [footnoteExtension] })],
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
  clearFootnoteAttrs();
});

describe('Scenario 1: 单行定义识别', () => {
  it('FootnoteDefinition 区间覆盖整行，id 与定义内容可分别提取', () => {
    const md = `[^1]: 这是单行脚注定义。`;
    const tree = parse(md);
    const nodes = collectNodes(tree);

    const defs = filterByName(nodes, 'FootnoteDefinition');
    expect(defs.length).toBe(1);

    const def = defs[0];
    // 节点区间覆盖整行
    expect(def.from).toBe(0);
    expect(def.to).toBe(md.length);
    expect(md.slice(def.from, def.to)).toBe(md);

    // 脚注 id 可提取（side-table）
    expect(getFootnoteDefId(def.from)).toBe('1');

    // 定义内容可提取（从 def 区间文本切串）
    const defText = md.slice(def.from, def.to);
    const m = /^\[\^([^\]]+)\]:\s*(.*)$/.exec(defText);
    expect(m).not.toBeNull();
    expect(m![1]).toBe('1');
    expect(m![2]).toBe('这是单行脚注定义。');
  });
});

describe('Scenario 2: 多段落定义识别（缩进续行）', () => {
  it('整个多段落块被识别为同一 FootnoteDefinition，缩行续行归入该定义块', () => {
    const md = [
      `[^1]: 第一段定义。`,
      ``,
      `    续行段落（4 空格缩进）。`,
      ``,
      `    另一段续行。`,
    ].join('\n');
    const tree = parse(md);
    const nodes = collectNodes(tree);

    const defs = filterByName(nodes, 'FootnoteDefinition');
    expect(defs.length).toBe(1);

    const def = defs[0];
    // 节点区间覆盖从 [^1]: 到最后一段续行末
    expect(def.from).toBe(0);
    expect(def.to).toBe(md.length);
    expect(md.slice(def.from, def.to)).toBe(md);

    // id 可提取
    expect(getFootnoteDefId(def.from)).toBe('1');

    // 缩行续行段落归入该定义块，不被识别为独立的段落块
    // 检查 def 区间内没有 Paragraph 节点重叠
    const paragraphs = filterByName(nodes, 'Paragraph');
    for (const p of paragraphs) {
      const overlap = p.from < def.to && p.to > def.from;
      expect(overlap).toBe(false);
    }

    // 同时验证没有 IndentedCode 块落在 def 区间内
    // （4 空格缩进行可能被 IndentedCode 认领，但本 spike 由 FootnoteDefinition 先消费）
    const codeBlocks = filterByName(nodes, 'CodeBlock');
    for (const c of codeBlocks) {
      const overlap = c.from < def.to && c.to > def.from;
      expect(overlap).toBe(false);
    }
  });
});

describe('Scenario 3: 行内引用识别', () => {
  it('FootnoteRef 节点区间仅覆盖 [^1] 本身，id 可提取', () => {
    const md = `正文中间出现引用[^1]继续文字。`;
    const tree = parse(md);
    const nodes = collectNodes(tree);

    const refs = filterByName(nodes, 'FootnoteRef');
    expect(refs.length).toBe(1);

    const ref = refs[0];
    // 节点区间仅覆盖 [^1] 本身
    expect(md.slice(ref.from, ref.to)).toBe('[^1]');
    // 不含前后正文
    expect(md.slice(0, ref.from)).toBe('正文中间出现引用');
    expect(md.slice(ref.to)).toBe('继续文字。');

    // 脚注 id 可提取
    expect(getFootnoteRefId(ref.from)).toBe('1');
  });
});
