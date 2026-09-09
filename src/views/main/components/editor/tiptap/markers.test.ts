/**
 * markers.ts 单元测试 — 章节标记剥离/回插的纯字符串行为。
 */

import { describe, it, expect } from 'vitest';
import { extractSecMarkers, reinsertSecMarkers } from './markers';

const M1 = '<!-- @sec_id:aaaabbbbccccdddd -->';
const M2 = '<!-- @sec_id:1111222233334444 -->';

describe('extractSecMarkers', () => {
  it('剥离紧邻 H1 的标记并按顺序绑定', () => {
    const md = `${M1}\n# 第一章\n\n正文\n\n${M2}\n# 第二章\n\n内容\n`;
    const { body, markers } = extractSecMarkers(md);
    expect(body).not.toContain(M1);
    expect(body).not.toContain(M2);
    expect(body).toContain('# 第一章');
    expect(markers).toEqual([M1, M2]);
  });

  it('堆叠标记只有最后一个绑定', () => {
    const md = `${M1}\n${M2}\n# 标题\n\n正文\n`;
    const { markers } = extractSecMarkers(md);
    expect(markers).toEqual([M2]);
  });

  it('标记后跟普通文本时标记失效（不绑定后续 H1）', () => {
    const md = `${M1}\n普通段落\n\n# 标题\n\n正文\n`;
    const { markers, body } = extractSecMarkers(md);
    expect(markers).toEqual([]);
    expect(body).toContain('普通段落');
    expect(body).toContain('# 标题');
  });

  it('标记允许与 H1 之间隔空行', () => {
    const md = `${M1}\n\n# 标题\n\n正文\n`;
    const { markers } = extractSecMarkers(md);
    expect(markers).toEqual([M1]);
  });

  it('EOF 处未绑定的标记被丢弃', () => {
    const md = `# 标题\n\n正文\n\n${M1}\n`;
    const { markers } = extractSecMarkers(md);
    expect(markers).toEqual([]);
  });

  it('无标记时 markers 为空、body 原样返回', () => {
    const md = '# 标题\n\n正文\n';
    const { body, markers } = extractSecMarkers(md);
    expect(markers).toEqual([]);
    expect(body).toBe(md);
  });
});

describe('reinsertSecMarkers', () => {
  it('剥离后回插可还原原文', () => {
    const md = `${M1}\n# 第一章\n\n正文\n\n${M2}\n# 第二章\n\n内容\n`;
    const { body, markers } = extractSecMarkers(md);
    expect(reinsertSecMarkers(body, markers)).toBe(md);
  });

  it('H1 数量与标记数量不一致时返回 null（新增章节）', () => {
    const { markers } = extractSecMarkers(`${M1}\n# 一\n\n正文\n`);
    const newDoc = '# 一\n\n正文\n\n# 二\n\n内容\n';
    expect(reinsertSecMarkers(newDoc, markers)).toBeNull();
  });

  it('删除章节（H1 变少）时返回 null', () => {
    const { markers } = extractSecMarkers(`${M1}\n# 一\n\n正文\n\n${M2}\n# 二\n\n内容\n`);
    expect(reinsertSecMarkers('# 一\n\n正文\n', markers)).toBeNull();
  });

  it('无标记且无 H1 时返回原文', () => {
    expect(reinsertSecMarkers('正文段落\n', [])).toBe('正文段落\n');
  });

  it('回插后的文本可再次剥离且结果稳定（往返恒等）', () => {
    const md = `${M1}\n# 一\n\n正文\n\n${M2}\n# 二\n\n内容\n`;
    const first = extractSecMarkers(md);
    const round = reinsertSecMarkers(first.body, first.markers);
    expect(round).toBe(md);
    const second = extractSecMarkers(round!);
    expect(second).toEqual(first);
  });
});
