<script setup lang="ts">
/**
 * FluenWysiwygEditor — 预览编辑视图宿主组件（实验，TipTap 3 所见即所得）。
 *
 * 与 FluenEditor（CM6）平行：以 TipTap 编辑剥离章节标记后的 main.md 正文，
 * 序列化时按 H1 位置回插标记，产出完整的 main.md 文本。
 *
 * 职责：
 *   - 挂载 TipTap 编辑器（扩展清单见 tiptap/setup.ts）
 *   - 标记剥离/回插（tiptap/markers.ts）：进入前剥离，每次更新序列化回插
 *   - 编辑产物流出：emit `doc-change`（完整 MD）→ ContentPanel 更新 liveMd
 *   - 外部 md 变化（项目切换、保存归一化）→ setContent 回写编辑器
 *   - 保存：向 useFluenEditor 注册 content provider，使 Ctrl+S 保存 TipTap
 *     的内容而非隐藏 CM6 的陈旧内容
 *   - 命令路由：注册 command target，工具栏的格式/表格/公式/撤销命令
 *     在 wysiwyg 视图下转发给 TipTap（tiptap/commands.ts）
 *
 * 回声防护：序列化有损（空白差异），外部 prop 比对不能像 CM6 那样用全等，
 * 改为记录 `internalMd`（最近一次自身产出的文本）——prop 等于它时跳过回写。
 * IME 组合期间（`view.composing`）跳过程序化 setContent。
 *
 * 已接受的实验限制：
 *   - 章节整体重排序时，标记按位置配对，两章节的元数据随位置互换
 *   - 手工输入 `<!-- @sec_id:x -->` 文本可能产生幻影章节
 *   - 公式节点编辑：点击选中后经公式面板追加片段；不支持直接改写已有 latex
 *   - 撤销历史按视图独立（v-if 挂载，离开视图即丢弃）
 */
import { watch, onMounted, onBeforeUnmount } from 'vue';
import { EditorContent, useEditor } from '@tiptap/vue-3';
import { buildWysiwygExtensions } from './tiptap/setup';
import { createTiptapCommandTarget } from './tiptap/commands';
import { extractSecMarkers, reinsertSecMarkers, type SecMarkerState } from './tiptap/markers';
import { useFluenEditor } from './composables/useFluenEditor';

const props = defineProps<{
  /** 待加载的 Markdown 文本（完整 main.md，含章节标记）。 */
  md: string;
}>();

const emit = defineEmits<{
  /** 文档内容变化时触发（完整 MD 文本，含回插标记）。 */
  'doc-change': [md: string];
}>();

/** 编辑器单例（保存路由 / 脏标记广播）。 */
const fluen = useFluenEditor();

/** 最近一次自身序列化产出的文本（回声防护基线）。 */
let internalMd = '';
/** 当前文档的章节标记状态（非响应式：仅序列化时使用）。 */
let markerState: SecMarkerState = { body: '', markers: [] };

/** 序列化：TipTap 文档 → markdown → 回插章节标记；失败降级为正文原文。 */
function serializeWithMarkers(): string {
  const ed = tipEditor.value;
  if (!ed || ed.isDestroyed) return internalMd;
  const raw = ed.getMarkdown();
  return reinsertSecMarkers(raw, markerState.markers) ?? raw;
}

const tipEditor = useEditor({
  content: '',
  contentType: 'markdown',
  extensions: buildWysiwygExtensions(() => { void fluen.save(); }),
  editorProps: {
    attributes: {
      class: 'fluen-wysiwyg-prosemirror',
    },
  },
  onUpdate: () => {
    const md = serializeWithMarkers();
    internalMd = md;
    emit('doc-change', md);
    // 维护共享脏标记并广播给大纲/预览订阅者（不触碰 CM6 实例）
    fluen.notifyExternalDocChange(md);
  },
});

onMounted(() => {
  markerState = extractSecMarkers(props.md);
  internalMd = props.md;
  tipEditor.value?.commands.setContent(markerState.body, {
    contentType: 'markdown',
    emitUpdate: false,
  });
  // 保存期间由 provider 提供内容，Ctrl+S 保存的是 TipTap 的最新状态
  fluen.registerContentProvider(serializeWithMarkers);
  // 工具栏命令（格式/表格/公式/撤销）路由到 TipTap
  fluen.registerCommandTarget(createTiptapCommandTarget(() => tipEditor.value ?? null));
});

onBeforeUnmount(() => {
  fluen.registerCommandTarget(null);
  fluen.registerContentProvider(null);
});

/* ── 外部 md 变化 → 回写 TipTap（项目切换、保存归一化） ───────────── */
watch(
  () => props.md,
  (newMd) => {
    // 自身产出的回声（liveMd 由本组件的 doc-change 更新而来）跳过
    if (newMd === internalMd) return;
    // IME 组合期间不打断输入
    if (tipEditor.value?.view.composing) return;
    markerState = extractSecMarkers(newMd);
    internalMd = newMd;
    tipEditor.value?.commands.setContent(markerState.body, {
      contentType: 'markdown',
      emitUpdate: false,
    });
  },
);

/** 供模板渲染的编辑器实例引用（EditorContent 需要）。 */
const editorInstance = tipEditor;
</script>

<template>
  <div class="fluen-wysiwyg">
    <EditorContent v-if="editorInstance" :editor="editorInstance" class="fluen-wysiwyg__surface" />
  </div>
</template>

