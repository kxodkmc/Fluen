<script setup lang="ts">
/**
 * ReferencesPanel — 参考文献面板。
 *
 * 功能：
 *   - 展示当前项目的文献列表（含状态：待处理 / 处理中 / 已完成 / 失败）
 *   - 导入按钮：调用系统文件选择对话框，支持多选 PDF / 图片
 *   - 导入进度通过任务队列通知系统在状态栏实时展示
 *   - 删除文献（同时清理 raw / md / resource）
 *   - 重试失败导入
 *   - 内联编辑标题
 *   - 右键上下文菜单（可扩展，当前提供「加入知识库」）
 *   - 文献条目三行布局：文件名 / 作者信息占位 / 状态徽标（可扩展）
 *
 * 数据来源：
 *   - `useReferences` composable：文献列表与导入状态
 *   - `useProject` composable（单例）：当前项目路径
 *   - `useKnowledgeBase` composable：知识库构建状态（跨组件共享）
 *
 * 事件流：
 *   - 批量导入采用 fire-and-forget 模型：`importFiles` 立即返回任务句柄，
 *     进度与结果通过 `reference:*` 事件推送，由 composable 注册到任务队列。
 *   - 知识库构建通过 `knowledge_build_start` 入队，`kb-build:*` 事件
 *     由 `useKnowledgeBase` 监听并更新 `buildStatusOf(refId)`。
 */
import { ref, computed, onMounted, watch, inject } from 'vue';
import type { ComponentPublicInstance } from 'vue';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { useReferences } from '../../../../../composables/useReferences';
import { useProject } from '../../../../../composables/useProject';
import { useKnowledgeBase } from '../../../../../composables/useKnowledgeBase';
import { useI18n } from '../../../../../i18n';
import { MAIN_LAYOUT_KEY } from '../../../composables/useMainLayout';
import type { ReferenceEntry, ReferenceStatus } from '../../../../../types/references';
import ContextMenu, { type ContextMenuItem } from '../../../../../components/ContextMenu.vue';

const { t } = useI18n();
const {
  references,
  isImporting,
  error,
  loadReferences,
  importFiles,
  deleteReference,
  retryImport,
  updateTitle,
  setupEventListeners,
} = useReferences();
const { currentProject } = useProject();
const {
  buildStatusOf,
  startBuild,
  loadExistingStatus,
  setupEventListeners: setupKbEventListeners,
} = useKnowledgeBase();

/** 主界面布局（inject 自 MainView，用于打开阅读器标签页）。 */
const layout = inject(MAIN_LAYOUT_KEY, null);

/** 当前项目路径（响应式）。 */
const projectPath = computed(() => currentProject.value?.project_path ?? '');

/** 是否有打开的项目。 */
const hasProject = computed(() => !!projectPath.value);

// ── 标题内联编辑 ────────────────────────────────────────────────────

/** 正在编辑标题的文献 ID。 */
const editingId = ref<string | null>(null);
/** 编辑中的标题值。 */
const editingTitle = ref('');

/** 文件选择对话框过滤器（PDF / 图片）。 */
const DIALOG_FILTERS = [
  { name: 'PDF / Images', extensions: ['pdf', 'jpg', 'jpeg', 'png', 'bmp', 'tiff', 'tif', 'webp'] },
];

// ── 右键上下文菜单 ──────────────────────────────────────────────────

/** 菜单是否可见。 */
const ctxMenuVisible = ref(false);
/** 菜单视窗 X 坐标。 */
const ctxMenuX = ref(0);
/** 菜单视窗 Y 坐标。 */
const ctxMenuY = ref(0);
/** 菜单作用的目标文献 ID（用于 select 回调定位）。 */
const ctxMenuTargetId = ref<string | null>(null);

/** 「加入知识库」菜单项 SVG path（书本图标）。 */
const ICON_ADD_TO_KB =
  'M4 19.5A2.5 2.5 0 0 1 6.5 17H20 M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z';

/** 「编辑标题」菜单项 SVG path（铅笔图标）。 */
const ICON_EDIT_TITLE =
  'M12 20h9M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4Z';

