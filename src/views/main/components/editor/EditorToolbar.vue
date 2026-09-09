<script setup lang="ts">
/**
 * EditorToolbar — MD 源码编辑工具栏。
 *
 * 排布参照文字处理软件的功能分组逻辑（编辑历史 ｜ 字体 ｜ 标题样式 ｜ 插入），
 * 组间以分隔线区隔。结构为「声明式 schema + 通用渲染」：新增工具只需在
 * `groups` 中追加一条定义（普通按钮 / 下拉 / 网格面板），渲染层与交互逻辑
 * 零改动；复合面板组件置于 `toolbar/` 子目录。
 *
 * 按钮点击时调用 `useFluenEditor()` 的编辑命令：行内格式经
 * `toggleFormat(kind)` 对当前选区/光标执行切换（行为由 `codemirror/formatting.ts`
 * 纯函数层实现）；标题按钮为下拉菜单，可从 H1-H6 中选择目标级别；表格经
 * `TableGridPicker` 选择尺寸与语法形态后由 `insertTable` 在光标处插入
 * （CM6 视图生成 `<f-tbl>` 块，预览编辑视图插入原生表格，由命令路由分发）。
 * 撤销/重做直接消费 composable 暴露的历史栈状态（不可用时按钮禁用）。
 *
 * 随源码/半预览/预览编辑视图显示（ContentPanel 控制），仅渲染模式不渲染。
 */
import { ref, onMounted, onUnmounted } from 'vue';
import { useI18n } from '../../../../i18n';
import { useFluenEditor } from './composables/useFluenEditor';
import type { TableSyntax } from './codemirror/tableModel';
import TableGridPicker from './toolbar/TableGridPicker.vue';
import MathPicker from './toolbar/MathPicker.vue';
import type { MarkdownFormatKind, HeadingLevel } from './codemirror/formatting';

const { t } = useI18n();
const editor = useFluenEditor();
const { canUndo, canRedo } = editor;

// ── 工具定义（schema） ─────────────────────────────────────────────

/** 普通按钮工具：glyph（文本字形）与 icon（SVG 路径组）二选一。 */
interface ToolButton {
  kind: 'button';
  id: string;
  glyph?: string;
  /** 字形修饰类（加粗/斜体/下划线形态），如 `editor-toolbar__glyph--bold`。 */
  glyphClass?: string;
  icon?: string[];
  label: string;
  /** Markdown 语法示例，附加在 tooltip 中；无则仅显示名称。 */
  hint?: string;
  run: () => void;
  /** 可用性判定；缺省视为恒可用（不可用时按钮禁用）。 */
  enabled?: () => boolean;
}

/** 下拉工具：按钮 + 弹出选项菜单（如标题级别）。 */
interface ToolDropdown {
  kind: 'dropdown';
  id: string;
  glyph: string;
  label: string;
  hint: string;
  options: { value: HeadingLevel; glyph: string; hint: string }[];
  apply: (value: HeadingLevel) => void;
}

/** 网格选择工具：弹出尺寸选择面板（如插入表格），选择结果经 onInsert 上交。 */
interface ToolGrid {
  kind: 'grid';
  id: string;
  icon: string[];
  label: string;
  maxRows: number;
  maxCols: number;
  onInsert: (rows: number, cols: number, syntax: TableSyntax) => void;
}

/** 公式面板工具：弹出符号/结构面板，插入目标由命令层自动判定。 */
interface ToolMath {
  kind: 'math';
  id: string;
  glyph: string;
  label: string;
}

type Tool = ToolButton | ToolDropdown | ToolGrid | ToolMath;

/** 工具分组，渲染为一段由分隔线区隔的按钮簇。 */
interface ToolGroup {
  id: string;
  tools: Tool[];
}

/** 字体组行内格式：kind 同时用作格式类型、i18n 键与字形修饰类名。 */
const inlineFormats: { kind: Exclude<MarkdownFormatKind, 'heading'>; glyph: string; hint: string }[] = [
  { kind: 'bold', glyph: 'B', hint: '**文本**' },
  { kind: 'italic', glyph: 'I', hint: '*文本*' },
  { kind: 'underline', glyph: 'U', hint: '++文本++' },
];

