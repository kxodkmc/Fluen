/**
 * marks 子模块导出。
 *
 * @module reader/marks
 */

export { buildMarksOverlayScript } from './MarksOverlay';
export type {
  SerializedAnchor,
  SerializedMark,
  SelectionRect,
  OuterToInnerMessage,
  InnerToOuterMessage,
  MarksRenderMessage,
  MarksClearMessage,
  MarksRemoveMessage,
  MarksScrollMessage,
  SelectionCreateMessage,
  MarkClickMessage,
  MarksReadyMessage,
} from './types';
export { isInnerMessage } from './types';
