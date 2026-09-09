/**
 * formatting.ts 纯函数格式化层单元测试。
 *
 * 覆盖工具栏三态交互的用户可见行为：
 *   - 有选区：剥离内部同类标记后整体包裹；整体已被包裹则取消
 *   - 无选区：光标位于标记对内 → 取消；否则插入空标记对
 *   - heading：行前缀 toggle（添加 / 移除 / 级别切换，支持 H1-H6）
 */
import { describe, expect, it } from 'vitest';
import { applyMarkdownFormat, type DocChange, type HeadingLevel } from '../formatting';

/** 将 changes（按 from 升序）应用到文档，得到结果文本。 */
function applyChanges(doc: string, changes: DocChange[]): string {
  let out = doc;
  for (let i = changes.length - 1; i >= 0; i--) {
    const { from, to, insert } = changes[i];
    out = out.slice(0, from) + insert + out.slice(to);
  }
  return out;
}

/** 便捷断言：格式化后文本与选区位置（heading 可指定 level）。 */
function expectFormat(doc: string, from: number, to: number, kind: Parameters<typeof applyMarkdownFormat>[3], expectedDoc: string, anchor: number, head: number, level?: HeadingLevel): void {
  const result = applyMarkdownFormat(doc, from, to, kind, level);
  expect(applyChanges(doc, result.changes)).toBe(expectedDoc);
  expect(result.selection).toEqual({ anchor, head });
}

describe('applyMarkdownFormat — bold', () => {
  it('选中文本时整体包裹', () => {
    // “我是天之骄子”选中“是天”(1..3) → “我**是天**之骄子”
    expectFormat('我是天之骄子', 1, 3, 'bold', '我**是天**之骄子', 3, 5);
  });

  it('再次框选含标记的更大范围时剥离内部标记后整体包裹', () => {
    // “我**是天**之骄子”框选“我**是天**之”(0..8) → “**我是天之**骄子”
    expectFormat('我**是天**之骄子', 0, 8, 'bold', '**我是天之**骄子', 2, 6);
  });

  it('选区整体已被包裹时取消包裹', () => {
    expectFormat('**bold** rest', 0, 8, 'bold', 'bold rest', 0, 4);
  });

  it('选区内多个加粗标记统一剥离后整体包裹', () => {
    // `甲**乙**丙**丁**` 共 12 字符，选区 0..12 覆盖全部
    expectFormat('甲**乙**丙**丁**', 0, 12, 'bold', '**甲乙丙丁**', 2, 6);
  });

  it('选区紧贴后续加粗区域时合并为一段（不再产生碎片标记）', () => {
    // `你好我是**Kimi**` 选中 `是` 加粗 → `你好我**是Kimi**`（而非 **是****Kimi**）
    expectFormat('你好我是**Kimi**', 3, 4, 'bold', '你好我**是Kimi**', 5, 10);
  });

  it('选区吞掉后续加粗区域的开启标记时合并为一段', () => {
    // 选中 `是**K`（含既有 `**` 开启标记）加粗 → 仍为 `你好我**是Kimi**`
    expectFormat('你好我是**Kimi**', 3, 7, 'bold', '你好我**是Kimi**', 5, 10);
  });

  it('选区覆盖后续加粗区域时合并为一段', () => {
    // 选中 `我是`（紧贴 `**Kimi**`）加粗 → `你好**我是Kimi**`
    expectFormat('你好我是**Kimi**', 2, 4, 'bold', '你好**我是Kimi**', 4, 10);
  });

  it('选区紧贴前序加粗区域时向左合并', () => {
    expectFormat('**Kimi**是', 8, 9, 'bold', '**Kimi是**', 2, 7);
  });

  it('选区位于两个加粗区域之间时合并全部', () => {
    expectFormat('**a**b**c**', 5, 6, 'bold', '**abc**', 2, 5);
  });

  it('选区整体位于既有加粗区域内时不产生多余标记', () => {
    // 选中 `**Kimi**` 内部的 `im` 加粗 → 文档不变
    expectFormat('**Kimi**', 3, 5, 'bold', '**Kimi**', 2, 6);
  });

  it('选区吞掉开启标记且未选中闭合标记时不残留不对称标记', () => {
    expectFormat('你好我是**Kimi**', 4, 7, 'bold', '你好我是**Kimi**', 6, 10);
  });

  it('光标位于加粗标记对内时取消加粗', () => {
    // “**我是天之**骄子”，光标处于“我是天之”内部(5) → “我是天之骄子”
    expectFormat('**我是天之**骄子', 5, 5, 'bold', '我是天之骄子', 3, 3);
  });

  it('光标紧贴开标记之后取消加粗', () => {
    expectFormat('**bold**', 2, 2, 'bold', 'bold', 0, 0);
  });

  it('光标紧贴关标记之前取消加粗', () => {
    expectFormat('**bold**', 6, 6, 'bold', 'bold', 4, 4);
  });

  it('光标位于开标记两个字符之间时不取消、插入空对（不产生非法位置）', () => {
    // `**bold**` 位置 1 位于两个 `*` 之间：判定不成立 → 插入空 `****` 并居中
    expectFormat('**bold**', 1, 1, 'bold', '******bold**', 3, 3);
  });

  it('光标在空加粗对中间时取消包裹', () => {
    expectFormat('****', 2, 2, 'bold', '', 0, 0);
  });

  it('光标在空加粗对的两个字符之间时不取消（不产生重叠 changes）', () => {
    // 位置 1 位于第一个 `**` 的两个字符之间：判定不成立 → 插入空 `****`
    expectFormat('****', 1, 1, 'bold', '********', 3, 3);
  });

  it('光标不在标记内时插入空标记对并居中', () => {
    expectFormat('abc', 1, 1, 'bold', 'a****bc', 3, 3);
  });

  it('光标在相邻两个加粗之间时按最近一对取消（可预测行为）', () => {
    // 朴素成对判定：光标两侧最近的 `**` 视为一对
    expectFormat('**a** **b**', 6, 6, 'bold', '**a b**', 4, 4);
  });
});

