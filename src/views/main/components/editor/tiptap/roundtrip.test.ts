/**
 * TipTap Markdown 往返测试（@tiptap/markdown 3.31 Beta 行为钉桩）。
 *
 * setContent(md, { contentType: 'markdown' }) → getMarkdown()，空白归一化后
 * 与输入比对。用途：
 *   1. 钉住常见结构的往返保真（升级 Beta 包时第一时间发现回归）
 *   2. 固定已知限制的当前行为，明确预期而非静默漂移
 */

import { describe, it, expect } from 'vitest';
import { Editor } from '@tiptap/core';
import { StarterKit } from '@tiptap/starter-kit';
import { Markdown } from '@tiptap/markdown';
import { Table } from '@tiptap/extension-table';
import { TableRow } from '@tiptap/extension-table-row';
import { TableHeader } from '@tiptap/extension-table-header';
import { TableCell } from '@tiptap/extension-table-cell';
import { BlockMath, InlineMath } from './math';
import { Ftbl, TableCaption } from './ftbl';

/** 空白归一化：行尾空格去除、3+ 连续换行收敛为 2（序列化对空行有冗余）。 */
function normalize(md: string): string {
  return md
    .split('\n')
    .map((l) => l.replace(/\s+$/, ''))
    .join('\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

function roundTrip(md: string): string {
  const ed = new Editor({
    extensions: [
      StarterKit.configure({ underline: false }),
      Table.configure({ resizable: false }),
      TableRow,
      TableHeader,
      TableCell,
      TableCaption,
      Ftbl,
      InlineMath,
      BlockMath,
      Markdown,
    ],
    content: md,
    contentType: 'markdown',
  });
  const out = ed.getMarkdown();
  ed.destroy();
  return out;
}

describe('TipTap markdown 往返（无编辑）', () => {
  it('标题/行内格式/嵌套列表/引用/围栏代码/hr/链接 逐字节一致', () => {
    const md = [
      '# 标题一',
      '',
      '正文段落，**加粗**与*斜体*与`code`。',
      '',
      '## 小节',
      '',
      '### 三级',
      '',
      '- 项目a',
      '- 项目b',
      '  - 嵌套',
      '',
      '1. 第一',
      '2. 第二',
      '',
      '> 引用文字',
      '',
      '```ts',
      'const x = 1;',
      '```',
      '',
      '---',
      '',
      '[链接文字](https://example.com)',
    ].join('\n');
    expect(normalize(roundTrip(md))).toBe(normalize(md));
  });

  it('行内公式 $...$ 字面保留', () => {
    const md = '质能方程 $E=mc^2$ 是物理学基石。\n';
    expect(normalize(roundTrip(md))).toContain('$E=mc^2$');
  });

  it('不含特殊字符的块级公式 $$...$$ 保留', () => {
    const md = '$$\ny = ax + b\n$$\n\n后续段落。\n';
    const out = normalize(roundTrip(md));
    expect(out).toContain('$$');
    expect(out).toContain('y = ax + b');
  });

  it('块级公式 $$...$$ 含反斜杠/下划线往返无损（数学节点序列化）', () => {
    const md = '$$\n\\int_0^1 x dx\n$$\n\n后续段落。\n';
    const out = normalize(roundTrip(md));
    expect(out).toContain('$$');
    expect(out).toContain('\\int_0^1 x dx');
    expect(out).not.toContain('\\\\int');
    expect(out).not.toContain('\\_');
  });

  it('f-tbl 题注表格往返：解析为可编辑结构并按规范序列化', () => {
    const md = '<f-tbl>\n  <f-caption>表格题注</f-caption>\n\n| 列A | 列B |\n| --- | --- |\n| 1 | 2 |\n\n</f-tbl>\n';
    const out = normalize(roundTrip(md));
    expect(out).toContain('<f-tbl>');
    expect(out).toContain('<f-caption>表格题注</f-caption>');
    expect(out).toContain('| 列A');
    // 列宽填充允许变化（序列化对齐美化），内容必须完好
    expect(out).toMatch(/\|\s*1\s*\|/);
    expect(out).toMatch(/\|\s*2\s*\|/);
    expect(out).toContain('</f-tbl>');
  });

  it('裸 GFM 表格（粘贴等来源）往返：内容与单元格内格式完好', () => {
    const md = [
      '前置段落。',
      '',
      '| 列A | 列B |',
      '| --- | --- |',
      '| 1 | 2 |',
      '| **粗** | *斜* |',
      '',
      '后置段落。',
    ].join('\n');
    const out = normalize(roundTrip(md));
    expect(out).toContain('前置段落。');
    expect(out).toContain('| 列A');
    expect(out).toContain('| **粗** | *斜* |');
    expect(out).toContain('后置段落。');
  });

  it('已知限制：HTML 注释（章节标记）被静默丢弃——证实剥离/回插方案的必要性', () => {
    const md = '<!-- @sec_id:aaaabbbbccccdddd -->\n# 章节\n\n正文\n';
    const out = roundTrip(md);
    expect(out).not.toContain('@sec_id');
    expect(out).toContain('# 章节');
  });

  it('Fluen 自定义围栏块（f-tbl）原样保留', () => {
    const md = '```f-tbl\nx=1\n```\n\n普通段落\n';
    expect(normalize(roundTrip(md))).toBe(normalize(md));
  });
});
