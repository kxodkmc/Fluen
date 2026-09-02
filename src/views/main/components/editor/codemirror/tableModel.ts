/**
 * 表格纯函数层（不依赖 CodeMirror / Vue / DOM）——表格领域的唯一逻辑源。
 *
 * 职责四块：
 *   - 块生成：`buildTableBlock` 生成规范 §5.3 的 `<f-tbl>` 插入块
 *     （markdown 默认形态 B / html 拓展形态 C）
 *   - 解析：`parseMarkdownTableText` 将 lezer Table 节点文本解析为模型
 *   - 序列化：`serializeMarkdownTable` 将模型写回 GFM 文本（`\|` 转义、
 *     对齐标记保留），解析↔序列化严格往返一致
 *   - 结构操作：行列插入/删除（半预览表格手柄消费），全部返回新模型
 *
 * 模型是「单元格为已 trim 纯文本」的规范形：DOM 编辑态读写均先归一化
 * 到此形态，再比较/序列化，保证 widget 复用判定（eq）稳定。
 */

/** 表格语法形态。 */
export type TableSyntax = 'markdown' | 'html';

/** 表格对齐（来自分隔行 `:---` / `:---:` / `---:`）。 */
export type TableAlignment = 'left' | 'center' | 'right' | null;

/** GFM 表格模型（表头 + 对齐 + 数据行；单元格均为 trim 后纯文本）。 */
export interface MarkdownTableModel {
  header: string[];
  aligns: TableAlignment[];
  rows: string[][];
}

/** 列数下限：删除列时不允许删空。 */
const MIN_COLS = 1;

/* ── 块生成（插入器） ─────────────────────────────────────────────── */

/** 单元格空白行文本（表头与数据行同形，由用户填写内容）。 */
function emptyMarkdownRow(cols: number): string {
  return `| ${Array.from({ length: cols }, () => '').join(' | ')} |`;
}

/** GFM 表：表头行 + 分隔行 + rows-1 空数据行。 */
function markdownTable(rows: number, cols: number): string {
  const separator = `| ${Array.from({ length: cols }, () => '---').join(' | ')} |`;
  const body = Array.from({ length: rows - 1 }, () => emptyMarkdownRow(cols));
  return [emptyMarkdownRow(cols), separator, ...body].join('\n');
}

/** HTML 表子集：thead 表头行 + rows-1 tbody 数据行；单行表省略 tbody。 */
function htmlTable(rows: number, cols: number): string {
  const headCells = Array.from({ length: cols }, () => '<th></th>').join('');
  const bodyCells = Array.from({ length: cols }, () => '<td></td>').join('');
  const lines = ['<table>', '  <thead>', `    <tr>${headCells}</tr>`, '  </thead>'];
  if (rows > 1) {
    lines.push('  <tbody>');
    for (let i = 1; i < rows; i++) lines.push(`    <tr>${bodyCells}</tr>`);
    lines.push('  </tbody>');
  }
  lines.push('</table>');
  return lines.join('\n');
}

/**
 * 生成 `<f-tbl>` 包裹的表格块（不含尾随换行；空行隔离由插入层处理）。
 *
 * `<f-caption>` 为 linter 必需项（规范 §6.2：恰为 1），预置占位文本由用户改写。
 *
 * @param syntax  语法形态，默认 markdown。
 * @param rows    总行数（含表头），最小 1。
 * @param cols    列数，最小 1。
 * @param caption 题注占位文本。
 */
export function buildTableBlock(
  syntax: TableSyntax = 'markdown',
  rows: number,
  cols: number,
  caption = '',
): string {
  const r = Math.max(1, Math.floor(rows));
  const c = Math.max(1, Math.floor(cols));
  const body = syntax === 'html' ? htmlTable(r, c) : markdownTable(r, c);
  return `<f-tbl>\n  <f-caption>${caption}</f-caption>\n\n${body}\n\n</f-tbl>`;
}

/* ── 解析 ─────────────────────────────────────────────────────────── */

/** 切分一行表格为单元格文本（处理 `\|` 转义，两端 `|` 可省略）。 */
function splitTableRow(line: string): string[] {
  const t = line.trim().replace(/^\|/, '').replace(/\|$/, '');
  const cells: string[] = [];
  let cur = '';
  for (let i = 0; i < t.length; i++) {
    if (t[i] === '\\' && t[i + 1] === '|') {
      cur += '|';
      i += 1;
    } else if (t[i] === '|') {
      cells.push(cur.trim());
      cur = '';
    } else {
      cur += t[i];
    }
  }
  cells.push(cur.trim());
  return cells;
}

