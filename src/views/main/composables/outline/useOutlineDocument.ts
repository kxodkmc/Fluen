/**
 * 大纲文档同步层（单例）。
 *
 * 编辑器打开期间，CM6 缓冲是内容的唯一事实源。本层订阅编辑器文档变化与项目打开
 * 事件，将 `main.md` 解析为带稳定 id 的扁平标题列表，供 UI 渲染与结构操作使用。
 *
 * 数据源：
 * - 主：`useFluenEditor().onDocChange` —— 覆盖实时打字与本地结构操作（dispatch 亦
 *   会触发本回调），保证大纲与编辑器内容实时一致；
 * - 辅：`useProject().mainMd`（模块级注册一次）—— 覆盖"项目打开但编辑器尚未挂载"
 *   的窗口期；编辑器挂载后由主数据源接管。
 *
 * 每次仅做一次 [`parseOutlineFlat`] + [`assignStableIds`]（两者均 O(N)、亚毫秒级），
 * 稳定 id 保证 key 不因上方增删行而变化，从而避免整棵子树重建。
 */

import { shallowRef, readonly, watch } from 'vue';
import { useProject } from '../../../../composables/useProject';
import { useFluenEditor } from '../../components/editor/composables/useFluenEditor';
import { parseOutlineFlat, type FlatHeading, type OutlineFlat } from './outlineParser';
import { assignStableIds } from './headingIds';

/** 当前扁平标题列表（含稳定 id）。始终与编辑器缓冲内容一致。 */
const flatHeadings = shallowRef<FlatHeading[]>([]);

/** 依据最新文档文本更新标题列表，并复用上轮 id 以稳定 key。 */
function publish(md: string): void {
  const base: OutlineFlat[] = md ? parseOutlineFlat(md) : [];
  flatHeadings.value = assignStableIds(flatHeadings.value, base);
}

/* ── 订阅（模块级、随单例生命周期存在，注册一次，避免 N+1 watcher）── */
useFluenEditor().onDocChange((md) => publish(md));
watch(
  useProject().mainMd,
  (md) => {
    if (md) publish(md);
  },
  { immediate: true },
);

/** 访问大纲文档状态。返回的 `flatHeadings` 为只读响应式列表。 */
export function useOutlineDocument() {
  return {
    flatHeadings: readonly(flatHeadings),
  };
}