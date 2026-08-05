/**
 * 力导向图仿真器 composable。
 *
 * 实现经典的 Fruchterman-Reingold 风格力导向布局：
 *   - 节点间排斥力（库仑式，反比于距离平方）
 *   - 边弹簧吸引力（胡克式，正比于位移）
 *   - 向中心收敛引力（防止图整体漂移）
 *   - 速度阻尼（系统能量衰减，最终收敛到稳定布局）
 *
 * 设计要点：
 *   - **纯 TS 实现**：不依赖 d3-force 等外部库，符合项目"轻量"原则
 *   - **O(n²) 复杂度**：知识库规模通常 < 1000 节点，无需 Barnes-Hut 优化
 *   - **响应式快照**：仿真内部维护原始节点数组，每 tick 通过 `requestAnimationFrame`
 *     触发响应式更新，避免 Vue 深度响应式带来的性能损耗
 *   - **可控启停**：拖拽节点时临时固定；可手动暂停 / 重置 / 收敛
 *
 * @example
 * ```ts
 * const sim = useForceSimulation();
 * sim.init(nodes, edges, { ...DEFAULT_SIMULATION_OPTIONS, centerX: 400, centerY: 300 });
 * sim.start();
 * // 拖拽中：sim.fixNode(id, x, y);  释放：sim.releaseNode(id);
 * sim.stop();
 * ```
 */

import { ref, shallowRef } from 'vue';
import {
  DEFAULT_SIMULATION_OPTIONS,
  type ForceSimulationOptions,
  type GraphNode,
  type GraphEdge,
} from '../types';

/** 单次 tick 最大位移上限，防止数值爆炸。 */
const MAX_DISPLACEMENT = 200;
/** 仿真收敛阈值：当所有节点单 tick 位移均小于此值时自动停止。 */
const CONVERGE_THRESHOLD = 0.4;

/**
 * 力导向仿真器。
 *
 * 状态：
 *   - `nodes`：浅响应式节点数组（每 tick 替换引用以触发更新）
 *   - `running`：是否正在运行
 *   - `alpha`：温度系数（0~1），逐 tick 衰减，控制节点位移幅度
 */
