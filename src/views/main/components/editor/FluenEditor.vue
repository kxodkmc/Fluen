<script setup lang="ts">
/**
 * FluenEditor — CodeMirror 6 编辑器组件（双栏左栏：MD 源码编辑）。
 *
 * 将 `useFluenEditor` composable（逻辑层）封装为开箱即用的 Vue 组件，
 * 负责挂载/卸载 CM6 实例，并将编辑器内部状态以事件形式上抛。
 *
 * 职责：
 *   - 在 `hostRef` 上挂载 CM6，初始内容来自 `props.md`
 *   - 订阅 `onActiveLineChange` / `onDocChange`，转发为 Vue 事件
 *   - 监听外部 `props.md` 变化（项目切换、保存归一化）并回写编辑器
 *   - 通过观察 `isSaving` 周期推断保存完成，成功时上抛 `save` 事件
 *
 * 不负责：保存触发（由 CM6 keymap 内部调用 composable.save）、
 * 内容持久化（由 composable + useProject 完成）、HTML 预览（由 FluenPreview 完成）。
 *
 * @example
 * ```vue
 * <FluenEditor :md="content" @doc-change="onMd" @update:dirty="onDirty"
 *              @active-line-change="onLine" @save="onSaved" />
 * ```
 */
import { ref, onMounted, onBeforeUnmount, watch } from 'vue';
import { useFluenEditor } from './composables/useFluenEditor';

const props = defineProps<{
  /** 待加载的 Markdown 文本。外部变化会同步到编辑器。 */
  md: string;
}>();

const emit = defineEmits<{
  /** 保存成功时触发（通过观察 isSaving 周期推断）。 */
  save: [];
  /** dirty 状态变化时触发。 */
  'update:dirty': [dirty: boolean];
  /** 光标活动行变化时触发（0-based 行号）。 */
  'active-line-change': [line: number];
  /** 文档内容变化时触发（完整 MD 文本，供右栏预览订阅）。 */
  'doc-change': [md: string];
}>();

/** CM6 挂载宿主元素。 */
const hostRef = ref<HTMLDivElement | null>(null);

/** 编辑器单例 composable。 */
const editor = useFluenEditor();

/* ── 事件订阅句柄 ─────────────────────────────────────────────────── */
let unsubActiveLine: (() => void) | null = null;
let unsubDocChange: (() => void) | null = null;

onMounted(() => {
  if (hostRef.value) {
    editor.mount(hostRef.value, props.md);
  }
  unsubActiveLine = editor.onActiveLineChange((line) => {
    emit('active-line-change', line);
  });
  unsubDocChange = editor.onDocChange((md) => {
    emit('doc-change', md);
    emit('update:dirty', editor.isDirty.value);
  });
});

onBeforeUnmount(() => {
  unsubActiveLine?.();
  unsubDocChange?.();
  editor.unmount();
});

/* ── 外部 md 变化 → 回写编辑器（项目切换、保存归一化等） ──────────── */
watch(
  () => props.md,
  (newMd) => {
    if (editor.getMd() !== newMd) {
      editor.setMd(newMd);
    }
  },
);

/* ── 保存完成检测 ─────────────────────────────────────────────────── */
/* 观察 isSaving 周期：true → false 且 isDirty 为 false 时，判定保存成功，
 * 上抛 `save` 事件。Phase 1 启发式，足以覆盖 Ctrl+S 触发的保存流程。 */
watch(
  () => editor.isSaving.value,
  (saving, prevSaving) => {
    if (prevSaving && !saving && !editor.isDirty.value) {
      emit('save');
    }
  },
);
</script>

<template>
  <div ref="hostRef" class="fluen-editor-host"></div>
</template>

<style scoped>
.fluen-editor-host {
  height: 100%;
  width: 100%;
  overflow: hidden;
}

.fluen-editor-host :deep(.cm-editor) {
  height: 100%;
  font-family: var(--fluen-font-mono, 'SF Mono', Consolas, monospace);
  font-size: 14px;
}

.fluen-editor-host :deep(.cm-scroller) {
  overflow: auto;
}

.fluen-editor-host :deep(.cm-content) {
  padding: 12px 16px;
}

.fluen-editor-host :deep(.cm-line) {
  padding: 1px 0;
}

/* 光标与选区 */
.fluen-editor-host :deep(.cm-cursor) {
  border-left-color: var(--fluen-accent, #007acc);
}

.fluen-editor-host :deep(.cm-selectionBackground) {
  background-color: var(--fluen-selection-bg, rgba(0, 122, 204, 0.2));
}

/* 活动行 */
.fluen-editor-host :deep(.cm-activeLine) {
  background-color: var(--fluen-active-line-bg, rgba(0, 0, 0, 0.03));
}

.fluen-editor-host :deep(.cm-activeLineGutter) {
  background-color: var(--fluen-active-line-bg, rgba(0, 0, 0, 0.03));
}

/* 标题样式（CM6 markdown 语法高亮已处理，这里仅加重字重） */
.fluen-editor-host :deep(.cm-header) {
  font-weight: 600;
}

/* 章节标记行（<!-- @sec_id:xxx -->）：对用户不可见，仅作章节切分元数据 */
.fluen-editor-host :deep(.fluen-sec-marker-line) {
  display: none;
}
</style>