/** 解析分隔行的对齐标记；非合法分隔行返回 null。 */
function parseAlignmentRow(line: string): TableAlignment[] | null {
  const cells = splitTableRow(line);
  if (cells.length === 0) return null;
  const aligns: TableAlignment[] = [];
  for (const cell of cells) {
    const m = /^(:?)(-+)(:?)$/.exec(cell);
    if (!m) return null;
    aligns.push(m[1] && m[3] ? 'center' : m[1] ? 'left' : m[3] ? 'right' : null);
  }
  return aligns;
}

/**
 * 解析 GFM 表格文本为模型。
 *
 * 输入为 lezer `Table` 节点覆盖的文档片段；结构异常（缺分隔行等）时
 * 返回 null，装饰层据此降级为源码显示。
 */
export function parseMarkdownTableText(text: string): MarkdownTableModel | null {
  const lines = text.split('\n').filter((l) => l.trim().length > 0);
  if (lines.length < 2) return null;
  const aligns = parseAlignmentRow(lines[1]);
  if (aligns === null) return null;
  return {
    header: splitTableRow(lines[0]),
    aligns,
    rows: lines.slice(2).map(splitTableRow),
  };
}

/* ── 序列化 ───────────────────────────────────────────────────────── */

/** 序列化单元格：`\|` 转义 + 折叠换行为空格 + trim。 */
function serializeCell(text: string): string {
  return text.replace(/\s*\n\s*/g, ' ').trim().replace(/\|/g, '\\|');
}

function alignmentMarker(align: TableAlignment): string {
  if (align === 'left') return ':---';
  if (align === 'center') return ':---:';
  if (align === 'right') return '---:';
  return '---';
}

/**
 * 将模型序列化为 GFM 文本（表头 + 分隔行 + 数据行）。
 *
 * 列数以表头为准；数据行缺格补空、超格截断。与
 * {@link parseMarkdownTableText} 严格往返一致。
 */
export function serializeMarkdownTable(model: MarkdownTableModel): string {
  const cols = model.header.length;
  const rowToText = (cells: string[]): string =>
    `| ${Array.from({ length: cols }, (_, i) => serializeCell(cells[i] ?? '')).join(' | ')} |`;
  return [
    rowToText(model.header),
    `| ${model.aligns.map(alignmentMarker).join(' | ')} |`,
    ...model.rows.map(rowToText),
  ].join('\n');
}

/* ── 结构操作（半预览表格手柄） ───────────────────────────────────── */

/** 空白行（与表头等宽）。 */
function emptyModelRow(cols: number): string[] {
  return Array.from({ length: cols }, () => '');
}

function cloneModel(model: MarkdownTableModel): MarkdownTableModel {
  return {
    header: [...model.header],
    aligns: [...model.aligns],
    rows: model.rows.map((r) => [...r]),
  };
}

/**
 * 在 `index` 行前插入空行（index === rows.length 表示追加到末尾）。
 * 越界自动收敛；返回新模型。
 */
export function insertTableRow(model: MarkdownTableModel, index: number): MarkdownTableModel {
  const next = cloneModel(model);
  const at = Math.min(Math.max(index, 0), next.rows.length);
  next.rows.splice(at, 0, emptyModelRow(next.header.length));
  return next;
}

/**
 * 在 `index` 列前插入空列（index === 列数表示追加到末尾）。
 * 越界自动收敛；返回新模型。
 */
export function insertTableColumn(model: MarkdownTableModel, index: number): MarkdownTableModel {
  const next = cloneModel(model);
  const at = Math.min(Math.max(index, 0), next.header.length);
  next.header.splice(at, 0, '');
  next.aligns.splice(at, 0, null);
  for (const row of next.rows) row.splice(at, 0, '');
  return next;
}

/**
 * 删除 `index` 行。仅剩表头时也允许继续删除数据行；越界返回原模型。
 */
export function deleteTableRow(model: MarkdownTableModel, index: number): MarkdownTableModel {
  if (index < 0 || index >= model.rows.length) return model;
  const next = cloneModel(model);
  next.rows.splice(index, 1);
  return next;
}

/**
 * 删除 `index` 列。至少保留 {@link MIN_COLS} 列，否则返回原模型。
 */
export function deleteTableColumn(model: MarkdownTableModel, index: number): MarkdownTableModel {
  if (index < 0 || index >= model.header.length || model.header.length <= MIN_COLS) return model;
  const next = cloneModel(model);
  next.header.splice(index, 1);
  next.aligns.splice(index, 1);
  for (const row of next.rows) row.splice(index, 1);
  return next;
}

// ===== 单元测试（守卫：仅在 vitest 环境运行，生产构建被 define 树摇）=====

