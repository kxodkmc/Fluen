<template>
  <div class="reference-content">
    <iframe
      ref="iframeRef"
      class="reference-iframe"
      sandbox="allow-scripts allow-same-origin allow-popups"
      :title="title"
    ></iframe>
  </div>
</template>

<script setup lang="ts">
/**
 * ReferenceContent —— 文献内容渲染区（iframe 隔离）。
 *
 * 接收已渲染的 HTML 片段与主题选项，通过 iframe contentDocument 写入：
 *   - HTML 变化：重写整个文档（含 CSS + body + MarksOverlay 脚本）
 *   - options 变化：仅更新 `#reader-style` 的 CSS（避免 iframe 重载闪烁）
 *
 * iframe 内的 MarksOverlay 脚本通过 postMessage 与外层通信：
 *   - 上行：selection:create / mark:click / marks:ready
 *   - 下行：marks:render / marks:clear / marks:remove / marks:scroll
 *
 * 用 iframe 而非 v-html：阅读器 CSS 含 body/html 级规则，
 * iframe 提供独立浏览上下文，彻底隔离样式污染。
 */

import { onBeforeUnmount, onMounted, ref, watch } from 'vue';
import type { ReaderOptions } from '../../../../types/reader';
import { buildReaderCss } from './readerTheme';
import {
  buildMarksOverlayScript,
  isInnerMessage,
  type SerializedAnchor,
  type SerializedMark,
  type SelectionRect,
} from './marks';
import { useMousePosition } from '../../../../composables/useMousePosition';
// KaTeX + texmath CSS（?inline → Vite 返回 CSS 字符串，注入 iframe <style>）
// KaTeX CSS 含字体 url()，Vite 解析为绝对路径，iframe 同源可加载。
import katexCss from 'katex/dist/katex.min.css?inline';
import texmathCss from 'markdown-it-texmath/css/texmath.css?inline';
// auto-render：扫描 DOM 文本节点中的 $...$ 并用 KaTeX 渲染。
// 用于处理 texmath 无法覆盖的场景（html:true 下 HTML 块如 <table> 内的公式）。
import renderMathInElement from 'katex/contrib/auto-render';

/** selection:create 事件载荷（外层据此构造 MarkAnchor）。 */
interface SelectionCreatePayload {
  anchor: SerializedAnchor;
  text: string;
  rect?: SelectionRect;
}

const props = defineProps<{
  /** 渲染后的 HTML 片段（不含 html/head/body 包裹）。 */
  html: string;
  /** 阅读器选项（主题/字号/行高）。 */
  options: ReaderOptions;
  /** iframe title（无障碍标签）。 */
  title?: string;
}>();

const emit = defineEmits<{
  'selection-create': [payload: SelectionCreatePayload];
  'mark-click': [markId: string];
  'marks-ready': [];
}>();

const iframeRef = ref<HTMLIFrameElement | null>(null);
let iframeReady = false;
let htmlDebounce: ReturnType<typeof setTimeout> | null = null;

// marks 状态：iframe 是否就绪 + 待刷新缓冲 + 最近一次下发的 marks
let marksReady = false;
let bufferedMarks: SerializedMark[] | null = null;
const lastMarks = ref<SerializedMark[]>([]);

/* ── iframe mousemove 上报 ─────────────────────────────────────────── */
/**
 * iframe 是独立浏览上下文，其内部 mousemove 事件不会冒泡到父窗口，
 * 导致 Mascot 眼球追踪、分割条拖拽等外层逻辑失效。
 *
 * 解决方案：在 iframe 内注入一段 mousemove 监听脚本，
 * 用 rAF 节流后将坐标通过 postMessage 上报给外层。
 * 外层收到后转换为父窗口坐标系，调用 useMousePosition.setMousePosition。
 */
const { setMousePosition } = useMousePosition();

/** iframe → 父窗口：鼠标移动上报消息。 */
interface MouseMoveForwardMessage {
  type: 'mascot:mousemove';
  /** iframe 视口内的 clientX。 */
  x: number;
  /** iframe 视口内的 clientY。 */
  y: number;
}

/** 判断消息是否为 iframe mousemove 上报。 */
function isMouseMoveForwardMessage(data: unknown): data is MouseMoveForwardMessage {
  if (!data || typeof data !== 'object') return false;
  const t = (data as { type?: unknown }).type;
  return t === 'mascot:mousemove';
}

/** 构建 iframe 内 mousemove 上报脚本（rAF 节流）。 */
function buildMouseMoveForwarderScript(): string {
  // 内部脚本使用字符串拼接（非模板字面量），避免外层 ${} 插值冲突
  return `(function(){
"use strict";
var rafId = null;
var lastX = 0, lastY = 0;
function flush() {
  rafId = null;
  parent.postMessage({ type: 'mascot:mousemove', x: lastX, y: lastY }, '*');
}
function onMove(e) {
  lastX = e.clientX;
  lastY = e.clientY;
  if (rafId !== null) return;
  rafId = requestAnimationFrame(flush);
}
window.addEventListener('mousemove', onMove, { passive: true });
})();`;
}

/** 构建完整 HTML 文档（含 CSS + body + MarksOverlay + mousemove 上报脚本）。 */
function buildDocument(html: string, options: ReaderOptions): string {
  const css = buildReaderCss(options);
  return `<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style id="math-style">${katexCss}\n${texmathCss}</style>
<style id="reader-style">${css}</style>
</head>
<body>
<div class="fluen-reader">${html}</div>
<script>${buildMarksOverlayScript()}<\/script>
<script>${buildMouseMoveForwarderScript()}<\/script>
</body>
</html>`;
}

