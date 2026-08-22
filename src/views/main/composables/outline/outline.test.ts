/**
 * outlineParser 纯函数单测。
 */
import { describe, it, expect } from 'vitest';
import { parseOutlineFlat, buildTree, parseOutline, filterByLevel } from './outlineParser';

function texts(content: string): string[] {
  return parseOutlineFlat(content).map((h) => h.text);
}

describe('parseOutlineFlat', () => {
  it('空文档返回空列表', () => {
    expect(parseOutlineFlat('')).toEqual([]);
  });

  it('识别各级标题并记录行号', () => {
    const flat = parseOutlineFlat('# H1\n## H2\n# H4\n');
    expect(flat).toHaveLength(3);
    expect(flat[0]).toMatchObject({ level: 1, text: 'H1', line: 0, depth: 0 });
    expect(flat[1]).toMatchObject({ level: 2, text: 'H2', line: 1, depth: 1 });
  });

  it('忽略代码围栏内的标题行', () => {
    const content = '```\n# inside\n```\n# after\n';
    expect(texts(content)).toEqual(['after']);
  });

  it('继承最近章节标记的 sectionId', () => {
    const content = '# before\n<!-- @sec_id:abc -->\n# X\n## Y\n';
    const flat = parseOutlineFlat(content);
    expect(flat[0].sectionId).toBeNull();
    expect(flat[1].sectionId).toBe('abc');
    expect(flat[2].sectionId).toBe('abc');
  });

  it('计算后代数量（childrenCount）', () => {
    const flat = parseOutlineFlat('# H1\n## H2\n### H3\n# H4\n');
    expect(flat[0].childrenCount).toBe(2);
    expect(flat[1].childrenCount).toBe(1);
    expect(flat[2].childrenCount).toBe(0);
    expect(flat[3].childrenCount).toBe(0);
  });
});

describe('buildTree / parseOutline / filterByLevel', () => {
  it('buildTree 聚合父子层级', () => {
    const flat = parseOutline('# A\n## B\n# C\n');
    const tree = buildTree(flat);
    expect(tree.map((n) => n.text)).toEqual(['A', 'C']);
    expect(tree[0].children.map((n) => n.text)).toEqual(['B']);
  });

  it('parseOutline 一步完成建树', () => {
    const tree = parseOutline('# A\n## B\n');
    expect(tree[0].text).toBe('A');
    expect(tree[0].children[0].text).toBe('B');
  });

  it('filterByLevel 过滤浅层并提升子节点', () => {
    // 过滤掉 H1，H2 提升到根，H3 因超过 maxLevel=2 被跳过。
    const tree = parseOutline('# A\n## B\n### C\n');
    const filtered = filterByLevel(tree, 2, 2);
    expect(filtered.map((n) => n.text)).toEqual(['B']);
  });
});