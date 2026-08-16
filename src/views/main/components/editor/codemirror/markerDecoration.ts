/**
 * 章节标记（`<!-- @sec_id:xxx -->`）隐藏装饰。
 *
 * 章节标记是 Fluen 章节切分的内部元数据，必须保留在文档文本中（保存时按它
 * 拆分章节），但对用户不可见。本模块通过 CM6 行级装饰给标记行附加一个类，
 * 由 CSS（display:none）隐藏整行，文档文本与保存往返完全不受影响。
 */

import { RangeSetBuilder, StateField, Text } from '@codemirror/state';
import { Decoration, DecorationSet, EditorView } from '@codemirror/view';

/** 章节标记行：行首（允许前导空白）`<!-- @sec_id:` + id + `-->` 行尾（允许尾部空白）。 */
const SEC_MARKER_LINE = /^\s*<!--\s*@sec_id:.*-->\s*$/;

/** 标记行装饰：给 .cm-line 附加 `.fluen-sec-marker-line` 类，由 CSS 隐藏。 */
const markerLineDecoration = Decoration.line({ class: 'fluen-sec-marker-line' });

function buildMarkers(doc: Text): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  for (let i = 1; i <= doc.lines; i++) {
    const line = doc.line(i);
    if (SEC_MARKER_LINE.test(line.text)) {
      builder.add(line.from, line.from, markerLineDecoration);
    }
  }
  return builder.finish();
}

/** CM6 扩展：隐藏章节标记行（仅视觉隐藏，文档文本保持不变）。 */
export const hideSectionMarkers = StateField.define<DecorationSet>({
  create(state) {
    return buildMarkers(state.doc);
  },
  update(deco, tr) {
    return tr.docChanged ? buildMarkers(tr.state.doc) : deco;
  },
  provide: (field) => EditorView.decorations.from(field),
});
