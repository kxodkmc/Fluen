/**
 * 块级显示公式（`$$...$$`）的行扫描定位。
 *
 * 为什么不走语法树：`$$` 围栏可跨任意行，用 BlockParser 做任意前瞻会引入
 * 行推进状态机风险（见 footnoteSyntax 的续行语义差异）。行扫描是纯函数、
 * 线性推进、带硬上限，稳定性可证且完全可测。
 *
 * 识别规则：
 *   - 单行式：一行（trim 后）同时以 `$$` 开头并以 `$$` 结尾，且中间内容非空
 *   - 多行围栏：某行以 `$$` 开头，向后扫描至以 `$$` 结尾的行；扫描上限
 *     MAX_MATH_BLOCK_LINES 行，未找到闭栏则该开栏按普通文本处理
 *   - 跳过 fenced code 区域内的 `$$`（调用方传入排除区间）
 */

/** 前瞻闭栏的硬上限（行数）。超限视为未闭合，保持原文。 */
export const MAX_MATH_BLOCK_LINES = 1000;

/** 一个显示公式区域（文档绝对偏移区间，含两侧定界符与内部换行）。 */
export interface MathRegion {
  from: number;
  to: number;
}

/**
 * 在文档中定位所有显示公式区域。
 *
 * @param doc          CM6 文档文本。
 * @param excluded     需要排除的区间列表（如 fenced code 区域），须按 from 升序。
 * @returns 按 from 升序排列且互不重叠的区域数组。
 */
export function scanMathRegions(doc: { lines: number; line(n: number): { from: number; to: number; text: string } }, excluded: ReadonlyArray<{ from: number; to: number }>): MathRegion[] {
  const regions: MathRegion[] = [];
  for (let i = 1; i <= doc.lines; i++) {
    const openLine = doc.line(i);
    if (!isMathFenceOpen(openLine.text)) continue;
    if (inExcluded(openLine.from, excluded)) continue;

    // 空式 `$$$$` 不处理
    const trimmed = openLine.text.trim();
    if (trimmed === '$$$$' || trimmed === '$$ $$') continue;

    // 单行闭合
    if (trimmed.length > 4 && trimmed.endsWith('$$')) {
      regions.push({ from: openLine.from, to: openLine.to });
      continue;
    }

    // 多行围栏：前瞻闭栏（peek 式读取，不产生副作用）
    let closedAt = -1;
    const limit = Math.min(i + MAX_MATH_BLOCK_LINES, doc.lines);
    for (let j = i + 1; j <= limit; j++) {
      if (/\$\$\s*$/.test(doc.line(j).text)) {
        closedAt = j;
        break;
      }
    }
    if (closedAt === -1) continue; // 未闭合 → 保持原文

    // 跳过整个已确认区域（含排除检查）
    if (inExcluded(doc.line(closedAt).from, excluded)) continue;
    regions.push({ from: openLine.from, to: doc.line(closedAt).to });
    i = closedAt; // 外层 for 继续 i++ 会落到闭栏的下一行
  }
  return regions;
}

/** 判定一行是否以 `$$` 打开（trim 后）。 */
export function isMathFenceOpen(text: string): boolean {
  return text.trimStart().startsWith('$$');
}

/** 偏移是否落在升序排除区间内（二分查找）。 */
function inExcluded(pos: number, excluded: ReadonlyArray<{ from: number; to: number }>): boolean {
  let lo = 0;
  let hi = excluded.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const r = excluded[mid]!;
    if (pos < r.from) hi = mid - 1;
    else if (pos >= r.to) lo = mid + 1;
    else return true;
  }
  return false;
}

// ===== 单元测试（守卫：仅在 vitest 环境运行，生产构建被 define 树摇）=====

if (import.meta.vitest) {
  const { describe, it, expect } = import.meta.vitest;
  const { EditorState, Text } = await import('@codemirror/state');

  function makeDoc(md: string) {
    return Text.of(md.split('\n'));
  }

  describe('mathDisplayScan: 区域识别', () => {
    it('单行 $$...$$ 识别', () => {
      const doc = makeDoc('前置\n$$E=mc^2$$\n后置');
      const regions = scanMathRegions(doc, []);
      expect(regions.length).toBe(1);
      expect(regions[0]).toEqual({ from: 3, to: 13 });
    });

    it('多行围栏识别并合并为单一区域', () => {
      const md = '$$\nx=1\ny=2\n$$\n尾部';
      const doc = makeDoc(md);
      const regions = scanMathRegions(doc, []);
      expect(regions.length).toBe(1);
      expect(md.slice(regions[0]!.from, regions[0]!.to)).toBe('$$\nx=1\ny=2\n$$');
    });

    it('连续两个多行块分别识别', () => {
      const md = '$$\na\n$$\n中段\n$$\nb\n$$';
      const doc = makeDoc(md);
      const regions = scanMathRegions(doc, []);
      expect(regions.length).toBe(2);
      expect(md.slice(regions[0]!.from, regions[0]!.to)).toBe('$$\na\n$$');
      expect(md.slice(regions[1]!.from, regions[1]!.to)).toBe('$$\nb\n$$');
    });

    it('未闭合的 $$ 按普通文本处理', () => {
      const doc = makeDoc('$$\n无闭合段落\n继续更多文本');
      expect(scanMathRegions(doc, [])).toEqual([]);
    });

    it('空式 $$$$ 不产生区域', () => {
      const doc = makeDoc('$$$$\n正常段');
      expect(scanMathRegions(doc, [])).toEqual([]);
    });
  });

  describe('mathDisplayScan: 排除区间', () => {
    it('fenced code 内的 $$ 被跳过', () => {
      const md = '正文前\n```\n$$未渲染区$$\n```\n$$真公式$$';
      const state = EditorState.create({ doc: md });
      // 模拟装饰主流程先收集的 fenced 区间：第 6..14 字符即 ``` … ``` 行内容段
      const fenceFrom = md.indexOf('```');
      const fenceTo = md.lastIndexOf('```') + 3;
      const regions = scanMathRegions(state.doc, [{ from: fenceFrom + 4, to: fenceTo }]);
      expect(regions.length).toBe(1);
      expect(md.slice(regions[0]!.from, regions[0]!.to)).toBe('$$真公式$$');
    });
  });
}
