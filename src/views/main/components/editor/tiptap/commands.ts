/**
 * TipTap 命令目标实现 — 预览编辑视图的编辑命令（工具栏路由目标）。
 *
 * 实现 `EditorCommandTarget` 接口，把工具栏命令（格式切换 / 插入表格公式 /
 * 撤销重做）转发给 TipTap 编辑器。与 CM6 路由（useFluenEditor 内部实现）平行，
 * 由 FluenWysiwygEditor 挂载时注册。
 *
 * 公式：插入 inlineMath / blockMath 节点（KaTeX 渲染，见 tiptap/math.ts）；
 * 符号/结构片段在选中公式节点上时追加 latex，否则插入新的行内公式节点。
 * 表格：插入 `<f-tbl>` 规范结构（题注 + 原生表格，见 tiptap/ftbl.ts）。
 */

import { NodeSelection } from '@tiptap/pm/state';
import type { Node as PMNode } from '@tiptap/pm/model';
import type { Editor } from '@tiptap/core';
import type { MarkdownFormatKind, HeadingLevel } from '../codemirror/formatting';
import type { TableSyntax } from '../codemirror/tableModel';
import type { EditorCommandTarget } from '../composables/useFluenEditor';

/** 判断选区是否落在数学公式节点上（NodeSelection 直接选中 / 光标紧邻其后）。 */
function selectedMathNode(ed: Editor): { pos: number; node: PMNode } | null {
  const { selection } = ed.state;
  if (selection instanceof NodeSelection && ['inlineMath', 'blockMath'].includes(selection.node.type.name)) {
    return { pos: selection.from, node: selection.node };
  }
  const after = selection.$from.nodeAfter;
  if (after && ['inlineMath', 'blockMath'].includes(after.type.name)) {
    return { pos: selection.$from.pos, node: after };
  }
  return null;
}

/** 光标紧邻其前的公式节点（`$x$|` 形态——新建公式节点后的光标落点）。 */
function mathNodeBefore(ed: Editor): { pos: number; node: PMNode } | null {
  const before = ed.state.selection.$from.nodeBefore;
  if (before && ['inlineMath', 'blockMath'].includes(before.type.name)) {
    const pos = ed.state.selection.$from.pos - before.nodeSize;
    return { pos, node: before };
  }
  return null;
}

/** 向公式节点追加 latex 片段，并保持该节点选中（支持连续录入）。 */
function appendLatex(ed: Editor, pos: number, node: PMNode, latex: string): boolean {
  const tr = ed.state.tr.setNodeMarkup(pos, undefined, {
    ...node.attrs,
    latex: String(node.attrs.latex ?? '') + latex,
  });
  tr.setSelection(NodeSelection.create(tr.doc, pos));
  ed.view.dispatch(tr);
  return true;
}

/** 构建规范 f-tbl 结构 JSON：题注 + 表头行 + 数据行。 */
function buildFtblJSON(rows: number, cols: number, caption: string): Record<string, unknown> {
  const cell = (type: 'tableHeader' | 'tableCell') => ({ type, content: [{ type: 'paragraph' }] });
  const row = (type: 'tableHeader' | 'tableCell') => ({
    type: 'tableRow',
    content: Array.from({ length: cols }, () => cell(type)),
  });
  return {
    type: 'ftbl',
    content: [
      {
        type: 'tableCaption',
        content: caption ? [{ type: 'text', text: caption }] : [],
      },
      {
        type: 'table',
        content: [row('tableHeader'), ...Array.from({ length: Math.max(0, rows - 1) }, () => row('tableCell'))],
      },
    ],
  };
}

/**
 * 创建 TipTap 命令目标。
 *
 * @param getEditor 取当前 TipTap 编辑器实例（可能为 null，如销毁后）。
 */
