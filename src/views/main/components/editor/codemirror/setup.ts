/**
 * CodeMirror 6 装配——双栏 MD 源码编辑器的扩展组合。
 *
 * 仅负责 CM6 实例的创建与默认扩展装配（history、markdown、f- 标签/脚注语法高亮、
 * keymap、update listener），不引入任何渲染/装饰逻辑——预览由独立的 FluenPreview
 * 组件经后端 `editor_render_html` 完成。
 *
 * 这是唯一知道 CM6 导入的模块，其他编辑器模块均消费这里的工厂函数。
 */

import { EditorState } from '@codemirror/state';
import { EditorView, keymap, drawSelection } from '@codemirror/view';
import { defaultKeymap, historyKeymap, history, undo, redo } from '@codemirror/commands';
import { markdown } from '@codemirror/lang-markdown';
import {
  syntaxHighlighting,
  defaultHighlightStyle,
  bracketMatching,
} from '@codemirror/language';

import { ftagExtension } from './ftagSyntax';
import { footnoteExtension } from './footnoteSyntax';

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
      // 启用 f- 标签与脚注语法扩展（语法高亮，非装饰渲染）
      markdown({ extensions: [ftagExtension, footnoteExtension] }),
      keymap.of([
        // Ctrl/Cmd+S → save（return true 阻止浏览器默认行为）
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
