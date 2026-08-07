/**
 * 全局鼠标位置 composable（单例）。
 *
 * 作为鼠标位置的"单一数据源"，统一管理两个事件来源：
 *   - window mousemove（默认情况，覆盖普通 DOM 区域）
 *   - iframe mousemove 上报（通过 postMessage 转发，再由外层调用 setMousePosition）
 *
 * ## 为什么需要
 *
 * iframe 是独立浏览上下文，其内部 mousemove 事件**不会冒泡到父窗口**。
 * 当鼠标进入 iframe 区域（如 ReferenceContent 阅读器），外层 window 上的
 * mousemove 监听器完全失效。需要 iframe 内部监听并通过 postMessage 把坐标
 * 上报给父窗口，再由父窗口调用 `setMousePosition` 同步到全局状态。
 *
 * ## 设计原则
 *
 * - 单例：模块级 ref + lazy 初始化，任何组件 useMousePosition() 拿到同一份状态
 * - 轻量：mousemove 监听器为 passive，无防抖/节流（rAF 消费侧自行平滑）
 * - 可扩展：未来若引入 pointermove 或其他来源，只需新增 setMousePosition 调用
 *
 * @example
 * ```ts
 * const { mouseX, mouseY, setMousePosition } = useMousePosition();
 *
 * // 监听全局鼠标位置
 * watch([mouseX, mouseY], ([x, y]) => { ... });
 *
 * // iframe 内 mousemove 上报时（坐标已转换为父窗口坐标系）
 * setMousePosition(iframeRect.left + innerX, iframeRect.top + innerY);
 * ```
 */

import { readonly, ref, type Ref } from 'vue';

// ── 模块级状态（单例） ──────────────────────────────────────────────

/** 当前鼠标 clientX（视口坐标系，含 iframe 上报转换后的坐标）。 */
const _mouseX = ref(0);
/** 当前鼠标 clientY（视口坐标系，含 iframe 上报转换后的坐标）。 */
const _mouseY = ref(0);

/** 是否已初始化 window 监听器（避免重复注册）。 */
let _initialized = false;

// ── 内部工具 ───────────────────────────────────────────────────────

/** window mousemove 处理器（passive，无副作用）。 */
function _handleMouseMove(e: MouseEvent): void {
  _mouseX.value = e.clientX;
  _mouseY.value = e.clientY;
}

/** 确保 window 监听器仅注册一次（lazy 初始化）。 */
function _ensureInit(): void {
  if (_initialized) return;
  _initialized = true;
  window.addEventListener('mousemove', _handleMouseMove, { passive: true });
}

// ── composable ─────────────────────────────────────────────────────

/** 鼠标位置只读视图类型。 */
type ReadonlyMouseRef = Readonly<Ref<number>>;

export function useMousePosition(): {
  /** 鼠标 clientX（只读）。 */
  mouseX: ReadonlyMouseRef;
  /** 鼠标 clientY（只读）。 */
  mouseY: ReadonlyMouseRef;
  /**
   * 直接设置鼠标位置（供 iframe mousemove 上报使用）。
   *
   * 坐标必须已转换为父窗口坐标系（即加上 iframe 的 bounding rect 偏移）。
   */
  setMousePosition: (x: number, y: number) => void;
} {
  _ensureInit();
  return {
    mouseX: readonly(_mouseX),
    mouseY: readonly(_mouseY),
    setMousePosition: (x: number, y: number) => {
      _mouseX.value = x;
      _mouseY.value = y;
    },
  };
}