export function useForceSimulation() {
  /** 当前节点快照（每 tick 替换引用）。 */
  const nodes = ref<GraphNode[]>([]);
  /** 当前边列表（初始化后不变，除非重新 init）。 */
  const edges = shallowRef<GraphEdge[]>([]);
  /** 是否正在运行。 */
  const running = ref(false);
  /** 当前温度系数。 */
  const alpha = ref(1);

  /** 内部可变节点数组（仿真循环直接修改，避免响应式开销）。 */
  let innerNodes: GraphNode[] = [];
  /** 内部边列表（含 source/target 索引，避免每 tick 查表）。 */
  let innerEdges: Array<{ source: number; target: number }> = [];
  /** 当前仿真参数。 */
  let options: ForceSimulationOptions = { ...DEFAULT_SIMULATION_OPTIONS };
  /** requestAnimationFrame 句柄。 */
  let rafId: number | null = null;
  /** 累计 tick 数（用于调试与收敛判定）。 */
  let tickCount = 0;

  // ── 初始化 ────────────────────────────────────────────────────────

  /**
   * 初始化仿真器。
   *
   * @param inputNodes 初始节点列表（坐标会被保留作为起点）
   * @param inputEdges 边列表
   * @param opts 仿真参数（`centerX`/`centerY` 应为画布中心）
   */
  function init(
    inputNodes: GraphNode[],
    inputEdges: GraphEdge[],
    opts: Partial<ForceSimulationOptions> = {},
  ): void {
    stop();
    options = { ...DEFAULT_SIMULATION_OPTIONS, ...opts };
    // 深拷贝节点，避免外部修改干扰仿真
    innerNodes = inputNodes.map((n) => ({ ...n, vx: 0, vy: 0 }));
    // 构建 ID → 索引映射
    const idToIndex = new Map<string, number>();
    innerNodes.forEach((n, i) => idToIndex.set(n.id, i));
    // 边转换为索引形式，丢弃悬空边
    innerEdges = inputEdges
      .map((e) => ({
        source: idToIndex.get(e.source) ?? -1,
        target: idToIndex.get(e.target) ?? -1,
      }))
      .filter((e) => e.source !== -1 && e.target !== -1);

    nodes.value = innerNodes.map((n) => ({ ...n }));
    edges.value = inputEdges;
    alpha.value = 1;
    tickCount = 0;
  }

  // ── 单次 tick ────────────────────────────────────────────────────

  /** 执行一次仿真 tick：计算受力 → 更新速度 → 更新位置。 */
  function tick(): void {
    const n = innerNodes.length;
    if (n === 0) return;

    const { repulsion, springStrength, springLength, gravity, damping, centerX, centerY } = options;
    const currentAlpha = alpha.value;

    // 1. 节点间排斥力（O(n²)）
    for (let i = 0; i < n; i++) {
      const a = innerNodes[i];
      for (let j = i + 1; j < n; j++) {
        const b = innerNodes[j];
        let dx = a.x - b.x;
        let dy = a.y - b.y;
        let distSq = dx * dx + dy * dy;
        // 避免除零：距离过小时赋予最小值
        if (distSq < 0.01) {
          dx = Math.random() - 0.5;
          dy = Math.random() - 0.5;
          distSq = 0.01;
        }
        const dist = Math.sqrt(distSq);
        // 排斥力大小 = repulsion / dist²，方向沿 (a-b)/dist
        const force = repulsion / distSq;
        const fx = (dx / dist) * force;
        const fy = (dy / dist) * force;
        a.vx += fx;
        a.vy += fy;
        b.vx -= fx;
        b.vy -= fy;
      }
    }

    // 2. 边弹簧吸引力（胡克定律）
    for (const edge of innerEdges) {
      const a = innerNodes[edge.source];
      const b = innerNodes[edge.target];
      const dx = b.x - a.x;
      const dy = b.y - a.y;
      const dist = Math.sqrt(dx * dx + dy * dy) || 0.01;
      // 位移 = dist - springLength；力大小 = springStrength × 位移
      const displacement = dist - springLength;
      const fx = (dx / dist) * springStrength * displacement;
      const fy = (dy / dist) * springStrength * displacement;
      a.vx += fx;
      a.vy += fy;
      b.vx -= fx;
      b.vy -= fy;
    }

    // 3. 中心引力 + 速度阻尼 + 位置更新
    let maxDisp = 0;
    for (const node of innerNodes) {
      if (node.fixed) {
        node.vx = 0;
        node.vy = 0;
        continue;
      }
      // 中心引力
      node.vx += (centerX - node.x) * gravity;
      node.vy += (centerY - node.y) * gravity;
      // 阻尼
      node.vx *= damping;
      node.vy *= damping;
      // 温度衰减：限制单 tick 位移幅度
      let dx = node.vx * currentAlpha;
      let dy = node.vy * currentAlpha;
      // 位移上限，防止数值爆炸
      const dispMag = Math.sqrt(dx * dx + dy * dy);
      if (dispMag > MAX_DISPLACEMENT) {
        dx = (dx / dispMag) * MAX_DISPLACEMENT;
        dy = (dy / dispMag) * MAX_DISPLACEMENT;
      }
      node.x += dx;
      node.y += dy;
      const mag = Math.abs(dx) + Math.abs(dy);
      if (mag > maxDisp) maxDisp = mag;
    }

    // 4. 温度衰减（指数衰减）
    alpha.value = Math.max(0.02, currentAlpha * 0.985);
    tickCount++;

    // 5. 发布响应式快照
    nodes.value = innerNodes.map((node) => ({ ...node }));

    // 6. 收敛自动停止
    if (maxDisp < CONVERGE_THRESHOLD && tickCount > 30) {
      stop();
    }
  }

  // ── 运行控制 ──────────────────────────────────────────────────────

  /** 启动仿真（若已在运行则无操作）。 */
  function start(): void {
    if (running.value) return;
    running.value = true;
    const loop = (): void => {
      if (!running.value) return;
      tick();
      if (running.value) {
        rafId = requestAnimationFrame(loop);
      }
    };
    rafId = requestAnimationFrame(loop);
  }

  /** 停止仿真。 */
  function stop(): void {
    running.value = false;
    if (rafId !== null) {
      cancelAnimationFrame(rafId);
      rafId = null;
    }
  }

  /** 重置温度为 1 并重新启动（用于布局变动后重新收敛）。 */
  function reheat(): void {
    alpha.value = 1;
    if (!running.value) start();
  }

  // ── 节点交互 ──────────────────────────────────────────────────────

  /**
   * 固定节点到指定坐标（拖拽时调用）。
   *
   * @param id 节点 ID
   * @param x 画布坐标 x
   * @param y 画布坐标 y
   */
  function fixNode(id: string, x: number, y: number): void {
    const node = innerNodes.find((n) => n.id === id);
    if (!node) return;
    node.x = x;
    node.y = y;
    node.vx = 0;
    node.vy = 0;
    node.fixed = true;
    nodes.value = innerNodes.map((n) => ({ ...n }));
    // 拖拽时确保仿真运行，使其他节点跟随调整
    if (!running.value) reheat();
  }

  /** 释放节点（拖拽结束时调用）。 */
  function releaseNode(id: string): void {
    const node = innerNodes.find((n) => n.id === id);
    if (!node) return;
    node.fixed = false;
  }

  /** 更新仿真中心（画布尺寸变化时调用）。 */
  function setCenter(centerX: number, centerY: number): void {
    options.centerX = centerX;
    options.centerY = centerY;
  }

  /** 销毁仿真器，释放资源。 */
  function destroy(): void {
    stop();
    innerNodes = [];
    innerEdges = [];
    nodes.value = [];
    edges.value = [];
  }

  return {
    // 响应式状态（只读）
    nodes,
    edges,
    running,
    alpha,
    // 生命周期
    init,
    start,
    stop,
    reheat,
    destroy,
    // 交互
    fixNode,
    releaseNode,
    setCenter,
    // 调试
    tick,
  };
}