/** 「删除」菜单项 SVG path（垃圾桶图标）。 */
const ICON_DELETE =
  'M3 6h18M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2';

/** 「重试」菜单项 SVG path（刷新图标）。 */
const ICON_RETRY = 'M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5';

/**
 * 计算指定文献的右键菜单项（可扩展）。
 *
 * 当前包含：
 *   - 加入知识库（仅 completed 文献可用，已入库时禁用）
 *   - 编辑标题
 *   - 重试（仅 failed 状态显示）
 *   - 删除
 *
 * @returns 菜单项数组，包含分隔线
 */
function buildContextMenuItems(entry: ReferenceEntry): ContextMenuItem[] {
  const items: ContextMenuItem[] = [];
  const kbStatus = buildStatusOf(entry.id);

  // 加入知识库（仅已完成文献可加入；已入库或正在入库时禁用）
  const kbDisabled =
    entry.status !== 'completed' ||
    kbStatus === 'building' ||
    kbStatus === 'added';
  items.push({
    id: 'add-to-kb',
    label: t('main.sidebar.references.addToKnowledgeBase'),
    icon: ICON_ADD_TO_KB,
    disabled: kbDisabled,
  });

  items.push({ id: 'divider-actions', divider: true });

  // 编辑标题
  items.push({
    id: 'edit-title',
    label: t('main.sidebar.references.editTitle'),
    icon: ICON_EDIT_TITLE,
  });

  // 重试（仅 failed 状态显示）
  if (entry.status === 'failed' && !isImporting.value) {
    items.push({
      id: 'retry',
      label: t('main.sidebar.references.retry'),
      icon: ICON_RETRY,
    });
  }

  // 删除（危险动作）
  items.push({
    id: 'delete',
    label: t('main.sidebar.references.delete'),
    icon: ICON_DELETE,
    danger: true,
  });

  return items;
}

/** 右键事件：记录坐标并打开菜单。 */
function handleContextMenu(event: MouseEvent, entry: ReferenceEntry): void {
  event.preventDefault();
  ctxMenuTargetId.value = entry.id;
  ctxMenuX.value = event.clientX;
  ctxMenuY.value = event.clientY;
  ctxMenuVisible.value = true;
}

/** 菜单项选中回调。 */
function handleMenuSelect(id: string): void {
  const refId = ctxMenuTargetId.value;
  if (!refId) return;
  const entry = references.value.find((r) => r.id === refId);
  if (!entry) return;

  switch (id) {
    case 'add-to-kb':
      void handleAddToKnowledgeBase(entry);
      break;
    case 'edit-title':
      startEditTitle(entry);
      break;
    case 'retry':
      void handleRetry(entry);
      break;
    case 'delete':
      void handleDelete(entry);
      break;
    default:
      // 预留：未来菜单项扩展点
      break;
  }
}

/** 关闭菜单。 */
function handleCloseMenu(): void {
  ctxMenuVisible.value = false;
  ctxMenuTargetId.value = null;
}

// ── 知识库构建 ──────────────────────────────────────────────────────

/** 触发知识库构建。 */
async function handleAddToKnowledgeBase(entry: ReferenceEntry): Promise<void> {
  if (!projectPath.value) return;
  try {
    await startBuild(projectPath.value, entry.id);
  } catch (err) {
    console.error('[ReferencesPanel] 加入知识库失败:', err);
  }
}

// ── 生命周期 ─────────────────────────────────────────────────────────

onMounted(async () => {
  // 预注册事件监听，确保首批事件不丢失
  await setupEventListeners();
  await setupKbEventListeners();
  if (projectPath.value) {
    await loadReferences(projectPath.value);
    await loadExistingStatus(projectPath.value);
  }
});

// 项目切换时重新加载文献列表与知识库状态
watch(projectPath, async (path) => {
  if (path) {
    await loadReferences(path);
    await loadExistingStatus(path);
  } else {
    references.value = [];
  }
});

// ── 导入 ─────────────────────────────────────────────────────────────