<style scoped>
.fluen-wysiwyg {
  height: 100%;
  width: 100%;
  overflow: hidden;
  background: var(--fluen-canvas, #ffffff);
}

.fluen-wysiwyg__surface {
  height: 100%;
  overflow-y: auto;
}

.fluen-wysiwyg :deep(.ProseMirror) {
  outline: none;
  min-height: 100%;
  padding: 24px 32px;
  font-family: var(--fluen-font-sans, inherit);
  font-size: 16px;
  line-height: 1.6;
  color: var(--fluen-ink, #0a0a0a);
  caret-color: var(--fluen-accent, #1d4ed8);
}

.fluen-wysiwyg :deep(.ProseMirror h1) {
  font-size: 1.7em;
  font-weight: 700;
  margin: 0.8em 0 0.4em;
}

.fluen-wysiwyg :deep(.ProseMirror h2) {
  font-size: 1.4em;
  font-weight: 600;
  margin: 0.7em 0 0.35em;
}

.fluen-wysiwyg :deep(.ProseMirror h3) {
  font-size: 1.2em;
  font-weight: 600;
  margin: 0.6em 0 0.3em;
}

.fluen-wysiwyg :deep(.ProseMirror p) {
  margin: 0.4em 0;
}

.fluen-wysiwyg :deep(.ProseMirror blockquote) {
  border-left: 3px solid var(--fluen-hairline, #e5e7eb);
  padding-left: 12px;
  color: var(--fluen-stone, #6b7280);
  margin: 0.6em 0;
}

.fluen-wysiwyg :deep(.ProseMirror pre) {
  background: var(--fluen-surface, #f7f8fa);
  border: 1px solid var(--fluen-hairline, #e5e7eb);
  border-radius: 8px;
  padding: 10px 14px;
  font-family: var(--fluen-font-mono, 'SF Mono', Consolas, monospace);
  font-size: 14px;
  overflow-x: auto;
}

.fluen-wysiwyg :deep(.ProseMirror code) {
  background: var(--fluen-surface, #f7f8fa);
  border-radius: 4px;
  padding: 1px 4px;
  font-family: var(--fluen-font-mono, 'SF Mono', Consolas, monospace);
  font-size: 0.9em;
}

.fluen-wysiwyg :deep(.ProseMirror ul),
.fluen-wysiwyg :deep(.ProseMirror ol) {
  padding-left: 1.4em;
}

/* 表格：三线表（与半预览 livePreview/theme.ts 的 .fluen-lp-table 规范一致：
   顶线 2px / 栏目线 1px / 底线 2px，无竖线、无底色 —— 学术表格惯例） */
.fluen-wysiwyg :deep(.ProseMirror table) {
  border-collapse: collapse;
  table-layout: fixed;
  width: 100%;
  margin: 0.8em 0;
  border-top: 2px solid var(--fluen-ink, #0a0a0a);
  border-bottom: 2px solid var(--fluen-ink, #0a0a0a);
}

.fluen-wysiwyg :deep(.ProseMirror th),
.fluen-wysiwyg :deep(.ProseMirror td) {
  border: none;
  padding: 4px 12px;
  vertical-align: top;
  position: relative;
  min-width: 4em;
}

/* 空单元格保底高度，插入后立即可见可点 */
.fluen-wysiwyg :deep(.ProseMirror th > p),
.fluen-wysiwyg :deep(.ProseMirror td > p) {
  margin: 0;
  min-height: 1.5em;
}

.fluen-wysiwyg :deep(.ProseMirror th) {
  font-weight: 600;
  text-align: left;
  border-bottom: 1px solid var(--fluen-ink, #0a0a0a);
}

/* 末行底线：三线表的第三线 */
.fluen-wysiwyg :deep(.ProseMirror tr:last-child td) {
  border-bottom: 2px solid var(--fluen-ink, #0a0a0a);
}

/* 单元格选中态（TipTap 表格交互） */
.fluen-wysiwyg :deep(.ProseMirror .selectedCell) {
  background-color: rgba(29, 78, 216, 0.08);
}

/* ── f-tbl 题注表格 ─────────────────────────────────────────────── */
.fluen-wysiwyg :deep(.fluen-ftbl) {
  margin: 0.8em 0;
}

/* 题注：表上方居中，辅助色小字（与学术排版惯例一致） */
.fluen-wysiwyg :deep(.fluen-table-caption) {
  text-align: center;
  font-size: 0.9em;
  color: var(--fluen-stone, #6b7280);
  margin: 0 0 0.35em;
}

/* ── 数学公式（KaTeX 渲染节点） ─────────────────────────────────── */
.fluen-wysiwyg :deep(.fluen-math-inline) {
  display: inline-block;
  vertical-align: baseline;
}

.fluen-wysiwyg :deep(.fluen-math-block) {
  text-align: center;
  margin: 0.7em 0;
  overflow-x: auto;
}

/* 空公式占位：插入后立即可见可选中 */
.fluen-wysiwyg :deep(.fluen-math-empty)::before {
  content: '公式';
  display: inline-block;
  padding: 0 10px;
  border: 1px dashed var(--fluen-hairline, #e5e7eb);
  border-radius: 4px;
  color: var(--fluen-stone, #6b7280);
  font-size: 0.85em;
}

/* 节点选中态（NodeSelection 自动附加 ProseMirror-selectednode） */
.fluen-wysiwyg :deep(.fluen-math-inline.ProseMirror-selectednode),
.fluen-wysiwyg :deep(.fluen-math-block.ProseMirror-selectednode) {
  outline: 2px solid var(--fluen-accent, #1d4ed8);
  outline-offset: 2px;
  border-radius: 2px;
}

/* 选中样式（TipTap 默认蓝色选区，替换为 accent 弱化版） */
.fluen-wysiwyg :deep(.ProseMirror-selectednode),
.fluen-wysiwyg :deep(.ProseMirror ::selection) {
  background-color: rgba(29, 78, 216, 0.15);
}
</style>
