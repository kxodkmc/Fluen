<script setup lang="ts">
/**
 * ForceGraph — SVG 力导向网状图组件。
 *
 * 职责：
 *   - 渲染节点（圆形）与边（直线），按 `wiki_type` 着色
 *   - 鼠标滚轮缩放（以光标为中心）
 *   - 背景拖拽平移画布
 *   - 节点拖拽（通过回调写入仿真器 `fixNode`/`releaseNode`）
 *   - 节点悬停高亮（节点 + 邻接边 + 邻居节点）
 *   - 节点点击选中（emit `select-node`）
 *
 * 设计：
 *   - **纯展示 + 交互**：组件不拥有仿真器，仅消费 `nodes`/`edges` props
 *     与发出事件，由父组件（KnowledgeGraphView）持有仿真器实例
 *   - **坐标系**：仿真在 graph 坐标系（原点为中心）进行，
 *     SVG 通过 `transform="translate(panX,panY) scale(zoom)"` 映射到屏幕坐标
 *   - **性能**：节点数通常 < 1000，SVG 直接渲染即可；标签按缩放级别隐藏
 */
import { ref, computed, onMounted, onBeforeUnmount, watch } from 'vue';
import type { GraphNode, GraphEdge } from '../types';
import type { WikiType } from '../../../types/knowledgeBase';

const props = withDefaults(
  defineProps<{
    /** 图节点（来自仿真器的响应式快照）。 */
    nodes: GraphNode[];
    /** 图边列表。 */
    edges: GraphEdge[];
    /** 当前选中节点 ID（null 表示无选中）。 */
    selectedId?: string | null;
    /** 当前悬停节点 ID（null 表示无悬停）。 */
    hoveredId?: string | null;
  }>(),
  {
    selectedId: null,
    hoveredId: null,
  },
);

const emit = defineEmits<{
  /** 节点被点击。 */
  (e: 'select-node', id: string): void;
  /** 节点悬停变化。 */
  (e: 'hover-node', id: string | null): void;
  /** 节点拖拽中（坐标为 graph 坐标系）。 */
  (e: 'drag-node', id: string, x: number, y: number): void;
  /** 节点拖拽结束。 */
  (e: 'drag-end', id: string): void;
}>();

// ── 容器尺寸 ────────────────────────────────────────────────────────

const containerRef = ref<HTMLDivElement | null>(null);
const width = ref(800);
const height = ref(600);
let resizeObserver: ResizeObserver | null = null;

// ── 视图变换（pan / zoom） ──────────────────────────────────────────

const panX = ref(0);
const panY = ref(0);
const zoom = ref(1);
const MIN_ZOOM = 0.15;
const MAX_ZOOM = 4;

/** viewBox 字符串（覆盖整个屏幕坐标系区域）。 */
const viewBox = computed(() => `0 0 ${width.value} ${height.value}`);

/** 根 `<g>` 的 transform。 */
const graphTransform = computed(
  () => `translate(${panX.value} ${panY.value}) scale(${zoom.value})`,
);

// ── 类型配色 ────────────────────────────────────────────────────────

/** wiki_type → 主色 CSS 变量。 */
const TYPE_COLOR_VAR: Record<WikiType, string> = {
  concept: 'var(--fluen-info)',
  entity: 'var(--fluen-warning)',
  summary: 'var(--fluen-success-text)',
};

// ── 节点尺寸映射 ────────────────────────────────────────────────────

/** 节点基础半径。 */
const BASE_RADIUS = 8;
/** 每单位 degree 增加的半径。 */
const DEGREE_RADIUS_FACTOR = 1.2;
/** 节点半径上限。 */
const MAX_RADIUS = 22;

/** 计算节点显示半径（degree 越大节点越大）。 */
function nodeRadius(node: GraphNode): number {
  return Math.min(MAX_RADIUS, BASE_RADIUS + Math.sqrt(node.degree) * DEGREE_RADIUS_FACTOR);
}

// ── 节点位置索引（避免边渲染时 O(n) 查找） ─────────────────────────

/** nodeId → 节点对象 的映射（每帧随 nodes 变化重建）。 */
const nodeMap = computed<Map<string, GraphNode>>(() => {
  const map = new Map<string, GraphNode>();
  for (const n of props.nodes) map.set(n.id, n);
  return map;
});

