/**
 * 半预览核心接线层：装饰字段 + 原子区间 + 链接点击。
 *
 * 职责：
 *   - StateField 订阅事务（docChanged / 选区变化 / 开关切换），调用纯函数层
 *     重建装饰。**必须是 StateField**：表格与块级公式是跨多行的 block 替换
 *     装饰，CodeMirror 6 明确禁止 ViewPlugin 提供 block 装饰（渲染时抛
 *     RangeError: "Block decorations may not be specified via plugins"；
 *     且异常发生在文档状态已更新、DOM 未重建的中途，视图从此错乱并在
 *     后续每次更新重复抛错——表现为「插入表格后无法进入半渲染 / 半渲染中
 *     插入表格直接卡死」）。StateField 经 `EditorView.decorations` facet
 *     提供的装饰集不受此限制。
 *   - 将 replace/widget 区间注册到 atomicRanges，光标不会进入零宽隐藏区
 *   - 拦截链接点击，经 scheme 白名单校验后交给系统打开器
 *
 * 表格渲染态的结构化编辑（单元格输入 / 行列手柄）不经过本模块：其事件
 * 由 widget DOM 原生监听处理（CM6 会丢弃 ignoreEvent widget 内冒泡的事
 * 件），写回逻辑见 tableEditing.ts。
 *
 * IME 安全：装饰重建不做组词期冻结（字段层无法感知 view.composing），
 * 但揭示语义保证光标处显示原始源码（无 replace/widget），组词视图稳定；
 * 表格单元格的 DOM 写回由 tableEditing 层的 IME 状态守护。
 */

import { syntaxTree } from '@codemirror/language';
import { StateField } from '@codemirror/state';
import type { EditorState } from '@codemirror/state';
import { Decoration, EditorView } from '@codemirror/view';
import type { DecorationSet } from '@codemirror/view';
import { livePreviewField, setLivePreviewEffect } from './state';
import { buildLivePreviewDecorations, type DecorationBuildResult } from './decorations';
import { openExternalLink } from './linkOpen';

/** 关闭态 / 未挂载时的空装饰集。 */
const EMPTY_SET: DecorationSet = Decoration.none;

/** 关闭态的空构建结果（以引用相等跳过冗余更新）。 */
const EMPTY_RESULT: DecorationBuildResult = {
  decorations: EMPTY_SET,
  atomicRanges: EMPTY_SET,
};

/** 按状态构建装饰；开关关闭时返回共享空集。 */
function computeFor(state: EditorState): DecorationBuildResult {
  if (!state.field(livePreviewField, false)) {
    return EMPTY_RESULT;
  }
  const sel = state.selection.main;
  return buildLivePreviewDecorations(state.doc, syntaxTree(state), {
    from: sel.from,
    to: sel.to,
  });
}

/**
 * 半预览装饰字段：持有视觉装饰集与原子区间子集。
 *
 * 重建时机与常规 live-preview 一致：文档变更 / 选区显式变化 / 开关切换。
 * 依赖由 CM6 的槽机制自动解析（livePreviewField 与语言字段先于本字段就绪）。
 */
export const livePreviewDecorations = StateField.define<DecorationBuildResult>({
  create: () => EMPTY_RESULT,
  update(value, tr) {
    const toggled = tr.effects.some((e) => e.is(setLivePreviewEffect));
    if (!tr.docChanged && tr.selection === undefined && !toggled) return value;
    const next = computeFor(tr.state);
    // 关闭态：共享空集引用相等，避免无谓的字段变更通知
    return next === EMPTY_RESULT && value === EMPTY_RESULT ? value : next;
  },
  provide: (field) => [
    // 静态装饰集（facet.compute）——允许 block widget
    EditorView.decorations.from(field, (v) => v.decorations),
    // 原子区间 facet 的输入为函数，静态闭包读取字段即可
    EditorView.atomicRanges.of(
      (view) => view.state.field(field, false)?.atomicRanges ?? EMPTY_SET,
    ),
  ],
});

// ── 链接点击：白名单校验后交系统打开 ────────────────────────────────

/** 半预览 DOM 事件处理（链接在普通行内，事件可正常冒泡至编辑器）。 */
const livePreviewDomEvents = EditorView.domEventHandlers({
  mousedown(event) {
    const target = event.target as HTMLElement | null;
    const linkHost = target?.closest?.('.fluen-lp-link') as HTMLElement | null;
    if (!linkHost) return false;
    const href = linkHost.getAttribute('data-fluen-href');
    if (!href) return false;
    event.preventDefault();
    void openExternalLink(href);
    return true;
  },
});