/** 点击导入按钮：打开文件选择对话框并触发批量导入。 */
async function handleImport(): Promise<void> {
  if (!projectPath.value || isImporting.value) return;
  try {
    const selected = await openDialog({
      multiple: true,
      filters: DIALOG_FILTERS,
    });
    if (!selected) return;
    const filePaths = Array.isArray(selected) ? selected : [selected];
    if (filePaths.length === 0) return;
    await importFiles(projectPath.value, filePaths);
  } catch (err) {
    console.error('[ReferencesPanel] 导入失败:', err);
  }
}

// ── 删除 ─────────────────────────────────────────────────────────────

/** 删除文献。 */
async function handleDelete(entry: ReferenceEntry): Promise<void> {
  if (!projectPath.value) return;
  try {
    await deleteReference(projectPath.value, entry.id);
  } catch (err) {
    console.error('[ReferencesPanel] 删除失败:', err);
  }
}

// ── 重试 ─────────────────────────────────────────────────────────────

/** 重试失败导入。 */
async function handleRetry(entry: ReferenceEntry): Promise<void> {
  if (!projectPath.value || isImporting.value) return;
  try {
    await retryImport(projectPath.value, entry.id);
  } catch (err) {
    console.error('[ReferencesPanel] 重试失败:', err);
  }
}

// ── 标题编辑 ─────────────────────────────────────────────────────────

/** 进入标题编辑态。 */
function startEditTitle(entry: ReferenceEntry): void {
  editingId.value = entry.id;
  editingTitle.value = entry.title;
}

/**
 * 标题输入框挂载时聚焦并全选。
 *
 * 使用函数 ref 避免 v-for 内静态 ref 被收集为数组的问题。
 */
function onTitleInputMount(el: Element | ComponentPublicInstance | null): void {
  if (el instanceof HTMLInputElement) {
    el.focus();
    el.select();
  }
}

/** 确认标题修改。 */
async function confirmEditTitle(): Promise<void> {
  const id = editingId.value;
  const newTitle = editingTitle.value.trim();
  editingId.value = null;
  if (!id || !projectPath.value) return;
  if (!newTitle) return;
  try {
    await updateTitle(projectPath.value, id, newTitle);
  } catch (err) {
    console.error('[ReferencesPanel] 修改标题失败:', err);
  }
}

/** 取消标题编辑。 */
function cancelEditTitle(): void {
  editingId.value = null;
}

// ── 状态展示辅助 ─────────────────────────────────────────────────────

/**
 * 打开文献阅读器（在内容区以新标签页展示）。
 *
 * 仅已完成（completed）的文献可打开；已存在同 id 标签页则激活。
 */
function handleOpenReference(entry: ReferenceEntry): void {
  if (entry.status !== 'completed') return;
  layout?.openTab({
    id: `ref-${entry.id}`,
    title: entry.title,
    type: 'reference',
    referenceId: entry.id,
  });
}

/** 状态对应的图标 SVG path。 */
function statusIconPath(status: ReferenceStatus): string {
  switch (status) {
    case 'completed':
      return 'M5 13l4 4L19 7'; // 勾
    case 'failed':
      return 'M18 6 6 18M6 6l12 12'; // 叉
    case 'processing':
      return 'M21 12a9 9 0 1 1-6.219-8.56'; // 旋转
    default:
      return 'M12 8v4M12 16h.01'; // 待处理（感叹号）
  }
}

/** 状态 CSS 修饰类。 */
function statusClass(status: ReferenceStatus): string {
  return `ref-item--${status}`;
}

/** 状态文案。 */
function statusLabel(status: ReferenceStatus): string {
  switch (status) {
    case 'pending': return t('main.sidebar.references.statusPending');
    case 'processing': return t('main.sidebar.references.statusProcessing');
    case 'completed': return t('main.sidebar.references.statusCompleted');
    case 'failed': return t('main.sidebar.references.statusFailed');
  }
}

// ── 状态徽标（可扩展） ──────────────────────────────────────────────

/** 徽标视觉色调。 */
type BadgeTone = 'neutral' | 'info' | 'success' | 'warning' | 'error';