describe('applyMarkdownFormat — italic', () => {
  it('选中文本时整体包裹', () => {
    expectFormat('abc', 1, 2, 'italic', 'a*b*c', 2, 3);
  });

  it('光标位于斜体标记对内时取消斜体', () => {
    expectFormat('a*it*b', 3, 3, 'italic', 'aitb', 2, 2);
  });

  it('光标不在标记内时插入空斜体对并居中', () => {
    expectFormat('abc', 1, 1, 'italic', 'a**bc', 2, 2);
  });

  it('斜体不误判加粗标记（光标在 **bold** 内插入空斜体对）', () => {
    // 光标位于 `**bold**` 的 bold 内(4)，前后最近的独立 `*` 不存在 → 插入空对
    expectFormat('**bold**', 4, 4, 'italic', '**bo**ld**', 5, 5);
  });

  it('选区内剥离独立 `*` 时保留 `**` 加粗标记', () => {
    // `**b*i*old**`（11 字符）整体选区：首尾 `*` 属于 `**`，不算斜体包裹；
    // 剥离独立 `*` 后整体包裹 → `***biold***`（粗斜体），内部文本 9 字符
    expectFormat('**b*i*old**', 0, 11, 'italic', '***biold***', 1, 10);
  });

  it('选区紧贴后续斜体区域时合并为一段', () => {
    expectFormat('a*b*c', 0, 1, 'italic', '*ab*c', 1, 3);
  });
});

describe('applyMarkdownFormat — underline', () => {
  it('选中文本时整体包裹（++ 标记）', () => {
    expectFormat('abc', 1, 2, 'underline', 'a++b++c', 3, 4);
  });

  it('选区整体已被包裹时取消包裹', () => {
    expectFormat('++un++ rest', 0, 6, 'underline', 'un rest', 0, 2);
  });

  it('光标位于下划线标记对内时取消下划线', () => {
    expectFormat('a++un++b', 4, 4, 'underline', 'aunb', 2, 2);
  });

  it('光标不在标记内时插入空标记对并居中', () => {
    expectFormat('abc', 1, 1, 'underline', 'a++++bc', 3, 3);
  });

  it('选区紧贴后续下划线区域时合并为一段', () => {
    // `你好我是++Kimi++` 选中 `是` → `你好我++是Kimi++`（而非 ++是++++Kimi++）
    expectFormat('你好我是++Kimi++', 3, 4, 'underline', '你好我++是Kimi++', 5, 10);
  });

  it('与加粗标记互不干扰（在 **bold** 内部包裹下划线）', () => {
    expectFormat('**bold**', 2, 6, 'underline', '**++bold++**', 4, 8);
  });
});

