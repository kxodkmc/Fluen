/**
 * TipTap 预览编辑视图 — 扩展构建。
 *
 * 实验视图的扩展清单保持最小：StarterKit + 官方 Markdown 包（Beta，marked
 * 内核）。原则：**关闭一切没有 markdown 序列化形式的扩展**（如 Underline），
 * 否则用户设置的格式在序列化回 main.md 时会被静默丢弃。
 *
 * 数学公式：`$...$` / `$$...$$` 解析为 KaTeX 渲染节点（见 tiptap/math.ts），
 * 序列化还原为标准 LaTeX 定界符，往返无损。
 */

import { Extension } from '@tiptap/core';
import { StarterKit } from '@tiptap/starter-kit';
import { Markdown } from '@tiptap/markdown';
import { Table } from '@tiptap/extension-table';
import { TableRow } from '@tiptap/extension-table-row';
import { TableHeader } from '@tiptap/extension-table-header';
import { TableCell } from '@tiptap/extension-table-cell';
import { BlockMath, InlineMath } from './math';
import { Ftbl, TableCaption } from './ftbl';
import type { Extensions } from '@tiptap/core';

/** 编辑器内 Mod-s 保存快捷键（全局 capture 通道为主，此处兜底）。 */
function saveShortcut(onSave: () => void): Extension {
  return Extension.create({
    name: 'fluenSaveShortcut',
    addKeyboardShortcuts() {
      return {
        'Mod-s': () => {
          onSave();
          return true;
        },
      };
    },
  });
}

/**
 * 构建预览编辑视图的扩展列表。
 *
 * @param onSave 编辑器内 Ctrl+S 触发的保存回调。
 */
export function buildWysiwygExtensions(onSave: () => void): Extensions {
  return [
    StarterKit.configure({
      // 下划线没有 markdown 等价形式，序列化会静默丢失，禁用
      underline: false,
    }),
    // 原生表格（GFM 往返已验证：内容与单元格内格式完好，仅列宽填充不同）
    Table.configure({ resizable: false }),
    TableRow,
    TableHeader,
    TableCell,
    // 题注表格（<f-tbl> 规范块）与数学公式（KaTeX 渲染节点）
    TableCaption,
    Ftbl,
    InlineMath,
    BlockMath,
    Markdown,
    saveShortcut(onSave),
  ];
}