/** 单个徽标数据。 */
interface StatusBadge {
  /** 唯一标识（便于未来 key 跟踪）。 */
  id: string;
  /** 显示文案。 */
  label: string;
  /** 视觉色调。 */
  tone: BadgeTone;
}

/**
 * 计算指定文献的状态徽标列表（可扩展）。
 *
 * 当前包含：
 *   - 导入状态徽标（pending / processing / failed，completed 不显示）
 *   - 知识库徽标（added / building / failed）
 *   - 翻译徽标（占位，未来接入翻译模块时启用）
 *
 * 新增徽标类型时在此函数扩展即可，模板通过 v-for 自动渲染。
 */
function buildBadges(entry: ReferenceEntry): StatusBadge[] {
  const badges: StatusBadge[] = [];

  // 导入状态（completed 不显示，避免与「已入库」重复语义）
  if (entry.status !== 'completed') {
    const toneMap: Record<ReferenceStatus, BadgeTone> = {
      pending: 'neutral',
      processing: 'info',
      failed: 'error',
      completed: 'success',
    };
    badges.push({
      id: 'status',
      label: statusLabel(entry.status),
      tone: toneMap[entry.status],
    });
  }

  // 知识库状态
  const kbStatus = buildStatusOf(entry.id);
  switch (kbStatus) {
    case 'building':
      badges.push({
        id: 'kb',
        label: t('main.sidebar.references.badgeKnowledgeBuilding'),
        tone: 'info',
      });
      break;
    case 'added':
      badges.push({
        id: 'kb',
        label: t('main.sidebar.references.badgeKnowledgeBase'),
        tone: 'success',
      });
      break;
    case 'failed':
      badges.push({
        id: 'kb',
        label: t('main.sidebar.references.badgeKnowledgeFailed'),
        tone: 'error',
      });
      break;
    case 'idle':
    default:
      // 未入库不显示徽标
      break;
  }

  // 翻译徽标（占位，未来接入翻译模块时根据真实状态显示）
  // 示例扩展点：
  // if (translationStatusOf(entry.id) === 'translated') {
  //   badges.push({ id: 'translated', label: t('...badgeTranslated'), tone: 'success' });
  // }

  return badges;
}

/** 徽标 CSS 类。 */
function badgeClass(tone: BadgeTone): string {
  return `ref-badge--${tone}`;
}
</script>

