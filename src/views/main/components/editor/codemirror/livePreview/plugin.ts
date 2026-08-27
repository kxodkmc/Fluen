/**
 * 半预览 ViewPlugin 接线层。
 *
 * 职责：
 *   - 订阅 update（docChanged / selectionSet / 开关切换），调用纯函数层重建装饰
 *   - IME 安全：`view.composing` 期间不做任何重建，组词视图完全静止；
 *     已有装饰集随文档事务由 CM6 自动 map，不存在错位风险
 *   - 将 replace/widget 区间注册到 atomicRanges，光标不会进入零宽隐藏区
 *   - 拦截链接点击，经 scheme 白名单校验后交给系统打开器
 */

import { syntaxTree } from '@codemirror/language';
import { Decoration, EditorView, ViewPlugin } from '@codemirror/view';
import type {
  DecorationSet,
  PluginSpec,
  PluginValue,
  ViewUpdate,
} from '@codemirror/view';
import { livePreviewField } from './state';
import { buildLivePreviewDecorations } from './decorations';
import { openExternalLink } from './linkOpen';

/** 关闭态 / 未挂载时的空装饰集。 */
const EMPTY_SET: DecorationSet = Decoration.none;

/**
 * 半预览插件本体。装饰构建全部委托给纯函数层，
 * 本类只负责「何时重建」与「如何暴露给视图」。
 */
class LivePreviewPlugin implements PluginValue {
  /** 视觉装饰集。 */
  decorations: DecorationSet;

  /** replace/widget 区间子集（原子性注册）。 */
  atomicRanges: DecorationSet;

  private disposed = false;

  constructor(view: EditorView) {
    // 构造即按当前开关状态产出，保证从 source 切入 live 的首帧就绪
    ({ decorations: this.decorations, atomicRanges: this.atomicRanges } =
      computeFor(view));
  }

  update(update: ViewUpdate): void {
    if (this.disposed) return;
    // 稳定性护栏：IME 组合期间冻结视觉重排（中文输入法安全）
    if (update.view.composing) return;
    // 仅在内容 / 选区 / 开关变化时重建（滚动与焦点变化不触发）
    const toggled =
      update.startState.field(livePreviewField, false) !==
      update.state.field(livePreviewField, false);
    if (!update.docChanged && !update.selectionSet && !toggled) return;
    const next = computeFor(update.view);
    this.decorations = next.decorations;
    this.atomicRanges = next.atomicRanges;
  }

  destroy(): void {
    this.disposed = true;
  }
}

/** 按视图当前状态构建装饰；关闭时直接返回空集。 */
function computeFor(view: EditorView): {
  decorations: DecorationSet;
  atomicRanges: DecorationSet;
} {
  if (!view.state.field(livePreviewField, false)) {
    return { decorations: EMPTY_SET, atomicRanges: EMPTY_SET };
  }
  const sel = view.state.selection.main;
  return buildLivePreviewDecorations(view.state.doc, syntaxTree(view.state), {
    from: sel.from,
    to: sel.to,
  });
}

// ── 链接点击：白名单校验后交系统打开 ────────────────────────────────

function onMousedown(event: MouseEvent): boolean {
  const target = event.target as HTMLElement | null;
  const linkHost = target?.closest?.('.fluen-lp-link') as HTMLElement | null;
  if (!linkHost) return false;
  const href = linkHost.getAttribute('data-fluen-href');
  if (!href) return false;
  event.preventDefault();
  void openExternalLink(href);
  return true;
}

const spec: PluginSpec<LivePreviewPlugin> = {
  decorations: (plugin) => plugin.decorations,
  eventHandlers: {
    mousedown(event) {
      return onMousedown(event);
    },
  },
  provide: (plugin) =>
    EditorView.atomicRanges.of(
      (view) => view.plugin(plugin)?.atomicRanges ?? Decoration.none,
    ),
};

/**
 * 半预览核心扩展总装：开关字段 + 装饰插件 + 原子区间 + 点击处理。
 * 视觉主题见 theme.ts（由 setup.ts 一并装配）。
 */
export const livePreviewCore = [livePreviewField, ViewPlugin.fromClass(LivePreviewPlugin, spec)];
