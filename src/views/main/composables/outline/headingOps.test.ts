/**
 * headingOps 结构操作纯函数单测。
 */
import { describe, it, expect } from 'vitest';
import { renameHeadingAt, insertChildAt } from './headingOps';
import type { ChangeSpec } from '@codemirror/state';

/** 将编辑片段应用到文档（用于断言最终结果）。 */
function apply(doc: string, spec: ChangeSpec): string {
  const s = spec as { from: number; to: number; insert?: string };
  return doc.slice(0, s.from) + (s.insert ?? '') + doc.slice(s.to);
}

describe('renameHeadingAt', () => {
  it('重命名成功：替换整行并保留层级', () => {
    const res = renameHeadingAt('# 旧标题\n## 保留\n', { line: 0, level: 1, text: '旧标题' }, '新标题');
    expect(res.error).toBeUndefined();
    expect(apply('# 旧标题\n## 保留\n', res.changes![0])).toBe('# 新标题\n## 保留\n');
  });

  it('行未变化时报错且不产生片段', () => {
    const res = renameHeadingAt('# 新标题\n', { line: 0, level: 1, text: '旧标题' }, '其他');
    expect(res.error).toBeTruthy();
    expect(res.changes).toBeUndefined();
  });

  it('空标题或含换行时报错', () => {
    expect(renameHeadingAt('# A\n', { line: 0, level: 1, text: 'A' }, '  ').error).toBeTruthy();
    expect(renameHeadingAt('# A\n', { line: 0, level: 1, text: 'A' }, 'A\nB').error).toBeTruthy();
  });
});

describe('insertChildAt', () => {
  it('在父作用域末尾（兄弟标题前）插入，层级 +1', () => {
    const doc = '# H1\nparent text\n# H2\n';
    const res = insertChildAt(doc, { line: 0, level: 1 }, 'C');
    expect(res.error).toBeUndefined();
    expect(apply(doc, res.changes![0])).toBe('# H1\nparent text\n\n## C\n\n# H2\n');
  });

  it('无兄弟标题时追加到文档末尾', () => {
    const doc = '# H1\nonly content';
    const res = insertChildAt(doc, { line: 0, level: 1 }, 'C');
    expect(apply(doc, res.changes![0])).toBe('# H1\nonly content\n\n## C\n');
  });

  it('嵌套作用域边界：插入在最内层作用域之后、外层兄弟之前', () => {
    const doc = '# H1\n## H2\ntext2\n# H3\n';
    const res = insertChildAt(doc, { line: 0, level: 1 }, 'C');
    expect(apply(doc, res.changes![0])).toBe('# H1\n## H2\ntext2\n\n## C\n\n# H3\n');
  });

  it('跳过代码围栏内的标题，定位真实兄弟', () => {
    const doc = '# H1\n```\n# notReal\n```\n# H2';
    const res = insertChildAt(doc, { line: 0, level: 1 }, 'C');
    expect(res.error).toBeUndefined();
    expect(apply(doc, res.changes![0])).toBe('# H1\n```\n# notReal\n```\n\n## C\n\n# H2');
  });

  it('已达 H6 时拒绝再插入子标题', () => {
    expect(insertChildAt('# H1\n', { line: 0, level: 6 }, 'C').error).toBeTruthy();
  });
});