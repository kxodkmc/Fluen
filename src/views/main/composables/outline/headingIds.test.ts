/**
 * headingIds 稳定 id 单测。
 */
import { describe, it, expect } from 'vitest';
import { assignStableIds } from './headingIds';
import { parseOutlineFlat } from './outlineParser';

function ids(content: string): string[] {
  return assignStableIds([], parseOutlineFlat(content)).map((h) => h.id);
}

describe('assignStableIds', () => {
  it('上方插入行后，其后标题 id 不变', () => {
    const prev = assignStableIds([], parseOutlineFlat('# A\n# B\n'));
    const next = assignStableIds(prev, parseOutlineFlat('# A\n# M\n# B\n'));
    expect(next.map((h) => h.text)).toEqual(['A', 'M', 'B']);
    expect(next[0].id).toBe(prev[0].id);
    expect(next[2].id).toBe(prev[1].id); // B 复用旧 id，避免重建
    expect(next[1].id).not.toBe(prev[1].id); // 新标题分配新 id
  });

  it('上方删除行后，其后标题 id 不变', () => {
    const prev = assignStableIds([], parseOutlineFlat('# A\n# B\n# C\n'));
    const next = assignStableIds(prev, parseOutlineFlat('# B\n# C\n'));
    expect(next[0].id).toBe(prev[1].id);
    expect(next[1].id).toBe(prev[2].id);
  });

  it('新标题分配全新 id（不与旧集合冲突）', () => {
    const prev = assignStableIds([], parseOutlineFlat('# Old\n'));
    const next = assignStableIds(prev, parseOutlineFlat('# Old\n# New\n'));
    expect(next[0].id).toBe(prev[0].id);
    expect(prev.map((p) => p.id)).not.toContain(next[1].id);
  });

  it('重名标题按顺序队列复用 id', () => {
    const prev = assignStableIds([], parseOutlineFlat('# A\n# A\n'));
    const next = assignStableIds(prev, parseOutlineFlat('# A\n# A\n'));
    expect(next[0].id).toBe(prev[0].id);
    expect(next[1].id).toBe(prev[1].id);
  });

  it('空输入返回空列表', () => {
    expect(ids('')).toEqual([]);
  });
});