/**
 * 半预览核心扩展总装：开关字段 + 装饰字段（含原子区间）+ 事件处理。
 * 视觉主题见 theme.ts（由 setup.ts 一并装配）。
 */
export const livePreviewCore = [
  livePreviewField,
  livePreviewDecorations,
  livePreviewDomEvents,
];

// ===== 单元测试（守卫：仅在 vitest 环境运行，生产构建被 define 树摇） =====

if (import.meta.vitest) {
  const { describe, it, expect } = import.meta.vitest;
  const { EditorState } = await import('@codemirror/state');
  const { markdown, markdownLanguage } = await import('@codemirror/lang-markdown');

  /** 与生产 setup.ts 一致的方言组合 + livePreviewCore（不含主题）。 */
  function makeState(doc: string): EditorState {
    return EditorState.create({
      doc,
      extensions: [
        markdown({
          base: markdownLanguage,
          extensions: [ftagExtensionForTest, underlineForTest, mathForTest],
          addKeymap: false,
        }),
        livePreviewCore,
      ],
    });
  }

  // 复用生产方言（动态 import 惰性装配，避免顶层循环依赖）
  const { ftagExtension: ftagExtensionForTest } = await import('../ftagSyntax');
  const { underlineExtension: underlineForTest } = await import('../underlineSyntax');
  const { fluenMathExtension: mathForTest } = await import('./mathSyntax');

  function countAtomic(state: EditorState): number {
    let n = 0;
    const iter = state.field(livePreviewDecorations).atomicRanges.iter();
    while (iter.value) {
      n++;
      iter.next();
    }
    return n;
  }

  describe('livePreviewDecorations: 开关与重建', () => {
    it('默认（关闭态）为空集，开启后产出 block widget 装饰', () => {
      const md = '| A | B |\n| --- | --- |\n| 1 | 2 |';
      const off = makeState(md);
      expect(off.field(livePreviewDecorations).decorations).toBe(EMPTY_SET);
      expect(countAtomic(off)).toBe(0);

      const on = off.update({ effects: setLivePreviewEffect.of(true) }).state;
      const { decorations } = on.field(livePreviewDecorations);
      // 表格必须以 block 替换装饰提供（ViewPlugin 提供会被 CM 拒绝渲染）
      let hasBlockWidget = false;
      const iter = decorations.iter();
      while (iter.value) {
        const spec = iter.value.spec as { block?: boolean; widget?: unknown };
        if (spec.widget && spec.block) hasBlockWidget = true;
        iter.next();
      }
      expect(hasBlockWidget).toBe(true);
      expect(countAtomic(on)).toBeGreaterThan(0);
    });

    it('文档变更触发重建（新增表格出现新的替换区间）', () => {
      let st = makeState('正文段落。').update({ effects: setLivePreviewEffect.of(true) }).state;
      expect(countAtomic(st)).toBe(0);
      const table = '| A |\n| --- |\n| 1 |';
      st = st.update({ changes: { from: st.doc.length, insert: `\n\n${table}\n` } }).state;
      expect(countAtomic(st)).toBe(1);
    });

    it('选区移入强调元素揭示源码（原子区间随之清空），移出后恢复', () => {
      const md = '**加粗**文本';
      // 光标先置于文尾（避免默认起点 touching 粗体而处于揭示态）
      let st = makeState(md)
        .update({ effects: setLivePreviewEffect.of(true), selection: { anchor: md.length } })
        .state;
      expect(countAtomic(st)).toBe(2);
      st = st.update({ selection: { anchor: 3 } }).state; // 光标进入 ** 内部
      expect(countAtomic(st)).toBe(0);
      st = st.update({ selection: { anchor: md.length } }).state; // 移出
      expect(countAtomic(st)).toBe(2);
    });

    it('关闭后回到共享空集（引用相等，不触发冗余更新）', () => {
      const md = '| A |\n| --- |\n| 1 |';
      let st = makeState(md).update({ effects: setLivePreviewEffect.of(true) }).state;
      expect(countAtomic(st)).toBe(1);
      st = st.update({ effects: setLivePreviewEffect.of(false) }).state;
      expect(st.field(livePreviewDecorations).decorations).toBe(EMPTY_SET);
      expect(countAtomic(st)).toBe(0);
    });
  });
}