/** 标题下拉级别：glyph 为菜单显示，hint 为对应 Markdown 前缀示例。 */
const headingLevels: { value: HeadingLevel; glyph: string; hint: string }[] = [1, 2, 3, 4, 5, 6].map((level) => ({
  value: level as HeadingLevel,
  glyph: `H${level}`,
  hint: `${'#'.repeat(level)} 文本`,
}));

/**
 * 工具条 schema——按功能分类排布（编辑历史 ｜ 字体 ｜ 标题样式）。
 * 新增工具在此追加定义即可。
 */
const groups: ToolGroup[] = [
  {
    id: 'history',
    tools: [
      {
        kind: 'button',
        id: 'undo',
        icon: ['M9 14 4 9l5-5', 'M20 20v-7a4 4 0 0 0-4-4H4'],
        label: t('main.content.toolbar.undo'),
        run: () => editor.undo(),
        enabled: () => canUndo.value,
      },
      {
        kind: 'button',
        id: 'redo',
        icon: ['M15 14l5-5-5-5', 'M4 20v-7a4 4 0 0 1 4-4h12'],
        label: t('main.content.toolbar.redo'),
        run: () => editor.redo(),
        enabled: () => canRedo.value,
      },
    ],
  },
  {
    id: 'font',
    tools: inlineFormats.map((item) => ({
      kind: 'button' as const,
      id: item.kind,
      glyph: item.glyph,
      glyphClass: `editor-toolbar__glyph--${item.kind}`,
      label: t(`main.content.toolbar.${item.kind}`),
      hint: item.hint,
      run: () => editor.toggleFormat(item.kind),
    })),
  },
  {
    id: 'heading',
    tools: [
      {
        kind: 'dropdown',
        id: 'heading',
        glyph: 'H',
        label: t('main.content.toolbar.heading'),
        hint: 'H1–H6',
        options: headingLevels,
        apply: (level) => editor.toggleFormat('heading', level),
      },
    ],
  },
  {
    id: 'insert',
    tools: [
      {
        kind: 'math',
        id: 'math',
        glyph: '∫',
        label: t('main.content.toolbar.formula'),
      },
      {
        kind: 'grid',
        id: 'table',
        icon: [
          'M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z',
          'M3 9h18',
          'M9 9v12',
          'M15 9v12',
        ],
        label: t('main.content.toolbar.table'),
        maxRows: 8,
        maxCols: 10,
        onInsert: (rows, cols, syntax) => {
          editor.insertTable(rows, cols, syntax, t('main.content.toolbar.tableCaption'));
        },
      },
    ],
  },
];

// ── 弹出交互（支持任意 dropdown / grid 工具，同一时刻至多展开一个） ─

const openDropdown = ref<string | null>(null);

function toggleDropdown(id: string): void {
  openDropdown.value = openDropdown.value === id ? null : id;
}

function applyDropdown(tool: ToolDropdown, value: HeadingLevel): void {
  tool.apply(value);
  openDropdown.value = null;
}

/** 网格选择完成：执行插入并收起面板。 */
function onGridInsert(tool: ToolGrid, rows: number, cols: number, syntax: TableSyntax): void {
  tool.onInsert(rows, cols, syntax);
  openDropdown.value = null;
}

/**
 * 公式面板：行内/块级插入后收起面板；符号/结构插入保持面板展开，
 * 便于连续录入多个符号（面板按钮已 mousedown.prevent，单元格与
 * 文档光标在插入期间保持不动）。
 */
function onMathInsertInline(): void {
  editor.insertInlineMath();
  openDropdown.value = null;
}

function onMathInsertBlock(): void {
  editor.insertMathBlock();
  openDropdown.value = null;
}

function onMathInsertSymbol(latex: string): void {
  editor.insertMathSnippet(latex);
}

/** 点击下拉容器外部时关闭。 */
function handleDocumentClick(e: MouseEvent): void {
  if (!openDropdown.value) return;
  const target = e.target as Element | null;
  if (!target?.closest(`[data-dropdown="${openDropdown.value}"]`)) {
    openDropdown.value = null;
  }
}

/** Escape 键关闭。 */
function handleKeydown(e: KeyboardEvent): void {
  if (e.key === 'Escape' && openDropdown.value) {
    openDropdown.value = null;
  }
}

onMounted(() => {
  document.addEventListener('mousedown', handleDocumentClick);
  document.addEventListener('keydown', handleKeydown);
});

