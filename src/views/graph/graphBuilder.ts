/**
 * 图构建工具：从知识库条目列表构建图节点与边。
 *
 * 职责：
 *   - 节点：每条 `WikiEntry` 对应一个 `GraphNode`，计算连接度
 *   - 边：每条 `relations` 出向关系对应一条 `GraphEdge`
 *   - 去重：双向关系合并为单边；自环忽略
 *   - 过滤：目标不存在的边丢弃，避免悬空引用
 *   - 初始化：节点初始位置环形分布，避免力导向初始奇点
 *
 * 与仿真器（`useForceSimulation`）解耦：本模块只产出初始图数据，
 * 不参与后续物理仿真。
 */

import type { WikiEntry } from '../../types/knowledgeBase';
import type { GraphNode, GraphEdge } from './types';

/** 环形初始布局的半径基数（每节点数 × 此值累加）。 */
const INITIAL_RING_RADIUS = 120;
/** 环形初始布局的起始角度（弧度）。 */
const INITIAL_RING_OFFSET = -Math.PI / 2;

/**
 * 从知识库条目构建图数据。
 *
 * @param entries 知识库条目列表（来自 `knowledge_list_entries`）
 * @returns 图节点与边
 */
export function buildGraph(entries: WikiEntry[]): {
  nodes: GraphNode[];
  edges: GraphEdge[];
} {
  // 节点 ID → 索引，用于边去重与目标存在性校验
  const idSet = new Set(entries.map((e) => e.id));
  // 边去重集合：key 形如 `a->b`（a、b 已按字典序排序）
  const edgeKeys = new Set<string>();
  // 节点连接度计数
  const degreeMap = new Map<string, number>();

  const edges: GraphEdge[] = [];
  for (const entry of entries) {
    for (const rel of entry.relations ?? []) {
      const target = rel.id;
      // 自环忽略
      if (target === entry.id) continue;
      // 悬空引用丢弃
      if (!idSet.has(target)) continue;
      // 双向去重：始终以字典序较小的 ID 作为 source
      const [a, b] = entry.id < target ? [entry.id, target] : [target, entry.id];
      const key = `${a}->${b}`;
      if (edgeKeys.has(key)) continue;
      edgeKeys.add(key);
      edges.push({ id: key, source: a, target: b });
      degreeMap.set(a, (degreeMap.get(a) ?? 0) + 1);
      degreeMap.set(b, (degreeMap.get(b) ?? 0) + 1);
    }
  }

  // 节点初始位置：环形分布，避免力导向初始奇点（所有节点重叠于原点）
  const n = entries.length;
  const radius = Math.max(INITIAL_RING_RADIUS, n * 6);
  const nodes: GraphNode[] = entries.map((entry, i) => {
    const angle = INITIAL_RING_OFFSET + (2 * Math.PI * i) / Math.max(n, 1);
    return {
      id: entry.id,
      title: entry.title,
      wiki_type: entry.wiki_type,
      degree: degreeMap.get(entry.id) ?? 0,
      x: Math.cos(angle) * radius,
      y: Math.sin(angle) * radius,
      vx: 0,
      vy: 0,
    };
  });

  return { nodes, edges };
}

/**
 * 按类型筛选图数据。
 *
 * 保留指定类型的节点，并丢弃任一端点不在保留集合中的边。
 *
 * @param nodes 原始节点列表
 * @param edges 原始边列表
 * @param type 要保留的类型；`null` 表示全部保留
 * @returns 筛选后的节点与边（新数组，节点为同一引用副本）
 */
export function filterGraphByType(
  nodes: GraphNode[],
  edges: GraphEdge[],
  type: GraphNode['wiki_type'] | null,
): { nodes: GraphNode[]; edges: GraphEdge[] } {
  if (type === null) return { nodes: [...nodes], edges: [...edges] };
  const keptIds = new Set(nodes.filter((n) => n.wiki_type === type).map((n) => n.id));
  return {
    nodes: nodes.filter((n) => keptIds.has(n.id)),
    edges: edges.filter((e) => keptIds.has(e.source) && keptIds.has(e.target)),
  };
}
