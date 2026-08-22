/**
 * 功能面板内置图标（函数式组件）。
 *
 * 收敛各面板内联重复的 SVG 为小型可复用图标组件，供模板直接 `<Icon :size :stroke-width />` 使用。
 */

import { h, type FunctionalComponent } from 'vue';

/** 通用图标属性。 */
export interface IconProps {
  /** 边长（px）。 */
  size?: number;
  /** 描边宽度。 */
  strokeWidth?: number;
}

/** 依据 viewBox 与 path 数据生成图标组件。 */
function svgIcon(
  viewBox: string,
  paths: string[],
  defaultSize = 14,
): FunctionalComponent<IconProps> {
  return (p) => {
    const size = p.size ?? defaultSize;
    const sw = p.strokeWidth ?? 2;
    return h(
      'svg',
      {
        viewBox,
        width: size,
        height: size,
        fill: 'none',
        stroke: 'currentColor',
        'stroke-width': sw,
        'stroke-linecap': 'round',
        'stroke-linejoin': 'round',
      },
      paths.map((d) => h('path', { d })),
    );
  };
}

/** 折叠箭头（方向可经 CSS transform 旋转切换）。 */
export const Chevron = svgIcon('0 0 16 16', ['M4 6l4 4 4-4']);

/** 加号（新建子标题 / 新建章节）。 */
export const Plus = svgIcon('0 0 24 24', ['M12 5v14M5 12h14']);

/** 铅笔（重命名）。 */
export const Pencil = svgIcon('0 0 24 24', ['M12 20h9', 'M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z']);

/** 折叠全部（收缩箭头）。 */
export const CollapseAll = svgIcon('0 0 24 24', ['M5 8l7 7 7-7', 'M5 4h14']);

/** 展开全部（展开箭头）。 */
export const ExpandAll = svgIcon('0 0 24 24', ['M5 16l7-7 7 7', 'M5 4h14']);

/** 空图层级（列表）。 */
export const ListIcon = svgIcon('0 0 24 24', ['M4 6h16M4 12h12M4 18h8'], 32);