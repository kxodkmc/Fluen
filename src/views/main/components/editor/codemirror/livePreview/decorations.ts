/**
 * 半预览装饰构建核心（纯函数，不持有任何编辑器状态）。
 *
 * 架构：三遍式纯计算
 *   ① 收集代码块区间（FencedCode / 缩进代码块）——作为数学扫描的排除集
 *   ② 块级显示公式行扫描（mathDisplayScan）
 *   ③ 主树遍历：规则注册表按节点名派发，产出 mark / line / replace 装饰，
 *     并将显示公式区域替换为 KaTeX 块级 widget
 *
 * 稳定性约定：
 *   - 每条规则的执行被独立 try/catch 包裹，单规则故障降级为该段显示原文，
 *     绝不影响编辑器整体可用性
 *   - 「文档文本永不被改动」由构造保证：本文件只产出 Decoration，从不产生变更
 *   - 所有 range 集合经 Decoration.set(..., true) 排序，满足 RangeSet 有序约束
 *
 * 揭示语义（与 Obsidian 一致）：
 *   - 光标选区与元素区间相交/紧邻 → 显示原始 markdown（可编辑形态）
 *   - 不相交 → 隐藏语法标记、渲染排版效果；replace 区间注册原子性，
 *     光标自动落到边界，不会卡进零宽区
 */

import { Decoration } from '@codemirror/view';
import { type Range, type Text } from '@codemirror/state';
import type { DecorationSet } from '@codemirror/view';
import type { SyntaxNode, Tree } from '@lezer/common';
import { BulletWidget, MathWidget, TableWidget, TaskCheckboxWidget } from './widgets';
import { parseMarkdownTableText } from '../tableModel';
import { scanMathRegions, type MathRegion } from './mathDisplayScan';

// ── 装饰模板（模块级复用；Decoration 实例不可变可共享） ─────────────

const decoBold = Decoration.mark({ class: 'fluen-lp-strong' });
const decoItalic = Decoration.mark({ class: 'fluen-lp-em' });
const decoStrike = Decoration.mark({ class: 'fluen-lp-strike' });
const decoUnderline = Decoration.mark({ class: 'fluen-lp-underline' });
const decoInlineCode = Decoration.mark({ class: 'fluen-lp-code-inline' });

const lineHeading: ReadonlyArray<Decoration | null> = [
  null,
  Decoration.line({ class: 'fluen-lp-line fluen-lp-h1' }),
  Decoration.line({ class: 'fluen-lp-line fluen-lp-h2' }),
  Decoration.line({ class: 'fluen-lp-line fluen-lp-h3' }),
  Decoration.line({ class: 'fluen-lp-line fluen-lp-h4' }),
  Decoration.line({ class: 'fluen-lp-line fluen-lp-h5' }),
  Decoration.line({ class: 'fluen-lp-line fluen-lp-h6' }),
];
const lineQuote = Decoration.line({ class: 'fluen-lp-quote' });
const lineFence = Decoration.line({ class: 'fluen-lp-fence' });
const lineTaskDone = Decoration.line({ class: 'fluen-lp-task-done' });
const lineMathSrc = Decoration.line({ class: 'fluen-lp-math-src' });
const lineHr = Decoration.line({ class: 'fluen-lp-hr' });

/** 隐藏语法的零宽替换。 */
function hide(): Decoration {
  return Decoration.replace({});
}

/** 链接标记（携带 data-fluen-href 供点击处理读取）。 */
function linkDeco(href: string, title?: string): Decoration {
  const attrs: Record<string, string> = { 'data-fluen-href': href };
  if (title) attrs.title = title;
  return Decoration.mark({ class: 'fluen-lp-link', attributes: attrs });
}

/** 图片 alt 占位标记（hover 提示真实路径）。 */
function imageAltDeco(src: string): Decoration {
  return Decoration.mark({ class: 'fluen-lp-img-alt', attributes: { title: src } });
}