if (import.meta.vitest) {
  const { describe, it, expect } = import.meta.vitest;

  describe('buildTableBlock — markdown（默认）', () => {
    it('2 行 3 列：f-tbl 包裹 + caption + 空行隔离 + 表头/分隔/数据行', () => {
      expect(buildTableBlock('markdown', 2, 3, '题注')).toBe(
        [
          '<f-tbl>',
          '  <f-caption>题注</f-caption>',
          '',
          '|  |  |  |',
          '| --- | --- | --- |',
          '|  |  |  |',
          '',
          '</f-tbl>',
        ].join('\n'),
      );
    });

    it('非法行列数收敛为 1×1', () => {
      const block = buildTableBlock('markdown', 0, -3, '题注');
      expect(block).toContain('|  |');
      expect(block).toContain('| --- |');
    });
  });

  describe('buildTableBlock — html（拓展）', () => {
    it('thead + tbody 结构，单行表省略 tbody', () => {
      expect(buildTableBlock('html', 1, 1, '题注')).toBe(
        ['<f-tbl>', '  <f-caption>题注</f-caption>', '', '<table>', '  <thead>', '    <tr><th></th></tr>', '  </thead>', '</table>', '', '</f-tbl>'].join('\n'),
      );
      expect(buildTableBlock('html', 2, 2, '题注')).toContain('<tbody>');
    });

    it('仅使用后端白名单标签', () => {
      const block = buildTableBlock('html', 2, 2, '题注');
      for (const tag of ['table', 'thead', 'tbody', 'tr', 'th', 'td']) {
        expect(block).toContain(`<${tag}`);
      }
    });
  });

  describe('parse / serialize 往返', () => {
    it('基本解析：表头 / 对齐 / 数据行', () => {
      const m = parseMarkdownTableText('| A | B |\n| :--- | ---: |\n| 1 | 2 |');
      expect(m).toEqual({ header: ['A', 'B'], aligns: ['left', 'right'], rows: [['1', '2']] });
    });

    it('空单元格正常解析（插入器生成的初始表）', () => {
      const m = parseMarkdownTableText('|  |  |\n| --- | --- |\n|  |  |');
      expect(m).toEqual({ header: ['', ''], aligns: [null, null], rows: [['', '']] });
    });

    it('第二行非分隔行返回 null（装饰层降级为源码）', () => {
      expect(parseMarkdownTableText('| A |\n| x |\n| y |')).toBeNull();
      expect(parseMarkdownTableText('| A |')).toBeNull();
    });

    it('serialize → parse 往返一致（含转义与对齐）', () => {
      const model = { header: ['a | b', 'C'], aligns: ['center' as const, null], rows: [['x', 'y'], ['', '']] };
      const round = parseMarkdownTableText(serializeMarkdownTable(model));
      expect(round).toEqual(model);
    });

    it('序列化转义竖线、折叠单元格内换行', () => {
      const text = serializeMarkdownTable({ header: ['a|b'], aligns: [null], rows: [['l1\nl2']] });
      expect(text).toBe('| a\\|b |\n| --- |\n| l1 l2 |');
    });

    it('数据行缺格补空、超格截断（以表头列数为准）', () => {
      const text = serializeMarkdownTable({ header: ['A', 'B'], aligns: [null, null], rows: [['x'], ['1', '2', '3']] });
      const round = parseMarkdownTableText(text);
      expect(round?.rows).toEqual([['x', ''], ['1', '2']]);
    });
  });

  describe('结构操作', () => {
    const base = { header: ['A', 'B'], aligns: ['left' as const, null], rows: [['1', '2']] };

    it('插入行：指定位置插入空行，支持末尾追加', () => {
      expect(insertTableRow(base, 0).rows).toEqual([['', ''], ['1', '2']]);
      expect(insertTableRow(base, 5).rows).toEqual([['1', '2'], ['', '']]);
    });

    it('插入列：表头/对齐/数据行同步插入', () => {
      const next = insertTableColumn(base, 1);
      expect(next.header).toEqual(['A', '', 'B']);
      expect(next.aligns).toEqual(['left', null, null]);
      expect(next.rows).toEqual([['1', '', '2']]);
    });

    it('删除行：越界返回原模型', () => {
      expect(deleteTableRow(base, 0).rows).toEqual([]);
      expect(deleteTableRow(base, 3)).toBe(base);
    });

    it('删除列：保留至少一列，越界返回原模型', () => {
      expect(deleteTableColumn(base, 0).header).toEqual(['B']);
      expect(deleteTableColumn(base, 5)).toBe(base);
      const single = deleteTableColumn(base, 0);
      expect(deleteTableColumn(single, 0)).toBe(single);
    });

    it('行列操作不修改原模型（纯函数）', () => {
      insertTableRow(insertTableColumn(base, 1), 0);
      expect(base.rows).toEqual([['1', '2']]);
      expect(base.header).toEqual(['A', 'B']);
    });
  });
}