/** 将 HTML 写入 iframe 文档（整体重写）。 */
function writeDocument(html: string, options: ReaderOptions): void {
  const iframe = iframeRef.value;
  if (!iframe || !iframeReady) return;
  const doc = iframe.contentDocument;
  if (!doc) return;
  doc.open();
  doc.write(buildDocument(html, options));
  doc.close();
  // auto-render：扫描 DOM 文本节点中的 $...$（texmath 已处理的为 HTML 元素，不会重复）
  // 覆盖 html:true 下 HTML 块（如 <table>）内的公式 —— texmath 不解析 html_block
  autoRenderMath(doc.body);
  // iframe 重载 → MarksOverlay 脚本重新初始化后会再次上报 marks:ready
  marksReady = false;
  bufferedMarks = lastMarks.value.length ? lastMarks.value : null;
}

/**
 * 在 iframe DOM 上运行 KaTeX auto-render。
 *
 * - 仅处理文本节点中的 `$...$` / `$$...$$`，不影响 texmath 已渲染的 HTML
 * - ignoredClasses: ['katex'] 跳过 texmath 输出（`<span class="katex">` 内部）
 * - throwOnError: false 避免无效 LaTeX 中断渲染
 */
function autoRenderMath(body: HTMLElement): void {
  try {
    renderMathInElement(body, {
      delimiters: [
        { left: '$$', right: '$$', display: true },
        { left: '$', right: '$', display: false },
      ],
      throwOnError: false,
      ignoredClasses: ['katex'],
    });
  } catch {
    // auto-render 失败不应阻断阅读器（texmath 渲染的公式仍正常）
  }
}

/** 仅更新 CSS（options 变化时，避免 iframe 重载）。 */
function updateCss(options: ReaderOptions): void {
  const iframe = iframeRef.value;
  if (!iframe || !iframeReady) return;
  const doc = iframe.contentDocument;
  if (!doc) return;
  const style = doc.getElementById('reader-style');
  if (style) {
    style.textContent = buildReaderCss(options);
  } else {
    // iframe 尚未初始化完成，回退到整体重写
    writeDocument(props.html, options);
  }
}

/** 直接下发 marks:render（不做就绪检查）。 */
function doRenderMarks(marks: SerializedMark[]): void {
  iframeRef.value?.contentWindow?.postMessage({ type: 'marks:render', marks }, '*');
}

/** 接收 iframe 内 MarksOverlay 上行的 postMessage。 */
function onMessage(event: MessageEvent): void {
  if (event.source !== iframeRef.value?.contentWindow) return;

  // iframe mousemove 上报：转换为父窗口坐标系后同步到全局鼠标位置
  // （Mascot 眼球追踪等订阅者据此更新）
  if (isMouseMoveForwardMessage(event.data)) {
    const iframe = iframeRef.value;
    if (iframe) {
      const rect = iframe.getBoundingClientRect();
      setMousePosition(rect.left + event.data.x, rect.top + event.data.y);
    }
    return;
  }

  if (!isInnerMessage(event.data)) return;
  const msg = event.data;
  switch (msg.type) {
    case 'selection:create':
      emit('selection-create', { anchor: msg.anchor, text: msg.text, rect: msg.rect });
      break;
    case 'mark:click':
      emit('mark-click', msg.markId);
      break;
    case 'marks:ready':
      marksReady = true;
      if (bufferedMarks) {
        doRenderMarks(bufferedMarks);
        bufferedMarks = null;
      }
      emit('marks-ready');
      break;
  }
}

onMounted(() => {
  iframeReady = true;
  window.addEventListener('message', onMessage);
  if (props.html) {
    writeDocument(props.html, props.options);
  }
});

onBeforeUnmount(() => {
  iframeReady = false;
  window.removeEventListener('message', onMessage);
  if (htmlDebounce) clearTimeout(htmlDebounce);
});

// HTML 变化：防抖 200ms 后重写文档
watch(
  () => props.html,
  (newHtml) => {
    if (htmlDebounce) clearTimeout(htmlDebounce);
    htmlDebounce = setTimeout(() => {
      writeDocument(newHtml, props.options);
    }, 200);
  },
);

// options 变化：仅更新 CSS（不重载 iframe）
watch(
  () => props.options,
  (newOptions) => {
    updateCss(newOptions);
  },
  { deep: true },
);

// 暴露给父组件的 marks 操作接口
defineExpose({
  /** 渲染高亮（iframe 未就绪时缓冲，就绪后立即下发）。 */
  renderMarks(marks: SerializedMark[]): void {
    lastMarks.value = marks;
    if (!marksReady) {
      bufferedMarks = marks;
    } else {
      doRenderMarks(marks);
    }
  },
  /** 清除所有高亮。 */
  clearMarks(): void {
    iframeRef.value?.contentWindow?.postMessage({ type: 'marks:clear' }, '*');
  },
  /** 移除单条高亮。 */
  removeMark(markId: string): void {
    iframeRef.value?.contentWindow?.postMessage({ type: 'marks:remove', markId }, '*');
  },
  /** 滚动到指定高亮。 */
  scrollToMark(markId: string): void {
    iframeRef.value?.contentWindow?.postMessage({ type: 'marks:scroll', markId }, '*');
  },
});
</script>

<style scoped>
.reference-content {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  background: var(--app-bg, #ffffff);
}

.reference-iframe {
  width: 100%;
  height: 100%;
  border: none;
  display: block;
}
</style>
