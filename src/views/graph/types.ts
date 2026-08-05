/**
 * 知识库网状图模块的类型定义。
 *
 * 与 `types/knowledgeBase.ts` 的 `WikiEntry` / `WikiType` 解耦：
 * 图模块仅消费图计算所需的最小字段（id / title / wiki_type / degree），
 * 避免把完整条目数据塞进仿真循环，降低内存与计算开销。
 */

import type { WikiType } from '../../types/knowledgeBase';

/** 图节点（对应一个 wiki 条目）。 */
export interface GraphNode {
  /** 条目 ID（wiki-xxxxxxxxxxxxxxxx）。 */
  id: string;
  /** 条目标题。 */
  title: string;
  /** 条目类型。 */
  wiki_type: WikiType;
  /** 连接度（关联边数），用于节点大小映射。 */
  degree: number;
  /** 当前坐标 x（由仿真器维护）。 */
  x: number;
  /** 当前坐标 y（由仿真器维护）。 */
  y: number;
  /** 速度 x（由仿真器维护）。 */
  vx: number;
  /** 速度 y（由仿真器维护）。 */
  vy: number;
  /** 是否被固定（拖拽中或用户钉住时不参与受力位移）。 */
  fixed?: boolean;
}

/** 图边（对应一条 wiki relation：source → target）。 */
export interface GraphEdge {
  /** 边唯一标识，`source->target`。 */
  id: string;
  /** 起点节点 ID。 */
  source: string;
  /** 终点节点 ID。 */
  target: string;
}

/** 仿真参数（力导向布局）。 */
export interface ForceSimulationOptions {
  /** 节点间排斥力强度系数（越大节点越分散）。 */
  repulsion: number;
  /** 边的弹簧刚度（越大边越短）。 */
  springStrength: number;
  /** 边的目标长度（像素）。 */
  springLength: number;
  /** 向中心收敛的引力强度（0~1）。 */
  gravity: number;
  /** 速度阻尼（0~1，越大衰减越快）。 */
  damping: number;
  /** 仿真中心 x 坐标。 */
  centerX: number;
  /** 仿真中心 y 坐标。 */
  centerY: number;
}

/** 默认仿真参数（针对 30~200 节点的知识库规模调优）。 */
export const DEFAULT_SIMULATION_OPTIONS: ForceSimulationOptions = {
  repulsion: 6000,
  springStrength: 0.04,
  springLength: 140,
  gravity: 0.04,
  damping: 0.82,
  centerX: 0,
  centerY: 0,
};