describe('applyMarkdownFormat — inlineMath', () => {
  it('选中文本时整体包裹（$ 标记）', () => {
    expectFormat('质能方程 E=mc^2 很有名', 5, 11, 'inlineMath', '质能方程 $E=mc^2$ 很有名', 6, 12);
  });

  it('选区整体已被包裹时取消包裹', () => {
    expectFormat('$E=mc^2$ 很有名', 0, 8, 'inlineMath', 'E=mc^2 很有名', 0, 6);
  });

  it('光标位于行内公式标记对内时取消公式', () => {
    expectFormat('a $x+y$ b', 4, 4, 'inlineMath', 'a x+y b', 3, 3);
  });

  it('光标不在标记内时插入空标记对并居中', () => {
    expectFormat('abc', 1, 1, 'inlineMath', 'a$$bc', 2, 2);
  });

  it('与下划线 ++ 标记互不干扰', () => {
    expectFormat('++un++', 2, 4, 'inlineMath', '++$un$++', 3, 5);
  });
});

describe('applyMarkdownFormat — heading', () => {
  it('光标行添加 `# ` 前缀', () => {
    expectFormat('hello\nworld', 2, 2, 'heading', '# hello\nworld', 4, 4);
  });

  it('再次点击移除 `# ` 前缀', () => {
    expectFormat('# hello\nworld', 3, 3, 'heading', 'hello\nworld', 1, 1);
  });

  it('多行选区统一添加前缀', () => {
    // assoc=1：选区起点吸附到首行插入的 `# ` 之后
    expectFormat('a\nb\nc', 0, 4, 'heading', '# a\n# b\nc', 2, 8);
  });

  it('多行选区统一移除前缀', () => {
    expectFormat('# a\n# b\nc', 0, 8, 'heading', 'a\nb\nc', 0, 4);
  });

  it('带前导空白的行在前导空白后插入前缀', () => {
    expectFormat('  x', 2, 2, 'heading', '  # x', 4, 4);
  });

  it('移除前缀时保留前导空白', () => {
    expectFormat('  # x', 3, 3, 'heading', '  x', 2, 2);
  });

  it('空行插入 `# `', () => {
    expectFormat('a\n\nb', 2, 2, 'heading', 'a\n# \nb', 4, 4);
  });

  it('`#tag` 等非标题行不误判，直接添加前缀', () => {
    expectFormat('#tag', 1, 1, 'heading', '# #tag', 3, 3);
  });
});

describe('applyMarkdownFormat — heading 级别（H1-H6）', () => {
  it('光标行添加目标级别前缀（H3）', () => {
    expectFormat('hello\nworld', 2, 2, 'heading', '### hello\nworld', 6, 6, 3);
  });

  it('光标行添加 H6 前缀', () => {
    expectFormat('hello', 2, 2, 'heading', '###### hello', 9, 9, 6);
  });

  it('行已处于目标级别时再次点击移除前缀', () => {
    expectFormat('## hello\nworld', 4, 4, 'heading', 'hello\nworld', 1, 1, 2);
  });

  it('其他级别标题切换为目标级别（H1 → H3）', () => {
    expectFormat('# hello\nworld', 3, 3, 'heading', '### hello\nworld', 5, 5, 3);
  });

  it('降级切换（H3 → H1）', () => {
    expectFormat('### hello\nworld', 6, 6, 'heading', '# hello\nworld', 4, 4, 1);
  });

  it('H6 切换为 H2 时正确替换前缀长度', () => {
    expectFormat('###### hello\nworld', 9, 9, 'heading', '## hello\nworld', 5, 5, 2);
  });

  it('多行选区统一设为目标级别', () => {
    // 混合 H1/H2 → 统一 H3
    expectFormat('# a\n## b\nc', 0, 8, 'heading', '### a\n### b\nc', 4, 11, 3);
  });

  it('多行选区全部处于目标级别时统一移除', () => {
    expectFormat('## a\n## b\nc', 0, 8, 'heading', 'a\nb\nc', 0, 2, 2);
  });

  it('级别切换时保留前导空白', () => {
    expectFormat('  ## x', 5, 5, 'heading', '  # x', 4, 4, 1);
  });

  it('`#######` 七个井号不算标题，直接添加前缀', () => {
    expectFormat('####### x', 3, 3, 'heading', '# ####### x', 5, 5, 1);
  });
});