<template>
  <div class="references-panel">
    <!-- ── 标题栏 ────────────────────────────────────────────────────── -->
    <div class="references-panel__header">
      <span class="references-panel__title">{{ t('main.sidebar.references.title') }}</span>
      <button
        class="references-panel__import-btn"
        :class="{ 'references-panel__import-btn--disabled': !hasProject || isImporting }"
        :disabled="!hasProject || isImporting"
        :title="t('main.sidebar.references.import')"
        @click="handleImport"
      >
        <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
          <path d="M12 5v14M5 12h14" />
        </svg>
      </button>
    </div>

    <!-- ── 错误提示 ──────────────────────────────────────────────────── -->
    <div v-if="error" class="references-panel__error" :title="error">
      {{ error }}
    </div>

    <!-- ── 文献列表 ──────────────────────────────────────────────────── -->
    <div v-if="hasProject && references.length > 0" class="references-panel__body">
      <div
        v-for="entry in references"
        :key="entry.id"
        class="ref-item"
        :class="[statusClass(entry.status), { 'ref-item--clickable': entry.status === 'completed' }]"
        @click="handleOpenReference(entry)"
        @contextmenu="handleContextMenu($event, entry)"
      >
        <!-- 状态图标 -->
        <div class="ref-item__status">
          <svg
            v-if="entry.status === 'processing'"
            class="icon-spin"
            viewBox="0 0 24 24"
            width="14"
            height="14"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
          >
            <path :d="statusIconPath(entry.status)" />
          </svg>
          <svg
            v-else
            viewBox="0 0 24 24"
            width="14"
            height="14"
            fill="none"
            stroke="currentColor"
            stroke-width="2.2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path :d="statusIconPath(entry.status)" />
          </svg>
        </div>

        <!-- 主体（三行布局） -->
        <div class="ref-item__body">
          <!-- 第一行：文件名（可编辑标题，编辑态切换为输入框） -->
          <input
            v-if="editingId === entry.id"
            :ref="onTitleInputMount"
            v-model="editingTitle"
            class="ref-item__title-input"
            type="text"
            @keydown.enter="confirmEditTitle"
            @keydown.esc="cancelEditTitle"
            @blur="confirmEditTitle"
            @click.stop
          />
          <span
            v-else
            class="ref-item__filename"
            :title="entry.title || entry.original_filename"
          >
            {{ entry.original_filename }}
          </span>

          <!-- 第二行：作者/信息（占位，尚未实现） -->
          <span class="ref-item__author-info">
            {{ t('main.sidebar.references.authorInfoPlaceholder') }}
          </span>

          <!-- 第三行：状态徽标（可扩展） -->
          <div class="ref-item__badges">
            <span
              v-for="badge in buildBadges(entry)"
              :key="badge.id"
              class="ref-badge"
              :class="badgeClass(badge.tone)"
            >
              {{ badge.label }}
            </span>
          </div>

          <!-- 失败原因（仅 failed 时显示，附在徽标下方） -->
          <span v-if="entry.status === 'failed' && entry.error" class="ref-item__error" :title="entry.error">
            {{ entry.error }}
          </span>
        </div>

        <!-- 悬停操作 -->
        <div class="ref-item__actions">
          <button
            v-if="editingId !== entry.id"
            class="ref-action"
            :title="t('main.sidebar.references.editTitle')"
            @click.stop="startEditTitle(entry)"
          >
            <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M12 20h9M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4Z" />
            </svg>
          </button>
          <button
            v-if="entry.status === 'failed' && !isImporting"
            class="ref-action"
            :title="t('main.sidebar.references.retry')"
            @click.stop="handleRetry(entry)"
          >
            <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5" />
            </svg>
          </button>
          <button
            class="ref-action ref-action--danger"
            :title="t('main.sidebar.references.delete')"
            @click.stop="handleDelete(entry)"
          >
            <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M3 6h18M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
            </svg>
          </button>
        </div>
      </div>
    </div>

    <!-- ── 空状态：有项目但无文献 ──────────────────────────────────── -->
    <div v-else-if="hasProject" class="references-panel__empty">
      <svg viewBox="0 0 24 24" width="32" height="32" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2Z" />
      </svg>
      <p class="references-panel__empty-text">
        {{ t('main.sidebar.references.empty') }}
      </p>
    </div>

    <!-- ── 空状态：未打开项目 ──────────────────────────────────────── -->
    <div v-else class="references-panel__empty">
      <svg viewBox="0 0 24 24" width="32" height="32" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="M4 6h16M4 12h12M4 18h8" />
      </svg>
      <p class="references-panel__empty-text">
        {{ t('main.sidebar.references.noProject') }}
      </p>
    </div>

    <!-- ── 右键上下文菜单 ──────────────────────────────────────────── -->
    <ContextMenu
      :visible="ctxMenuVisible"
      :items="ctxMenuTargetId ? buildContextMenuItems(references.find((r) => r.id === ctxMenuTargetId)!): []"
      :x="ctxMenuX"
      :y="ctxMenuY"
      @select="handleMenuSelect"
      @close="handleCloseMenu"
    />
  </div>
</template>

<style scoped>
.references-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

/* ── 标题栏 ────────────────────────────────────────────────────────── */
.references-panel__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  flex-shrink: 0;
}

.references-panel__title {
  font-family: var(--fluen-font-sans);
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--fluen-slate);
}

.references-panel__import-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  background: var(--fluen-accent);
  color: var(--fluen-on-accent);
  cursor: pointer;
  border-radius: 6px;
  transition: all 0.15s ease;
  flex-shrink: 0;
}

.references-panel__import-btn:hover:not(:disabled) {
  opacity: 0.85;
}

