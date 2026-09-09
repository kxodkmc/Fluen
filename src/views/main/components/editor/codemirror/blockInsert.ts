/**
 * 块级内容插入位置计算（纯函数层，不依赖 CodeMirror / Vue）。
 *
 * 在光标处插入块级内容（表格、图片、公式等）时，保证 Markdown 块间
 * 空行隔离：
 *   - 当前行无内容（含空文档）→ 就地插入；上一行非空时补前导空行，
 *     块后补空行；
 *   - 当前行有内容且非末行   → 跳到下一行行首插入，与当前行之间补空行；
 *   - 当前行有内容且为末行   → 文档尾追加，前置两个换行形成独立段落。
 *
 * 与 `formatting.ts` 同构：输入文档文本与光标位置，输出变更与新选区。
 */

/** 单条文档变更（与 CodeMirror ChangeSpec 兼容的子集）。 */
export interface DocChange {
  from: number;
  to: number;
  insert: string;
}

/** 插入结果：文档变更 + 变化后文档中的光标位置。 */
export interface BlockInsertSpec {
  changes: DocChange[];
  selection: { anchor: number; head: number };
  /** 插入后文档中 block 文本的起始位置（前导空行之后），供块内光标定位。 */
  blockStart: number;
}

/**
 * 计算在 `pos` 处插入块级文本 `block` 的变更与光标位置。
 *
 * @param doc   完整文档文本。
 * @param pos   插入锚点（通常为光标位置）。
 * @param block 块级内容（不含首尾空行，由本函数补齐）。
 */
export function insertBlockSpec(doc: string, pos: number, block: string): BlockInsertSpec {
  const lineStart = doc.lastIndexOf('\n', pos - 1) + 1;
  const nl = doc.indexOf('\n', pos);
  const lineEnd = nl === -1 ? doc.length : nl;
  const hasContent = doc.slice(lineStart, lineEnd).trim().length > 0;

  let at: number;
  let before: string;
  let after: string;
  if (!hasContent) {
    at = lineStart;
    // 上一行仍为非空文本时补一个空行，保证块前空行隔离（空行本身可充当隔离）
    const prevStart = lineStart >= 2 ? doc.lastIndexOf('\n', lineStart - 2) + 1 : 0;
    const prevLine = doc.slice(prevStart, lineStart - 1);
    before = lineStart > 0 && prevLine.trim().length > 0 ? '\n' : '';
    // 当前行自身的换行符保留在插入点之后，与 after 拼出块后空行
    after = '\n';
  } else if (lineEnd < doc.length) {
    at = lineEnd + 1;
    before = '\n';
    after = '\n\n';
  } else {
    at = doc.length;
    before = '\n\n';
    after = '\n\n';
  }

  const text = `${before}${block}${after}`;
  const caret = at + text.length;
  return {
    changes: [{ from: at, to: at, insert: text }],
    selection: { anchor: caret, head: caret },
    blockStart: at + before.length,
  };
}

// ===== 单元测试（守卫：仅在 vitest 环境运行，生产构建被 define 树摇）=====

if (import.meta.vitest) {
  const { describe, it, expect } = import.meta.vitest;

  function apply(doc: string, pos: number, block: string): { text: string; caret: number } {
    const spec = insertBlockSpec(doc, pos, block);
    const c = spec.changes[0];
    const text = doc.slice(0, c.from) + c.insert + doc.slice(c.to);
    return { text, caret: spec.selection.anchor };
  }

  describe('insertBlockSpec', () => {
    it('空文档：就地插入', () => {
      const r = apply('', 0, 'BLOCK');
      expect(r.text).toBe('BLOCK\n');
      expect(r.caret).toBe(6);
    });

    it('光标在空行上：就地插入，上一行非空时补前导空行', () => {
      const r = apply('AB\n\nCD', 3, 'BLOCK');
      expect(r.text).toBe('AB\n\nBLOCK\n\nCD');
    });

    it('连续空行就地插入：多余空行原样保留', () => {
      const r = apply('AB\n\n\nCD', 3, 'BLOCK');
      expect(r.text).toBe('AB\n\nBLOCK\n\n\nCD');
    });

    it('当前行有内容且非末行：另起一段并补前导空行', () => {
      const r = apply('AB\nCD', 1, 'BLOCK');
      expect(r.text).toBe('AB\n\nBLOCK\n\nCD');
    });

    it('当前行为末行：文档尾追加', () => {
      const r = apply('AB', 2, 'BLOCK');
      expect(r.text).toBe('AB\n\nBLOCK\n\n');
    });

    it('光标在行中时按整行判定，插入到该行下方', () => {
      const r = apply('AB\nCD', 4, 'BLOCK');
      expect(r.text).toBe('AB\nCD\n\nBLOCK\n\n');
    });

    it('blockStart 指向 block 文本起点（前导空行之后）', () => {
      // 'AB' 行有内容 → at=3（行尾后），before='\n' → block 从 4 开始
      const spec = insertBlockSpec('AB\nCD', 1, 'BLOCK');
      expect(spec.blockStart).toBe(4);
    });
  });
}