// ── 类型与上下文 ─────────────────────────────────────────────────────

/** 单次构建产出。 */
export interface DecorationBuildResult {
  /** 全部视觉装饰。 */
  decorations: DecorationSet;
  /** 其中 replace/widget 区间子集（注册到 atomicRanges 用）。 */
  atomicRanges: DecorationSet;
}

interface Acc {
  deco: Array<Range<Decoration>>;
  atomic: Array<Range<Decoration>>;
}

export interface SelInfo {
  from: number;
  to: number;
}

interface RuleContext {
  doc: Text;
  sel: SelInfo;
  acc: Acc;
  /** 当前是否处于代码块内（内部内容不做行内渲染）。 */
  inFence: boolean;
  /** 列表嵌套深度。 */
  listDepth: number;
  /** 已挂过行类名的行号集合（防重复）。 */
  seenLines: Set<number>;
  /** 已确认的显示公式区域（升序、互不重叠）。 */
  mathRegions: readonly MathRegion[];
}

type NodeRule = (node: SyntaxNode, cx: RuleContext) => void;

/**
 * 规则注册表：新增渲染能力只需在此追加一条规则。
 * （如未来支持图片内联缩略图，追加 Image 规则即可，无需触碰其他逻辑。）
 */
const RULES: Record<string, NodeRule[]> = {};

function addRule(name: string, fn: NodeRule): void {
  (RULES[name] ??= []).push(fn);
}

// ── 工具 ────────────────────────────────────────────────────────────

/**
 * 选区是否「揭示」区间：选区与 [from-1, to+1] 相交即视为触及。
 * ±1 的余量覆盖原子区间边缘停靠的光标位置。
 */
export function selectionTouches(from: number, to: number, sel: SelInfo): boolean {
  return sel.to >= Math.max(0, from - 1) && sel.from <= to + 1;
}