// ── 高亮集合（悬停或选中时计算邻接） ───────────────────────────────

/** 当前活跃节点（悬停优先于选中，用于高亮判定）。 */
const activeId = computed(() => props.hoveredId ?? props.selectedId ?? null);

/** 邻接表：nodeId → Set<neighborId>。 */
const adjacency = computed(() => {
  const map = new Map<string, Set<string>>();
  for (const edge of props.edges) {
    if (!map.has(edge.source)) map.set(edge.source, new Set());
    if (!map.has(edge.target)) map.set(edge.target, new Set());
    map.get(edge.source)!.add(edge.target);
    map.get(edge.target)!.add(edge.source);
  }
  return map;
});

/** 活跃节点的邻居集合（含自身）。 */
const highlightSet = computed<Set<string>>(() => {
  const id = activeId.value;
  if (!id) return new Set();
  const neighbors = adjacency.value.get(id) ?? new Set<string>();
  return new Set([id, ...neighbors]);
});

/** 边是否高亮（任一端点为活跃节点）。 */
function isEdgeHighlighted(edge: GraphEdge): boolean {
  const id = activeId.value;
  return !!id && (edge.source === id || edge.target === id);
}

/** 节点是否高亮。 */
function isNodeHighlighted(node: GraphNode): boolean {
  const set = highlightSet.value;
  return set.size === 0 || set.has(node.id);
}

// ── 标签显示策略（按缩放级别与节点重要性） ─────────────────────────

/** 缩放低于此值时隐藏所有标签，避免重叠。 */
const LABEL_HIDE_ZOOM = 0.5;

/** 是否显示节点标签。 */
function showLabel(node: GraphNode): boolean {
  if (zoom.value < LABEL_HIDE_ZOOM) return false;
  // 高优先级：活跃节点或其邻居始终显示
  if (highlightSet.value.size > 0 && highlightSet.value.has(node.id)) return true;
  // 高连接度节点始终显示
  if (node.degree >= 3) return true;
  // 其余按缩放级别渐显
  return zoom.value > 0.9;
}

// ── 坐标转换：屏幕 → graph ──────────────────────────────────────────

/** 将屏幕坐标（相对容器）转换为 graph 坐标。 */
function screenToGraph(screenX: number, screenY: number): { x: number; y: number } {
  return {
    x: (screenX - panX.value) / zoom.value,
    y: (screenY - panY.value) / zoom.value,
  };
}

/** 获取指针相对容器的屏幕坐标。 */
function getPointerPos(event: PointerEvent): { x: number; y: number } {
  const rect = containerRef.value?.getBoundingClientRect();
  if (!rect) return { x: 0, y: 0 };
  return { x: event.clientX - rect.left, y: event.clientY - rect.top };
}

// ── 交互状态机 ──────────────────────────────────────────────────────

type DragMode = 'none' | 'pan' | 'node';

let dragMode: DragMode = 'none';
let dragNodeId: string | null = null;
let lastPointerX = 0;
let lastPointerY = 0;
/** 拖拽起始位置，用于区分点击与拖拽。 */
let dragStartX = 0;
let dragStartY = 0;
let dragMoved = false;

/** 缩放（以光标为中心）。 */
function onWheel(event: WheelEvent): void {
  event.preventDefault();
  const rect = containerRef.value?.getBoundingClientRect();
  if (!rect) return;
  const cursorX = event.clientX - rect.left;
  const cursorY = event.clientY - rect.top;
  // graph 坐标在缩放前后应映射到同一光标位置：
  //   cursor = graphX * newZoom + newPan
  //   newPan = cursor - graphX * newZoom
  const graphPos = screenToGraph(cursorX, cursorY);
  const factor = event.deltaY < 0 ? 1.15 : 1 / 1.15;
  const newZoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom.value * factor));
  panX.value = cursorX - graphPos.x * newZoom;
  panY.value = cursorY - graphPos.y * newZoom;
  zoom.value = newZoom;
}