onUnmounted(() => {
  document.removeEventListener('mousedown', handleDocumentClick);
  document.removeEventListener('keydown', handleKeydown);
});
</script>

<template>
  <div class="editor-toolbar" role="toolbar" :aria-label="t('main.content.toolbar.label')">
    <div v-for="group in groups" :key="group.id" class="editor-toolbar__group">
      <template v-for="tool in group.tools" :key="tool.id">
        <!-- 普通按钮 -->
        <button
          v-if="tool.kind === 'button'"
          class="editor-toolbar__btn"
          type="button"
          :title="tool.hint ? `${tool.label}（${tool.hint}）` : tool.label"
          :aria-label="tool.label"
          :disabled="tool.enabled ? !tool.enabled() : false"
          @mousedown.prevent
          @click="tool.run()"
        >
          <svg v-if="tool.icon" class="editor-toolbar__icon" viewBox="0 0 24 24" aria-hidden="true">
            <path
              v-for="(d, i) in tool.icon"
              :key="i"
              :d="d"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
          <span v-else class="editor-toolbar__glyph" :class="tool.glyphClass">{{ tool.glyph }}</span>
        </button>

        <!-- 下拉工具 -->
        <div v-else-if="tool.kind === 'dropdown'" class="editor-toolbar__dropdown" :data-dropdown="tool.id">
          <button
            class="editor-toolbar__btn"
            :class="{ 'editor-toolbar__btn--active': openDropdown === tool.id }"
            type="button"
            :title="`${tool.label}（${tool.hint}）`"
            :aria-label="tool.label"
            :aria-expanded="openDropdown === tool.id"
            @mousedown.prevent
            @click="toggleDropdown(tool.id)"
          >
            <span class="editor-toolbar__glyph editor-toolbar__glyph--heading">{{ tool.glyph }}</span>
            <svg class="editor-toolbar__chevron" viewBox="0 0 16 16" aria-hidden="true">
              <path
                d="M4 6l4 4 4-4"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
          </button>

          <Transition name="toolbar-dropdown">
            <div
              v-if="openDropdown === tool.id"
              class="editor-toolbar__menu"
              role="menu"
              :aria-label="tool.label"
            >
              <button
                v-for="opt in tool.options"
                :key="opt.value"
                class="editor-toolbar__menu-item"
                role="menuitem"
                type="button"
                :title="`${tool.label} ${opt.value}（${opt.hint}）`"
                @mousedown.prevent
                @click="applyDropdown(tool, opt.value)"
              >
                <span class="editor-toolbar__menu-glyph">{{ opt.glyph }}</span>
                <span class="editor-toolbar__menu-hint">{{ opt.hint }}</span>
              </button>
            </div>
          </Transition>
        </div>

        <!-- 公式面板工具 -->
        <div v-else-if="tool.kind === 'math'" class="editor-toolbar__dropdown" :data-dropdown="tool.id">
          <button
            class="editor-toolbar__btn"
            :class="{ 'editor-toolbar__btn--active': openDropdown === tool.id }"
            type="button"
            :title="tool.label"
            :aria-label="tool.label"
            :aria-expanded="openDropdown === tool.id"
            @mousedown.prevent
            @click="toggleDropdown(tool.id)"
          >
            <span class="editor-toolbar__glyph">{{ tool.glyph }}</span>
            <svg class="editor-toolbar__chevron" viewBox="0 0 16 16" aria-hidden="true">
              <path
                d="M4 6l4 4 4-4"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
          </button>

          <Transition name="toolbar-dropdown">
            <div v-if="openDropdown === tool.id" class="editor-toolbar__panel">
              <MathPicker
                @insert-inline="onMathInsertInline()"
                @insert-block="onMathInsertBlock()"
                @insert="onMathInsertSymbol"
              />
            </div>
          </Transition>
        </div>

        <!-- 网格选择工具 -->
        <div v-else class="editor-toolbar__dropdown" :data-dropdown="tool.id">
          <button
            class="editor-toolbar__btn"
            :class="{ 'editor-toolbar__btn--active': openDropdown === tool.id }"
            type="button"
            :title="tool.label"
            :aria-label="tool.label"
            :aria-expanded="openDropdown === tool.id"
            @mousedown.prevent
            @click="toggleDropdown(tool.id)"
          >
            <svg class="editor-toolbar__icon" viewBox="0 0 24 24" aria-hidden="true">
              <path
                v-for="(d, i) in tool.icon"
                :key="i"
                :d="d"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
            <svg class="editor-toolbar__chevron" viewBox="0 0 16 16" aria-hidden="true">
              <path
                d="M4 6l4 4 4-4"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
          </button>

          <Transition name="toolbar-dropdown">
            <div v-if="openDropdown === tool.id" class="editor-toolbar__panel">
              <TableGridPicker
                :max-rows="tool.maxRows"
                :max-cols="tool.maxCols"
                @insert="(rows, cols, syntax) => onGridInsert(tool, rows, cols, syntax)"
              />
            </div>
          </Transition>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.editor-toolbar {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
  height: 36px;
  padding: 0 8px;
  background: var(--fluen-surface);
  border-bottom: 1px solid var(--fluen-hairline);
}

