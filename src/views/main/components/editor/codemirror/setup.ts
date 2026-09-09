/**
 * CodeMirror 6 装配——MD 源码编辑器的扩展组合。
 *
 * 仅负责 CM6 实例的创建与默认扩展装配（history、markdown、f- 标签/脚注/数学
 * 语法高亮、半预览扩展、keymap、update listener）。HTML 级渲染仍由独立的
 * FluenPreview 组件经后端 `editor_render_html` 完成；半预览（live）视图
 * 由 livePreview 扩展包在同一编辑器实例上动态开关实现。
 *
 * 这是唯一知道 CM6 导入的模块，其他编辑器模块均消费这里的工厂函数。
 */

import { EditorState } from '@codemirror/state';
import { EditorView, keymap, drawSelection } from '@codemirror/view';
import { defaultKeymap, historyKeymap, history, undo, redo } from '@codemirror/commands';
import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
import {
  syntaxHighlighting,
  defaultHighlightStyle,
  bracketMatching,
} from '@codemirror/language';

import { ftagExtension } from './ftagSyntax';
import { footnoteExtension } from './footnoteSyntax';
import { underlineExtension } from './underlineSyntax';
import { hideSectionMarkers } from './markerDecoration';
import { fluenMathExtension, livePreviewExtension } from './livePreview';

/**
 * CM6 update listener 转发的回调集合。composable 订阅这些回调以同步响应式状态。
 */
export interface EditorCallbacks {
  /** Ctrl/Cmd+S 触发。 */
  onSave: () => void;
  /** 光标活动行变化（0-based 行号）。 */
  onActiveLineChange: (line: number) => void;
  /** 文档内容变化（完整 MD 文本）。 */
  onDocChange: (md: string) => void;
  /** 选区变化（含选区清空，from === to 表示空选区）。 */
  onSelectionChange: (from: number, to: number) => void;
}

/**
 * 创建带 Fluen 默认扩展集的 `EditorState`。
 *
 * @param doc        初始文档文本。
 * @param callbacks  update listener / keymap 转发的回调。
 */
export function createEditorState(doc: string, callbacks: EditorCallbacks): EditorState {
  return EditorState.create({
    doc,
    extensions: [
      history({ newGroupDelay: 500 }),
      EditorView.lineWrapping,
      drawSelection(),
      bracketMatching(),
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      // 启用 f- 标签、脚注、下划线与数学公式语法扩展；base 采用 GFM 方言
      // （表格 / 删除线 / 任务列表 / 上下标），与半预览渲染规则对齐
      markdown({
        base: markdownLanguage,
        extensions: [ftagExtension, footnoteExtension, underlineExtension, fluenMathExtension],
      }),
      // 隐藏章节标记行（<!-- @sec_id:xxx -->），对用户不可见但保留在文档中
      hideSectionMarkers,
      // 半预览扩展包：默认关闭，经 useFluenEditor().setLivePreview() 动态开启
      ...livePreviewExtension,
      keymap.of([
        // Ctrl/Cmd+S → save（return true 阻止浏览器默认行为）。
        // 注：全局快捷键模块（src/shortcuts）在 document capture 阶段已接管 Mod-s，
        // 此处的绑定成为编辑器内兜底通道（save 本身有防重入，双通道不会重复保存）。
        { key: 'Mod-s', run: () => { callbacks.onSave(); return true; } },
        // 显式注册 undo/redo，确保绑定优先于 defaultKeymap 中可能冲突的默认项
        { key: 'Mod-z', run: undo },
        { key: 'Mod-Shift-z', run: redo },
        { key: 'Mod-y', run: redo },
        ...defaultKeymap,
        ...historyKeymap,
      ]),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          callbacks.onDocChange(update.state.doc.toString());
        }
        if (update.selectionSet) {
          const line = update.state.doc.lineAt(update.state.selection.main.head).number - 1;
          callbacks.onActiveLineChange(line);
          const sel = update.state.selection.main;
          callbacks.onSelectionChange(
            Math.min(sel.anchor, sel.head),
            Math.max(sel.anchor, sel.head),
          );
        }
      }),
    ],
  });
}

/**
 * 在指定宿主元素上挂载 CM6 `EditorView`。
 *
 * @param parent  挂载点，CM6 会将 DOM 追加到其下。
 * @param state   初始 state（来自 {@link createEditorState}）。
 */
export function createEditorView(parent: HTMLElement, state: EditorState): EditorView {
  return new EditorView({ parent, state });
}