/** 指针按下：判断是节点拖拽还是画布平移。 */
function onPointerDown(event: PointerEvent): void {
  if (event.button !== 0) return; // 仅响应左键
  const target = event.target as Element;
  const nodeEl = target.closest('[data-node-id]') as HTMLElement | null;
  const pos = getPointerPos(event);
  dragStartX = pos.x;
  dragStartY = pos.y;
  dragMoved = false;
  if (nodeEl) {
    dragMode = 'node';
    dragNodeId = nodeEl.dataset.nodeId ?? null;
    (event.currentTarget as Element).setPointerCapture(event.pointerId);
  } else {
    dragMode = 'pan';
    (event.currentTarget as Element).setPointerCapture(event.pointerId);
  }
  lastPointerX = pos.x;
  lastPointerY = pos.y;
}

/** 指针移动：根据模式执行节点拖拽或画布平移。 */
function onPointerMove(event: PointerEvent): void {
  if (dragMode === 'none') return;
  const pos = getPointerPos(event);
  const dx = pos.x - lastPointerX;
  const dy = pos.y - lastPointerY;
  if (Math.abs(pos.x - dragStartX) > 3 || Math.abs(pos.y - dragStartY) > 3) {
    dragMoved = true;
  }
  if (dragMode === 'pan') {
    panX.value += dx;
    panY.value += dy;
  } else if (dragMode === 'node' && dragNodeId) {
    const graphPos = screenToGraph(pos.x, pos.y);
    emit('drag-node', dragNodeId, graphPos.x, graphPos.y);
  }
  lastPointerX = pos.x;
  lastPointerY = pos.y;
}

/** 指针抬起：结束拖拽；若未移动则视为点击。 */
function onPointerUp(event: PointerEvent): void {
  const wasNode = dragMode === 'node';
  const wasNodeId = dragNodeId;
  if (wasNode && wasNodeId && !dragMoved) {
    emit('select-node', wasNodeId);
  }
  if (wasNode && wasNodeId && dragMoved) {
    emit('drag-end', wasNodeId);
  }
  (event.currentTarget as Element).releasePointerCapture?.(event.pointerId);
  dragMode = 'none';
  dragNodeId = null;
}

/** 鼠标离开节点：清除悬停。 */
function onNodeLeave(): void {
  emit('hover-node', null);
}

/** 鼠标进入节点：设置悬停。 */
function onNodeEnter(id: string): void {
  emit('hover-node', id);
}

// ── 视图控制（供父组件通过 ref 调用） ──────────────────────────────

/** 重置视图：居中并恢复默认缩放。 */
function resetView(): void {
  panX.value = width.value / 2;
  panY.value = height.value / 2;
  zoom.value = 1;
}

/** 放大一级。 */
function zoomIn(): void {
  zoom.value = Math.min(MAX_ZOOM, zoom.value * 1.25);
}

/** 缩小一级。 */
function zoomOut(): void {
  zoom.value = Math.max(MIN_ZOOM, zoom.value / 1.25);
}

/** 将指定节点居中显示（用于点击侧栏跳转）。 */
function centerOnNode(id: string): void {
  const node = props.nodes.find((n) => n.id === id);
  if (!node) return;
  panX.value = width.value / 2 - node.x * zoom.value;
  panY.value = height.value / 2 - node.y * zoom.value;
}

defineExpose({ resetView, zoomIn, zoomOut, centerOnNode });

// ── 生命周期：尺寸监听 ──────────────────────────────────────────────

onMounted(() => {
  if (!containerRef.value) return;
  const rect = containerRef.value.getBoundingClientRect();
  width.value = rect.width || 800;
  height.value = rect.height || 600;
  // 初始 pan：让 graph 原点居中
  panX.value = width.value / 2;
  panY.value = height.value / 2;
  resizeObserver = new ResizeObserver((entries) => {
    for (const entry of entries) {
      const cr = entry.contentRect;
      width.value = cr.width || 800;
      height.value = cr.height || 600;
    }
  });
  resizeObserver.observe(containerRef.value);
});

onBeforeUnmount(() => {
  resizeObserver?.disconnect();
  resizeObserver = null;
});

// 当画布尺寸变化且 pan 仍为初始 0 时，重新居中（首次布局完成）
let initialCentered = false;
watch(
  () => [width.value, height.value],
  () => {
    if (!initialCentered) {
      panX.value = width.value / 2;
      panY.value = height.value / 2;
      initialCentered = true;
    }
  },
);
</script>

