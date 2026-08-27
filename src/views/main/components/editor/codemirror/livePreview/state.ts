/**
 * 半预览（Live Preview）模式开关——CM6 状态层。
 *
 * 通过 StateEffect 动态切换装饰扩展的启停，而非重建 EditorState：
 * 切换时撤销历史、光标位置、滚动偏移全部保留，视图瞬时生效。
 *
 * 装饰只作用于视觉层（Decoration），文档文本永远不变，
 * 因此本扩展不可能影响 dirty 判断、保存链路与大纲联动。
 */

import { StateEffect, StateField } from '@codemirror/state';
import type { EditorState } from '@codemirror/state';

/** 开关效果：dispatch 一个 `setLivePreviewEffect.of(true/false)` 即时切换。 */
export const setLivePreviewEffect = StateEffect.define<boolean>();

/** 扩展入口字段：持有当前是否启用半预览渲染。默认关闭（仅源码形态）。 */
export const livePreviewField = StateField.define<boolean>({
  create: () => false,
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setLivePreviewEffect)) return effect.value;
    }
    return value;
  },
});

/**
 * 读取当前半预览开关。
 *
 * 第二参传 `false`：未装配本字段的状态（如测试中构造的裸 State）安全返回 undefined，
 * 此处归一为 false，不抛 `RangeError`。
 */
export function isLivePreview(state: EditorState): boolean {
  return state.field(livePreviewField, false) ?? false;
}

// ===== 单元测试（守卫：仅在 vitest 环境运行，生产构建被 define 树摇） =====

if (import.meta.vitest) {
  const { describe, it, expect } = import.meta.vitest;
  const { EditorState } = await import('@codemirror/state');

  function makeState(enabled?: boolean): EditorState {
    let state = EditorState.create({ extensions: [livePreviewField] });
    if (enabled !== undefined) {
      state = state.update({ effects: setLivePreviewEffect.of(enabled) }).state;
    }
    return state;
  }

  describe('livePreviewState: 开关', () => {
    it('默认关闭', () => {
      expect(makeState().field(livePreviewField)).toBe(false);
    });

    it('可打开并再次关闭', () => {
      expect(makeState(true).field(livePreviewField)).toBe(true);
      expect(
        makeState(true)
          .update({ effects: setLivePreviewEffect.of(false) })
          .state.field(livePreviewField),
      ).toBe(false);
    });

    it('普通文档事务不改变开关', () => {
      const st = makeState(true).update({ changes: { from: 0, insert: 'x' } }).state;
      expect(st.field(livePreviewField)).toBe(true);
    });
  });
}
