/**
 * 状态栏 — 项目状态条目。
 *
 * 在左下角状态栏注册一个条目，响应式展示当前文章的打开状态：
 *   - 已打开：显示文章图标 + 文章标题
 *   - 未打开：显示灰色提示文本
 *
 * 通过 useStatusEntry 声明式注册，作用域销毁时自动清理。
 * 在 MainView setup 中调用一次即可全局生效。
 */

import { watch, type WatchSource } from 'vue';
import { useStatusEntry } from './useStatusBar';
import { useProject } from '../../../../composables/useProject';
import { useI18n } from '../../../../i18n';

/** 文档图标 SVG path（24×24 viewBox）。 */
const ICON_DOC = 'M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z M14 2v6h6';

/** 空文档图标 SVG path（带虚线感，表示未打开）。 */
const ICON_DOC_EMPTY = 'M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z M14 2v6h6 M9 13h6 M9 17h6';

/**
 * 注册项目状态条目到状态栏左侧。
 *
 * 必须在组件 setup 或 effect scope 中调用。
 */
export function useProjectStatus(): void {
  const { t } = useI18n();
  const { hasProject, config } = useProject();

  const { patch } = useStatusEntry('project.status', {
    label: t('main.status.projectClosed'),
    position: 'left',
    order: -100,
    icon: ICON_DOC_EMPTY,
    tone: 'info',
    tooltip: t('main.status.projectClosed'),
  });

  // 响应式更新
  watch(
    [hasProject, config] as WatchSource[],
    ([opened, cfg]) => {
      if (opened && cfg) {
        const title = (cfg as { title: string }).title;
        patch({
          label: t('main.status.projectOpen', { title }),
          icon: ICON_DOC,
          tone: 'default',
          tooltip: title,
        });
      } else {
        patch({
          label: t('main.status.projectClosed'),
          icon: ICON_DOC_EMPTY,
          tone: 'info',
          tooltip: t('main.status.projectClosed'),
        });
      }
    },
    { immediate: true },
  );
}
