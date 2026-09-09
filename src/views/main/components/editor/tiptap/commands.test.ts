/**
 * TipTap 命令目标测试 — 公式连续录入的合并行为。
 *
 * 核心约定：工具栏公式面板连续插入符号/结构时必须合并为**单个**公式节点
 * （`$\nabla\mu$`），而不是产生相邻的多个节点（`$\nabla$$\mu$`）。
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
import { createTiptapCommandTarget } from './commands';

function makeEditor(content = '正文段落。\n'): Editor {
  return new Editor({
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
    content,
    contentType: 'markdown',
  });
}

describe('命令目标：公式插入逻辑', () => {
  it('公式面板连续录入符号合并为单个行内公式节点', () => {
    const ed = makeEditor();
    const target = createTiptapCommandTarget(() => ed);
    ed.commands.focus('end');
    target.insertMathSnippet('\\nabla');
    target.insertMathSnippet('\\mu');
    target.insertMathSnippet('\\alpha');
    const md = ed.getMarkdown();
    expect(md).toContain('$\\nabla\\mu\\alpha$');
    expect(md).not.toContain('$\\nabla$$');
    ed.destroy();
  });

  it('先插入空公式再录入符号：同样合并', () => {
    const ed = makeEditor();
    const target = createTiptapCommandTarget(() => ed);
    ed.commands.focus('end');
    target.insertInlineMath();
    target.insertMathSnippet('E=mc^2');
    expect(ed.getMarkdown()).toContain('$E=mc^2$');
    ed.destroy();
  });

  it('行内公式切换：选中已有公式节点时删除（三态对齐 CM6）', () => {
    const ed = makeEditor('前 $x+y$ 后。\n');
    const target = createTiptapCommandTarget(() => ed);
    // 光标移到行内公式节点之后（$from.nodeBefore 命中）
    const docSize = ed.state.doc.content.size;
    // 定位到公式节点之后：简单起见聚焦末尾后逐步前移——直接找文本位置
    let target2 = 0;
    ed.state.doc.descendants((node, pos) => {
      if (node.type.name === 'inlineMath') target2 = pos + node.nodeSize;
    });
    ed.commands.focus(target2);
    void docSize;
    const removed = target.insertInlineMath();
    expect(removed).toBe(true);
    const md = ed.getMarkdown();
    expect(md).not.toContain('$x+y$');
    expect(md).toContain('x+y');
    ed.destroy();
  });

  it('块级公式插入为独立 $$ 节点并可连续录入', () => {
    const ed = makeEditor();
    const target = createTiptapCommandTarget(() => ed);
    ed.commands.focus('end');
    target.insertMathBlock();
    target.insertMathSnippet('\\sum_i x_i');
    const md = ed.getMarkdown();
    expect(md).toContain('$$\n\\sum_i x_i\n$$');
    ed.destroy();
  });
});