.references-panel__import-btn--disabled,
.references-panel__import-btn:disabled {
  background: var(--fluen-hover);
  color: var(--fluen-stone);
  cursor: not-allowed;
}

/* ── 错误提示 ──────────────────────────────────────────────────────── */
.references-panel__error {
  margin: 0 12px 8px;
  padding: 6px 10px;
  border-radius: 6px;
  background: var(--fluen-error-bg);
  color: var(--fluen-error);
  font-family: var(--fluen-font-sans);
  font-size: 0.72rem;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  flex-shrink: 0;
}

/* ── 文献列表 ──────────────────────────────────────────────────────── */
.references-panel__body {
  flex: 1;
  overflow-y: auto;
  padding: 0 8px 12px;
}

/* ── 文献条目 ──────────────────────────────────────────────────────── */
.ref-item {
  display: flex;
  align-items: flex-start;
  gap: 0.5rem;
  padding: 8px 8px;
  border-radius: 6px;
  transition: background 0.15s ease;
}

.ref-item:hover {
  background: var(--fluen-hover);
}

.ref-item--clickable {
  cursor: pointer;
}

.ref-item__status {
  flex-shrink: 0;
  margin-top: 2px;
}

.ref-item--completed .ref-item__status {
  color: var(--fluen-success-text);
}

.ref-item--failed .ref-item__status {
  color: var(--fluen-error);
}

.ref-item--processing .ref-item__status {
  color: var(--fluen-accent);
}

.ref-item--pending .ref-item__status {
  color: var(--fluen-stone);
}

.ref-item__body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

/* 第一行：文件名（主显示） */
.ref-item__filename {
  font-family: var(--fluen-font-sans);
  font-size: 0.8rem;
  font-weight: 500;
  color: var(--fluen-charcoal);
  line-height: 1.3;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.ref-item__title-input {
  width: 100%;
  padding: 2px 6px;
  border: 1px solid var(--fluen-accent);
  border-radius: 4px;
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 0.8rem;
  font-weight: 500;
  outline: none;
}

/* 第二行：作者/信息（占位） */
.ref-item__author-info {
  font-family: var(--fluen-font-sans);
  font-size: 0.68rem;
  color: var(--fluen-stone);
  line-height: 1.3;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 第三行：状态徽标（可扩展） */
.ref-item__badges {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 4px;
}

.ref-badge {
  display: inline-flex;
  align-items: center;
  padding: 1px 6px;
  border-radius: 9999px;
  font-family: var(--fluen-font-sans);
  font-size: 0.64rem;
  font-weight: 500;
  line-height: 1.4;
  background: var(--fluen-surface-deep);
  color: var(--fluen-steel);
  white-space: nowrap;
}

.ref-badge--neutral {
  background: var(--fluen-surface-deep);
  color: var(--fluen-steel);
}

.ref-badge--info {
  background: var(--fluen-info-bg);
  color: var(--fluen-info);
}

.ref-badge--success {
  background: var(--fluen-success-bg);
  color: var(--fluen-success-text);
}

.ref-badge--warning {
  background: var(--fluen-warning-bg);
  color: var(--fluen-warning);
}

.ref-badge--error {
  background: var(--fluen-error-bg);
  color: var(--fluen-error);
}

.ref-item__error {
  font-size: 0.66rem;
  color: var(--fluen-error);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* ── 悬停操作 ──────────────────────────────────────────────────────── */
.ref-item__actions {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
  opacity: 0;
  transition: opacity 0.15s ease;
}

.ref-item:hover .ref-item__actions {
  opacity: 1;
}

.ref-action {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
  border-radius: 4px;
  transition: all 0.15s ease;
}

.ref-action:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.ref-action--danger:hover {
  background: var(--fluen-error-bg);
  color: var(--fluen-error);
}

/* ── 空状态 ────────────────────────────────────────────────────────── */
.references-panel__empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--fluen-stone);
  padding: 24px 16px;
}

.references-panel__empty-text {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  color: var(--fluen-stone);
  text-align: center;
}

/* ── 旋转动画 ──────────────────────────────────────────────────────── */
.icon-spin {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}
</style>