/** HTML 属性转义。 */
export function escapeHtmlAttr(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

/** 推送一个隐藏替换并同时登记原子性。 */
function pushHide(cx: RuleContext, from: number, to: number): void {
  if (to <= from) return;
  const d = hide();
  cx.acc.deco.push(d.range(from, to));
  cx.acc.atomic.push(d.range(from, to));
}

/** 对覆盖的每一行去重地执行回调。 */
function eachLine(cx: RuleContext, from: number, to: number, fn: (line: ReturnType<Text['line']>) => void): void {
  let pos = from;
  while (pos <= to) {
    const line = cx.doc.lineAt(pos);
    if (!cx.seenLines.has(line.number)) {
      cx.seenLines.add(line.number);
      fn(line);
    }
    if (line.to >= to) break;
    pos = line.to + 1;
  }
}

/** 读取链接标题并转义。 */
function readTitle(doc: Text, node: SyntaxNode | null): string | undefined {
  if (!node) return undefined;
  const raw = doc.sliceString(node.from, node.to).trim();
  const unquoted = raw.length >= 2 ? raw.slice(1, -1) : raw;
  return unquoted || undefined;
}

// ── 元素规则 ────────────────────────────────────────────────────────

for (let level = 1 as const, cap = 6 as const; level <= cap; level++) {
  addRule(`ATXHeading${level}`, (node, cx) => {
    const lineDeco = lineHeading[level];
    if (lineDeco) {
      const line = cx.doc.lineAt(node.from);
      cx.acc.deco.push(lineDeco.range(line.from, line.from));
    }
    const mark = node.getChild('HeaderMark');
    if (!mark) return;

    // 内容起点：跳过 # 记号后的空白（一并隐藏）
    let contentStart = mark.to;
    while (contentStart < node.to) {
      const ch = cx.doc.sliceString(contentStart, contentStart + 1);
      if (ch !== ' ' && ch !== '\t') break;
      contentStart++;
    }

    // 光标位于该行 → 保持原始形态（便于改级数）；「空标题」永不隐藏
    const headLine = cx.doc.lineAt(mark.from);
    const cursorOnLine = cx.sel.from <= headLine.to && cx.sel.to >= headLine.from;
    if (cursorOnLine || contentStart === mark.to) return;

    pushHide(cx, mark.from, contentStart);
  });
}

/** 强调族：粗体/斜体/删除线/下划线/上标/下标。 */
const EMPHASIS_FAMILIES: ReadonlyArray<{ node: string; deco: Decoration; mark: string }> = [
  { node: 'StrongEmphasis', deco: decoBold, mark: 'EmphasisMark' },
  { node: 'Emphasis', deco: decoItalic, mark: 'EmphasisMark' },
  { node: 'Strikethrough', deco: decoStrike, mark: 'StrikethroughMark' },
  { node: 'Underline', deco: decoUnderline, mark: 'UnderlineMark' },
  { node: 'Superscript', deco: decoItalic, mark: 'SuperscriptMark' },
  { node: 'Subscript', deco: decoItalic, mark: 'SubscriptMark' },
];

for (const fam of EMPHASIS_FAMILIES) {
  addRule(fam.node, (node, cx) => {
    cx.acc.deco.push(fam.deco.range(node.from, node.to));
    // 选区触及整段 → 原始形态（** 可编辑）
    if (selectionTouches(node.from, node.to, cx.sel)) return;
    for (
      let child: SyntaxNode | null = node.firstChild;
      child;
      child = child.nextSibling
    ) {
      if (child.name === fam.mark) {
        pushHide(cx, child.from, child.to);
      }
    }
  });
}

/** 行内代码：隐藏反引号、套底色。 */
addRule('InlineCode', (node, cx) => {
  cx.acc.deco.push(decoInlineCode.range(node.from, node.to));
  if (selectionTouches(node.from, node.to, cx.sel)) return;
  for (
    let child: SyntaxNode | null = node.firstChild;
    child;
    child = child.nextSibling
  ) {
    if (child.name === 'CodeMark') {
      pushHide(cx, child.from, child.to);
    }
  }
});

/** 行内链接 `[text](url)`：隐藏语法与地址，保留可点击文字。 */
addRule('Link', (node, cx) => {
  const url = node.getChild('URL');
  if (!url) return; // 引用式链接保持原样
  if (selectionTouches(node.from, node.to, cx.sel)) return;

  const href = escapeHtmlAttr(cx.doc.sliceString(url.from, url.to).trim());
  cx.acc.deco.push(
    linkDeco(href, readTitle(cx.doc, node.getChild('LinkTitle'))).range(node.from, node.to),
  );

  for (
    let child: SyntaxNode | null = node.firstChild;
    child;
    child = child.nextSibling
  ) {
    if (child.name === 'LinkMark') {
      pushHide(cx, child.from, child.to);
    } else if (child === url || (child.from === url.from && child.to === url.to)) {
      pushHide(cx, child.from, child.to);
    }
  }
});

/** 自动链接 `<https://…>`：整体着色可点击。 */
addRule('Autolink', (node, cx) => {
  const raw = cx.doc.sliceString(node.from, node.to);
  const href = raw.startsWith('<') && raw.endsWith('>') ? raw.slice(1, -1) : raw;
  if (!href) return;
  cx.acc.deco.push(linkDeco(escapeHtmlAttr(href)).range(node.from, node.to));
});

/** 图片 `![alt](src)`：露出 alt 文本 + hover 路径提示（不加载图片本身）。 */
addRule('Image', (node, cx) => {
  const url = node.getChild('URL');
  if (!url) return;
  if (selectionTouches(node.from, node.to, cx.sel)) return;

  const marks: SyntaxNode[] = [];
  for (
    let child: SyntaxNode | null = node.firstChild;
    child;
    child = child.nextSibling
  ) {
    if (child.name === 'LinkMark') marks.push(child);
  }
  // alt 文本区域：首标记之后至 URL 或次标记之前
  const first = marks[0];
  if (first) {
    const altEndCandidates = [url.from, ...marks.slice(1).map((m) => m.from)].filter((p) => p > first.to);
    const altEnd = Math.min(...altEndCandidates, node.to - 1);
    if (altEnd > first.to) {
      const src = escapeHtmlAttr(cx.doc.sliceString(url.from, url.to).trim());
      cx.acc.deco.push(imageAltDeco(src).range(first.to, altEnd));
    }
  }
  for (const m of marks) pushHide(cx, m.from, m.to);
  pushHide(cx, url.from, url.to);
});

/** 转义字符 `\x`：光标不在其中时隐藏反斜杠。 */
addRule('Escape', (node, cx) => {
  if (selectionTouches(node.from, node.to, cx.sel)) return;
  if (node.to > node.from) {
    pushHide(cx, node.from, node.from + 1);
  }
});

/** 引用块：行样式 + 隐藏 `>`（本块未被光标触及时）。 */
addRule('Blockquote', (node, cx) => {
  eachLine(cx, node.from, node.to, (line) => {
    cx.acc.deco.push(lineQuote.range(line.from, line.from));
  });
  if (selectionTouches(node.from, node.to, cx.sel)) return;
  for (
    let child: SyntaxNode | null = node.firstChild;
    child;
    child = child.nextSibling
  ) {
    if (child.name === 'QuoteMark') {
      pushHide(cx, child.from, child.to);
    }
  }
});

/** 任务列表标记：widget 复选框 + 已完成淡显。 */
addRule('TaskMarker', (node, cx) => {
  const checked = /\[[xX]\]/.test(cx.doc.sliceString(node.from, node.to));
  if (checked) {
    const line = cx.doc.lineAt(node.from);
    cx.acc.deco.push(lineTaskDone.range(line.from, line.from));
  }
  if (selectionTouches(node.from, node.to, cx.sel)) return;
  const d = Decoration.replace({ widget: new TaskCheckboxWidget(checked) });
  cx.acc.deco.push(d.range(node.from, node.to));
  cx.acc.atomic.push(d.range(node.from, node.to));
});

/** 水平分割线：渲染为细线（光标在该行时显示 `---`）。 */
addRule('HorizontalRule', (node, cx) => {
  if (selectionTouches(node.from, node.to, cx.sel)) return;
  const line = cx.doc.lineAt(node.from);
  cx.acc.deco.push(lineHr.range(line.from, line.from));
});

/**
 * 表格：整体替换为可交互表格 widget（原子块，永不揭示源码）。
 *
 * 覆盖两种来源的同一渲染规则：
 *   - `Table`：裸 GFM 表（f-tbl 之外的普通 Markdown 表）
 *   - `FTagTable`：`<f-tbl>` 内嵌 MD 表（ftagSyntax 的块解析器接管后，
 *     内嵌表由 tableStructure.ts 产出 FTagTable 结构节点）
 *
 * 单元格编辑与行列操作在渲染态完成，由插件层写回文档（见 plugin.ts）；
 * 源码形态仅在仅源码视图出现。解析失败（结构异常）时降级为源码显示。
 */
function renderTableWidget(node: SyntaxNode, cx: RuleContext): void {
  const model = parseMarkdownTableText(cx.doc.sliceString(node.from, node.to));
  if (!model) return;
  const d = Decoration.replace({ widget: new TableWidget(model), block: true });
  cx.acc.deco.push(d.range(node.from, node.to));
  cx.acc.atomic.push(d.range(node.from, node.to));
}

addRule('Table', renderTableWidget);
addRule('FTagTable', renderTableWidget);

/** 围栏代码块：区域行样式（含语言标签高亮由 CSS 处理）。 */
addRule('FencedCode', (node, cx) => {
  eachLine(cx, node.from, node.to, (line) => {
    cx.acc.deco.push(lineFence.range(line.from, line.from));
  });
});

/** 无序列表符 → 圆点 widget（有序列表保持原样）。 */
addRule('ListMark', (node, cx) => {
  const raw = cx.doc.sliceString(node.from, node.to);
  if (!/^[-*+]$/.test(raw)) return;
  if (selectionTouches(node.from, node.to, cx.sel)) return;
  const d = Decoration.replace({ widget: new BulletWidget(Math.min(cx.listDepth, 2)) });
  cx.acc.deco.push(d.range(node.from, node.to));
  cx.acc.atomic.push(d.range(node.from, node.to));
});

/** 行内数学：KaTeX 渲染（揭示时显示原始 TeX）。 */
addRule('InlineMath', (node, cx) => {
  const tex = cx.doc.sliceString(node.from + 1, node.to - 1);
  if (!tex.trim()) return;
  if (selectionTouches(node.from, node.to, cx.sel)) return;
  const d = Decoration.replace({ widget: new MathWidget(tex, false) });
  cx.acc.deco.push(d.range(node.from, node.to));
  cx.acc.atomic.push(d.range(node.from, node.to));
});

// ── 主入口 ──────────────────────────────────────────────────────────

/**
 * 构建半预览装饰。
 *
 * @param doc  文档文本。
 * @param tree markdown 语法树（lang-markdown 解析）。
 * @param sel  主选区。
 */
export function buildLivePreviewDecorations(
  doc: Text,
  tree: Tree,
  sel: SelInfo,
): DecorationBuildResult {
  // ① 代码块区间收集（排除集）
  const fenceSpans: Array<{ from: number; to: number }> = [];
  collectByNames(tree, ['FencedCode', 'CodeBlock'], (n) => {
    fenceSpans.push({ from: n.from, to: n.to });
  });

  // ② 块级显示公式扫描
  const mathRegions = scanMathRegions(doc, fenceSpans);

  // ③ 主树遍历 + 公式 widget 化
  const cx: RuleContext = {
    doc,
    sel,
    acc: { deco: [], atomic: [] },
    inFence: false,
    listDepth: 0,
    seenLines: new Set<number>(),
    mathRegions,
  };
  visitChildren(tree.topNode, cx);
  emitMathWidgets(mathRegions, cx);

  return {
    decorations: Decoration.set(cx.acc.deco, true),
    atomicRanges: Decoration.set(cx.acc.atomic, true),
  };
}

/** 深度优先派发；维护 inFence / listDepth 栈态，规则执行彼此隔离。 */
function visitChildren(parent: SyntaxNode, cx: RuleContext): void {
  for (
    let node: SyntaxNode | null = parent.firstChild;
    node;
    node = node.nextSibling
  ) {
    dispatch(node, cx);
  }
}

function dispatch(node: SyntaxNode, cx: RuleContext): void {
  const savedFence = cx.inFence;
  const savedDepth = cx.listDepth;

  if (node.name === 'BulletList' || node.name === 'OrderedList') cx.listDepth++;

  // 先派发节点自身规则（保证 FencedCode 的行样式能挂上），再进入其内部作用域
  if (!cx.inFence && !isInsideAnyRegion(node, cx.mathRegions) && RULES[node.name]) {
    for (const rule of RULES[node.name]) {
      try {
        rule(node, cx);
      } catch {
        /* 故障隔离：单条规则失败降级为原文显示 */
      }
    }
  }

  if (node.name === 'FencedCode' || node.name === 'CodeBlock') cx.inFence = true;

  visitChildren(node, cx);

  cx.inFence = savedFence;
  cx.listDepth = savedDepth;
}

/** 节点是否完全落入某个显示公式区域内（避免与其 widget 替换重叠）。 */
function isInsideAnyRegion(node: SyntaxNode, regions: readonly MathRegion[]): boolean {
  return regions.some((r) => node.from >= r.from && node.to <= r.to);
}

/** 将显示公式区域替换为 KaTeX 块级 widget；揭示状态下仅铺底色提示边界。 */
function emitMathWidgets(regions: readonly MathRegion[], cx: RuleContext): void {
  for (const r of regions) {
    if (selectionTouches(r.from, r.to, cx.sel)) {
      eachLine(cx, r.from, r.to, (line) => {
        cx.acc.deco.push(lineMathSrc.range(line.from, line.from));
      });
      continue;
    }
    const tex = extractDisplayTex(cx.doc.sliceString(r.from, r.to));
    if (!tex.trim()) continue;
    const d = Decoration.replace({ widget: new MathWidget(tex, true), block: true });
    cx.acc.deco.push(d.range(r.from, r.to));
    cx.acc.atomic.push(d.range(r.from, r.to));
  }
}

/** 从 `$$...$$` 全文提取 LaTeX 内容（剥离两侧定界符）。 */
function extractDisplayTex(full: string): string {
  let s = full.trim();
  if (s.startsWith('$$')) s = s.slice(2);
  const closeIdx = s.lastIndexOf('$$');
  if (closeIdx !== -1) s = s.slice(0, closeIdx);
  return s.trim();
}

/** 第一遍扫描：按名字收集树节点（不下钻命中节点内部）。 */
function collectByNames(
  tree: Tree,
  names: readonly string[],
  fn: (n: SyntaxNode) => void,
): void {
  const wanted = new Set(names);
  const visit = (parent: SyntaxNode): void => {
    for (
      let node: SyntaxNode | null = parent.firstChild;
      node;
      node = node.nextSibling
    ) {
      if (wanted.has(node.name)) {
        fn(node);
        continue;
      }
      visit(node);
    }
  };
  visit(tree.topNode);
}

/* istanbul ignore file -- 视图联动在 plugin 层测；纯函数逻辑均在本文件内测 */

// ===== 单元测试（守卫：仅在 vitest 环境运行，生产构建被 define 树摇） =====

if (import.meta.vitest) {
  const { describe, it, expect } = import.meta.vitest;
  const { EditorState } = await import('@codemirror/state');
  const { syntaxTree } = await import('@codemirror/language');
  const { markdown, markdownLanguage } = await import('@codemirror/lang-markdown');
  const { fluenMathExtension } = await import('./mathSyntax');
  const { ftagExtension } = await import('../ftagSyntax');
  const { underlineExtension } = await import('../underlineSyntax');

  function setup(md: string) {
    const state = EditorState.create({
      doc: md,
      extensions: [
        // 与生产 setup.ts 完全一致的方言组合（GFM base + f-标签/下划线/数学），
        // 确保 f-tbl 块解析器认领后规则仍覆盖真实语法树
        markdown({
          base: markdownLanguage,
          extensions: [ftagExtension, underlineExtension, fluenMathExtension],
          addKeymap: false,
        }),
      ],
    });
    return { state, md, tree: syntaxTree(state) };
  }

  function build(md: string, sel?: [number, number]): DecorationBuildResult {
    const { state, tree } = setup(md);
    return buildLivePreviewDecorations(
      state.doc,
      tree,
      sel ? { from: sel[0], to: sel[1] } : { from: state.doc.length, to: state.doc.length },
    );
  }

  interface FoundRange {
    from: number;
    to: number;
    cls?: string;
    widget?: boolean;
  }

  function flatten(r: DecorationBuildResult): FoundRange[] {
    const out: FoundRange[] = [];
    const iter = r.decorations.iter();
    while (iter.value) {
      const spec = iter.value.spec as Record<string, unknown>;
      out.push({
        from: iter.from,
        to: iter.to,
        cls: typeof spec.class === 'string' ? spec.class : undefined,
        widget: !!spec.widget,
      });
      iter.next();
    }
    return out;
  }

  describe('decorations: 标题', () => {
    it('远端光标：# 被隐藏且行级类名就位', () => {
      const md = '# 大标题\n\n正文段落。\n\n## 二级标题';
      const r = build(md); // 默认光标在文档尾 → 全部渲染态
      const flat = flatten(r);
      expect(flat.some((d) => d.cls?.includes('fluen-lp-h1'))).toBe(true);
      expect(flat.some((d) => d.cls?.includes('fluen-lp-h2'))).toBe(true);
      // # 记号与其后空白被零宽隐藏：存在无类名、非 widget 的 replace 区间
      expect(flat.some((d) => !d.widget && d.cls === undefined && d.from < d.to)).toBe(true);
    });

    it('光标在标题行上时不隐藏记号', () => {
      const md = '# 大标题\n正文';
      const pos = md.indexOf('大标题');
      const r = build(md, [pos, pos]);
      const flat = flatten(r);
      expect(flat.some((d) => d.cls?.includes('fluen-lp-h1'))).toBe(true);
      // 标题行内没有零宽隐藏（无 replace into atomic except none）
      const atomicIter = r.atomicRanges.iter();
      expect(atomicIter.value).toBeNull();
    });

    it('空标题（纯 ### 行）不隐藏任何东西', () => {
      const md = '###\n正文';
      const r = build(md);
      expect(flatten(r).filter((d) => d.cls === undefined && d.widget).length).toBe(0);
    });
  });

  describe('decorations: 强调', () => {
    it('粗体隐藏 **，光标进入后揭示', () => {
      const md = '**加粗词**\n\n普通文本段落';
      const outside = build(md);
      expect(countAtomic(outside)).toBe(2); // 前后各一组

      const insidePos = md.indexOf('加粗');
      const revealed = build(md, [insidePos, insidePos]);
      expect(countAtomic(revealed)).toBe(0);
    });

    it('斜体与删除线的标记数量正确', () => {
      const md = '*斜体* ~~删除~~\n\n普通文本段落';
      const r = build(md);
      expect(countAtomic(r)).toBe(4);
      const flat = flatten(r);
      expect(flat.some((d) => d.cls === 'fluen-lp-em')).toBe(true);
      expect(flat.some((d) => d.cls === 'fluen-lp-strike')).toBe(true);
    });

    it('下划线渲染为 underline 装饰并隐藏 ++ 记号', () => {
      const md = '++下划++ 文本\n\n普通文本段落';
      const r = build(md);
      const flat = flatten(r);
      expect(flat.some((d) => d.cls === 'fluen-lp-underline')).toBe(true);
      expect(countAtomic(r)).toBe(2); // 两侧 ++ 各一组隐藏
    });
  });

  describe('decorations: 链接与图片', () => {
    it('链接文字可见并携带安全 data 属性，语法与地址隐藏', () => {
      const md = '[官网](https://example.com)\n\n尾部说明文字';
      const r = build(md);
      const flat = flatten(r);
      expect(flat.filter((d) => d.cls === 'fluen-lp-link').length).toBe(1);
      // [ ] ( ) 四个语法字符 + URL 地址均被隐藏
      expect(countAtomic(r)).toBeGreaterThanOrEqual(4);
    });

    it('图片显示 alt、地址隐藏', () => {
      const md = '![示意图](assets/a.png)\n\n尾部说明文字';
      const r = build(md);
      expect(flatten(r).some((d) => d.cls === 'fluen-lp-img-alt')).toBe(true);
      expect(countAtomic(r)).toBeGreaterThanOrEqual(3);
    });
  });

  describe('decorations: 列表 / 引用 / 数学', () => {
    it('无序列表符替换为 widget', () => {
      const md = '- 项目一\n- 项目二\n\n正文。';
      const r = build(md);
      const flat = flatten(r);
      expect(flat.filter((d) => d.widget).length).toBe(2);
    });

    it('引用块行类名 + QuoteMark 隐藏', () => {
      const md = '> 引用的句子\n\n正文。';
      const r = build(md);
      const flat = flatten(r);
      expect(flat.some((d) => d.cls === 'fluen-lp-quote')).toBe(true);
      expect(countAtomic(r)).toBeGreaterThanOrEqual(1);
    });

    it('行内数学整体替换为 widget', () => {
      const md = '质能 $E=mc^2$ 方程。';
      const r = build(md);
      const flat = flatten(r);
      const maths = flat.filter((d) => d.widget && d.to - d.from === '$E=mc^2$'.length);
      expect(maths.length).toBe(1);
    });

    it('块级多行公式整体替换', () => {
      const md = '$$\nx=y+1\n$$\n\n之后段落。';
      const r = build(md);
      const flat = flatten(r);
      const blocky = flat.find((d) => d.widget && d.to - d.from === '$$\nx=y+1\n$$'.length);
      expect(blocky).toBeDefined();
    });

    it('代码围栏内的 $ 与数学符号不做渲染', () => {
      const md = '```\n$x$\n```\n\n真式子 $x$ 收尾。';
      const r = build(md);
      const flat = flatten(r);
      expect(flat.some((d) => d.cls === 'fluen-lp-fence')).toBe(true);
      // 只应有一处（围栏外那一个）数学 widget
      expect(flat.filter((d) => d.widget).length).toBe(1);
    });

    it('光标落在块级公式区内时铺源码提示底色而非替换', () => {
      const md = '$$\nx=y+1\n$$\nabc';
      const pos = md.indexOf('y+1');
      const r = build(md, [pos, pos]);
      const flat = flatten(r);
      expect(flat.some((d) => d.cls === 'fluen-lp-math-src')).toBe(true);
    });
  });

  describe('decorations: 表格', () => {
    const BARE_TABLE = '| A | B |\n| --- | --- |\n| 1 | 2 |';

    it('裸 GFM 表整体替换为可交互 widget（原子块）', () => {
      const md = `${BARE_TABLE}\n\n正文段落。`;
      const r = build(md);
      const flat = flatten(r);
      const widget = flat.find((d) => d.widget && d.from === 0);
      expect(widget).toBeDefined();
      expect(widget!.to - widget!.from).toBe(BARE_TABLE.length);
      expect(r.atomicRanges.iter().value).not.toBeNull(); // 注册原子性，光标不进入
    });

    it('<f-tbl> 内嵌 MD 表（FTagTable 节点）同样渲染 widget', () => {
      // 插入器生成的精确形态：插入器输出直接复制于此，防两侧行为漂移
      const md = '<f-tbl>\n  <f-caption>表格题注</f-caption>\n\n|  |  |  |\n| --- | --- | --- |\n|  |  |  |\n|  |  |  |\n\n</f-tbl>\n\n正文段落。';
      const r = build(md);
      const widget = flatten(r).find((d) => d.widget && d.cls === undefined);
      expect(widget).toBeDefined();
      const inner = '|  |  |  |\n| --- | --- | --- |\n|  |  |  |\n|  |  |  |';
      expect(widget!.to - widget!.from).toBe(inner.length);
    });

    it('光标在表格内仍渲染 widget（半预览不揭示表格源码）', () => {
      const md = `${BARE_TABLE}\n\n正文段落。`;
      const inside = md.indexOf('| 1 |');
      const r = build(md, [inside, inside]);
      expect(flatten(r).some((d) => d.widget && d.cls === undefined)).toBe(true);
    });

    it('结构异常的表格（缺分隔行）降级为源码显示', () => {
      const r = build('| A |\n| x |\n| y |\n\n正文。');
      expect(flatten(r).some((d) => d.widget)).toBe(false);
    });
  });

  function countAtomic(r: DecorationBuildResult): number {
    let n = 0;
    const iter = r.atomicRanges.iter();
    while (iter.value) {
      n++;
      iter.next();
    }
    return n;
  }
}