/* ── 分组（组间分隔线，功能分区的视觉边界） ───────────────────────── */
.editor-toolbar__group {
  display: flex;
  align-items: center;
  gap: 2px;
}

.editor-toolbar__group + .editor-toolbar__group {
  margin-left: 6px;
  padding-left: 8px;
  border-left: 1px solid var(--fluen-hairline);
}

/* ── 按钮 ─────────────────────────────────────────────────────────── */
.editor-toolbar__btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 28px;
  height: 26px;
  padding: 0 6px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--fluen-slate);
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  line-height: 1;
  cursor: pointer;
  user-select: none;
  transition: background 0.15s ease, color 0.15s ease;
}

.editor-toolbar__btn:hover:not(:disabled) {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.editor-toolbar__btn:active:not(:disabled) {
  background: var(--fluen-hairline);
}

.editor-toolbar__btn:focus-visible {
  outline: 2px solid var(--fluen-accent);
  outline-offset: 1px;
}

.editor-toolbar__btn:disabled {
  opacity: 0.38;
  cursor: default;
}

.editor-toolbar__btn--active {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

/* ── 字形与图标 ───────────────────────────────────────────────────── */
.editor-toolbar__glyph {
  display: inline-block;
  line-height: 1;
}

.editor-toolbar__glyph--bold {
  font-weight: 700;
}

.editor-toolbar__glyph--italic {
  font-style: italic;
}

.editor-toolbar__glyph--underline {
  text-decoration: underline;
  text-underline-offset: 2px;
}

.editor-toolbar__glyph--heading {
  font-weight: 600;
  font-size: 12px;
}

.editor-toolbar__icon {
  width: 15px;
  height: 15px;
}

.editor-toolbar__chevron {
  width: 12px;
  height: 12px;
  margin-left: 2px;
}

/* ── 下拉菜单 ─────────────────────────────────────────────────────── */
.editor-toolbar__dropdown {
  position: relative;
}

.editor-toolbar__menu {
  position: absolute;
  top: 100%;
  left: 0;
  margin-top: 4px;
  min-width: 132px;
  padding: 4px;
  background: var(--fluen-surface);
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  box-shadow: var(--fluen-shadow-card);
  z-index: 1000;
}

/* 网格选择等复合面板（内容自带留白，浮层只负责定位与外观） */
.editor-toolbar__panel {
  position: absolute;
  top: 100%;
  left: 0;
  margin-top: 4px;
  background: var(--fluen-surface);
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  box-shadow: var(--fluen-shadow-card);
  z-index: 1000;
}

.editor-toolbar__menu-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  width: 100%;
  padding: 5px 8px;
  border: none;
  background: transparent;
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  line-height: 1;
  text-align: left;
  cursor: pointer;
  border-radius: 4px;
  transition: background 0.12s ease;
}

.editor-toolbar__menu-item:hover {
  background: var(--fluen-hover);
}

.editor-toolbar__menu-glyph {
  font-weight: 600;
  font-size: 12px;
}

.editor-toolbar__menu-hint {
  color: var(--fluen-stone);
  font-size: 12px;
}

/* ── 过渡动画 ─────────────────────────────────────────────────────── */
.toolbar-dropdown-enter-active,
.toolbar-dropdown-leave-active {
  transition: opacity 0.15s ease, transform 0.15s ease;
}

.toolbar-dropdown-enter-from,
.toolbar-dropdown-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