<template>
  <div
    ref="containerRef"
    class="force-graph"
    @wheel.passive.prevent="onWheel"
    @pointerdown="onPointerDown"
    @pointermove="onPointerMove"
    @pointerup="onPointerUp"
    @pointercancel="onPointerUp"
  >
    <svg
      class="force-graph__svg"
      :viewBox="viewBox"
      preserveAspectRatio="xMidYMid meet"
    >
      <g :transform="graphTransform">
        <!-- ── 边层 ─────────────────────────────────────────────── -->
        <g class="force-graph__edges">
          <line
            v-for="edge in edges"
            :key="edge.id"
            :x1="nodeMap.get(edge.source)?.x ?? 0"
            :y1="nodeMap.get(edge.source)?.y ?? 0"
            :x2="nodeMap.get(edge.target)?.x ?? 0"
            :y2="nodeMap.get(edge.target)?.y ?? 0"
            class="force-graph__edge"
            :class="{ 'force-graph__edge--active': isEdgeHighlighted(edge) }"
          />
        </g>

        <!-- ── 节点层 ───────────────────────────────────────────── -->
        <g class="force-graph__nodes">
          <g
            v-for="node in nodes"
            :key="node.id"
            :data-node-id="node.id"
            class="force-graph__node"
            :class="{
              'force-graph__node--active': activeId === node.id,
              'force-graph__node--selected': selectedId === node.id,
              'force-graph__node--dim': !isNodeHighlighted(node),
            }"
            :transform="`translate(${node.x} ${node.y})`"
            @pointerenter="onNodeEnter(node.id)"
            @pointerleave="onNodeLeave"
          >
            <!-- 外环（选中/活跃时高亮） -->
            <circle
              :r="nodeRadius(node) + 4"
              class="force-graph__node-ring"
              :style="{ stroke: TYPE_COLOR_VAR[node.wiki_type] }"
              fill="none"
            />
            <!-- 主体 -->
            <circle
              :r="nodeRadius(node)"
              class="force-graph__node-circle"
              :style="{ fill: TYPE_COLOR_VAR[node.wiki_type] }"
            />
            <!-- 标签 -->
            <text
              v-if="showLabel(node)"
              class="force-graph__node-label"
              :y="nodeRadius(node) + 14"
              text-anchor="middle"
            >{{ node.title }}</text>
          </g>
        </g>
      </g>
    </svg>
  </div>
</template>

<style scoped>
.force-graph {
  width: 100%;
  height: 100%;
  overflow: hidden;
  background: var(--fluen-canvas);
  cursor: grab;
  touch-action: none;
}

.force-graph:active {
  cursor: grabbing;
}

.force-graph__svg {
  width: 100%;
  height: 100%;
  display: block;
}

/* ── 边 ─────────────────────────────────────────────────────────── */
.force-graph__edge {
  stroke: var(--fluen-hairline);
  stroke-width: 1;
  transition: stroke 0.15s ease, stroke-width 0.15s ease;
}

.force-graph__edge--active {
  stroke: var(--fluen-accent);
  stroke-width: 2;
}

/* ── 节点 ───────────────────────────────────────────────────────── */
.force-graph__node {
  cursor: pointer;
  transition: opacity 0.15s ease;
}

.force-graph__node--dim {
  opacity: 0.25;
}

.force-graph__node-circle {
  stroke: var(--fluen-surface);
  stroke-width: 1.5;
  transition: r 0.15s ease;
}

.force-graph__node-ring {
  stroke-width: 0;
  opacity: 0;
  transition: opacity 0.15s ease, stroke-width 0.15s ease;
}

.force-graph__node--active .force-graph__node-ring,
.force-graph__node--selected .force-graph__node-ring {
  opacity: 0.6;
  stroke-width: 2;
}

.force-graph__node--selected .force-graph__node-circle {
  stroke-width: 2.5;
  stroke: var(--fluen-ink);
}

/* ── 标签 ───────────────────────────────────────────────────────── */
.force-graph__node-label {
  font-family: var(--fluen-font-sans);
  font-size: 11px;
  fill: var(--fluen-ink);
  paint-order: stroke;
  stroke: var(--fluen-canvas);
  stroke-width: 3px;
  stroke-linejoin: round;
  pointer-events: none;
  user-select: none;
}

.force-graph__node--dim .force-graph__node-label {
  opacity: 0.4;
}
</style>