export function createTiptapCommandTarget(getEditor: () => Editor | null): EditorCommandTarget {
  return {
    toggleFormat(kind: MarkdownFormatKind, level: HeadingLevel = 1): void {
      const ed = getEditor();
      if (!ed) return;
      switch (kind) {
        case 'bold':
          ed.chain().focus().toggleBold().run();
          break;
        case 'italic':
          ed.chain().focus().toggleItalic().run();
          break;
        case 'heading':
          ed.chain().focus().toggleHeading({ level }).run();
          break;
        case 'underline':
          // Underline 扩展已禁用（无 markdown 序列化形式），静默忽略
          break;
        case 'inlineMath':
          // 与 CM6 行为对齐：行内公式切换交给专用实现
          this.insertInlineMath();
          break;
      }
    },

    insertTable(rows: number, cols: number, _syntax: TableSyntax, caption: string): boolean {
      const ed = getEditor();
      if (!ed) return false;
      // wysiwyg 一律插入规范 f-tbl 结构（题注 + 原生表格），语法形态参数不适用
      return ed.chain().focus().insertContent(buildFtblJSON(rows, cols, caption)).run();
    },

    insertInlineMath(): boolean {
      const ed = getEditor();
      if (!ed) return false;
      // 已选中 / 光标紧邻行内公式节点 → 切换为纯文本（还原 latex，对齐 CM6 unwrap）
      const adj = selectedMathNode(ed) ?? mathNodeBefore(ed);
      if (adj && adj.node.type.name === 'inlineMath') {
        const latex = String(adj.node.attrs.latex ?? '');
        return ed
          .chain()
          .focus()
          .insertContentAt(
            { from: adj.pos, to: adj.pos + adj.node.nodeSize },
            latex ? { type: 'text', text: latex } : [],
          )
          .run();
      }
      const { from, to, empty } = ed.state.selection;
      if (empty) {
        const ok = ed.chain().focus().insertContent({ type: 'inlineMath', attrs: { latex: '' } }).run();
        if (!ok) return false;
        // 选中新节点：后续公式面板的符号插入直接追加进来
        const created = mathNodeBefore(ed);
        if (created) ed.chain().setNodeSelection(created.pos).run();
        return true;
      }
      const text = ed.state.doc.textBetween(from, to, '\n');
      return ed
        .chain()
        .focus()
        .insertContentAt({ from, to }, { type: 'inlineMath', attrs: { latex: text } })
        .run();
    },

    insertMathBlock(): boolean {
      const ed = getEditor();
      if (!ed) return false;
      const ok = ed
        .chain()
        .focus()
        .insertContent({ type: 'blockMath', attrs: { latex: '' } })
        .run();
      if (!ok) return false;
      // 选中新节点：后续符号插入追加进同一公式
      const adj = mathNodeBefore(ed);
      if (adj) ed.chain().setNodeSelection(adj.pos).run();
      return true;
    },

    insertMathSnippet(latex: string): boolean {
      const ed = getEditor();
      if (!ed) return false;
      // 选中公式节点 → 追加（连续录入合并为同一公式）
      const selected = selectedMathNode(ed);
      if (selected) return appendLatex(ed, selected.pos, selected.node, latex);
      // 光标紧跟公式节点之后（新建节点后的落点）→ 同样追加
      const before = mathNodeBefore(ed);
      if (before) return appendLatex(ed, before.pos, before.node, latex);
      // 否则新建行内公式节点并选中，随后的录入继续合并
      const ok = ed
        .chain()
        .focus()
        .insertContent({ type: 'inlineMath', attrs: { latex } })
        .run();
      if (!ok) return false;
      const adj = mathNodeBefore(ed);
      if (adj) ed.chain().setNodeSelection(adj.pos).run();
      return true;
    },

    undo(): void {
      getEditor()?.chain().focus().undo().run();
    },

    redo(): void {
      getEditor()?.chain().focus().redo().run();
    },

    canUndo(): boolean {
      const ed = getEditor();
      return ed ? ed.can().undo() : false;
    },

    canRedo(): boolean {
      const ed = getEditor();
      return ed ? ed.can().redo() : false;
    },
  };
}
