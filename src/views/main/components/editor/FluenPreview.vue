<template>
  <iframe
    ref="iframeRef"
    class="fluen-preview-iframe"
    sandbox="allow-same-origin"
    title="preview"
  ></iframe>
</template>

<script setup lang="ts">
/**
 * FluenPreview — 只读 HTML 预览组件。
 *
 * 接收 MD 文本，经 300ms 防抖后调用后端 `editor_render_html` 渲染为 HTML，
 * 通过 `iframe` 注入只读预览容器。
 *
 * 为什么用 iframe 而非 v-html：
 *   后端渲染的 HTML 包含 `<style>` 标签（fluen-markup 的 DEFAULT_CSS），
 *   其中有 `body { max-width: 820px; margin: 2rem auto; padding: 0 1rem; }` 等
 *   规则。若用 v-html 注入到 .fluen-preview div 内，<style> 标签会全局生效，
 *   污染应用本身的 <body>，导致整个窗口布局错乱（body 被限制为 820px 宽）。
 *   iframe 提供独立的浏览上下文，<style> 只在 iframe 内生效，彻底隔离。
 *
 * CM6 文档是唯一数据源，预览为派生产物。
 *
 * 竞态处理：每次发起渲染递增 `currentRequestId`，仅当响应时的 id 仍为最新
 * 时才写入 iframe，避免快速输入下乱序响应覆盖最新结果。
 */
import { ref, watch, onBeforeUnmount, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';

const props = defineProps<{ md: string }>();

const iframeRef = ref<HTMLIFrameElement | null>(null);
let debounceTimer: ReturnType<typeof setTimeout> | null = null;
let currentRequestId = 0;
let iframeReady = false;

/**
 * 将后端返回的 HTML 写入 iframe 的文档。
 *
 * 直接操作 iframe.contentDocument，避免 srcdoc 重新加载整个 iframe。
 * 写入前检查 iframe 是否已初始化（onMounted 设置 iframeReady）。
 */
function writeToIframe(html: string): void {
  const iframe = iframeRef.value;
  if (!iframe) return;
  const doc = iframe.contentDocument;
  if (!doc) return;

  // 清空旧内容并写入新 HTML
  doc.open();
  doc.write(html);
  doc.close();
}

async function renderHtml(md: string): Promise<void> {
  const requestId = ++currentRequestId;
  try {
    const html = await invoke<string>('editor_render_html', { content: md });
    // Guard against race: only apply if this is the latest request
    if (requestId === currentRequestId && iframeReady) {
      writeToIframe(html);
    }
  } catch (err) {
    if (requestId === currentRequestId && iframeReady) {
      const msg = escapeHtml(extractErrorMessage(err));
      writeToIframe(
        `<div style="color:#d32f2f;padding:1em;background:rgba(211,47,47,0.05);border-radius:4px;font-family:system-ui,sans-serif;font-size:14px;">预览渲染失败：${msg}</div>`,
      );
    }
  }
}

/**
 * 从 Tauri invoke 抛出的错误中提取可读消息。
 *
 * Tauri command 错误通常形如 `{ message: "..." }`（对应后端 `EditorErrorResponse`），
 * `String(err)` 会得到 "[object Object]"，因此需显式读取 `.message` 字段；
 * 兜底处理字符串、Error 实例与未知形态。
 */
function extractErrorMessage(err: unknown): string {
  if (err === null || err === undefined) return '未知错误';
  if (typeof err === 'string') return err;
  if (err instanceof Error) return err.message;
  if (typeof err === 'object' && 'message' in err) {
    const msg = (err as { message: unknown }).message;
    if (typeof msg === 'string') return msg;
  }
  try {
    return JSON.stringify(err);
  } catch {
    return String(err);
  }
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

onMounted(() => {
  // 标记 iframe 就绪，允许后续 renderHtml 写入
  iframeReady = true;
  // 首次挂载时立即触发一次渲染（配合 watch immediate）
  if (props.md) {
    void renderHtml(props.md);
  }
});

onBeforeUnmount(() => {
  iframeReady = false;
  if (debounceTimer) {
    clearTimeout(debounceTimer);
  }
});

watch(
  () => props.md,
  (newMd) => {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }
    debounceTimer = setTimeout(() => {
      void renderHtml(newMd);
    }, 300);
  },
);
</script>

<style scoped>
.fluen-preview-iframe {
  height: 100%;
  width: 100%;
  border: none;
  background: transparent;
  display: block;
}
</style>
