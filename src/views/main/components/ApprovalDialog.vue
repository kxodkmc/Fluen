<script setup lang="ts">
/**
 * ApprovalDialog — 工具写操作确认弹窗（共享组件）。
 *
 * 当智能体（Motis / 学术助手）通过写工具（project_write / project_edit /
 * manuscript）发起操作时，后端推送审批请求事件，本组件将请求
 * 展示为浮动卡片；用户点击「应用」才真正写入，点击「拒绝」则拦截。
 *
 * 条目附带后端预演算 diff 时展示 +A −R 统计徽标与「查看变更」按钮，
 * 点击打开全屏 ApprovalDiffModal hunk 视图；无 diff 时回退截断摘要。
 *
 * 纯展示组件：待审批列表由 props 注入，用户决策通过 `resolve` 事件上抛，
 * 由父组件（持有 useMotisChat / useAIAssistant 实例）回传后端。
 */
import { computed, ref, watch } from 'vue';
import { useI18n } from '../../../i18n';
import type { PendingApproval } from '../composables/approvalTypes';
import ApprovalDiffModal from './ApprovalDiffModal.vue';

const props = defineProps<{
  /** 待审批的工具写操作列表。 */
  approvals: readonly PendingApproval[];
}>();

const emit = defineEmits<{
  (e: 'resolve', id: string, approved: boolean): void;
}>();

const { t } = useI18n();

/** 是否展示确认弹窗（有待审批项时）。 */
const visible = computed(() => props.approvals.length > 0);

/* ── 全屏 diff 模态 ─────────────────────────────────────────────────── */
/** 当前打开 diff 视图的条目 id（null = 未打开）。 */
const openDiffId = ref<string | null>(null);
/** 当前打开 diff 视图的条目（条目被 resolve / 清空后自动为 null）。 */
const openDiffApproval = computed(
  () => props.approvals.find((a) => a.id === openDiffId.value) ?? null,
);
// 条目从列表移除（决策完成或新一轮清空）时收起已打开的 diff 视图
watch(openDiffApproval, (approval) => {
  if (approval === null) openDiffId.value = null;
});

/** 上抛决策并收起对应条目的 diff 视图。 */
function resolve(id: string, approved: boolean): void {
  if (openDiffId.value === id) openDiffId.value = null;
  emit('resolve', id, approved);
}

/** action 显示名。 */
function actionLabel(action: unknown): string {
  switch (action) {
    case 'write':
      return t('main.motisPanel.approval.actionWrite');
    case 'edit':
      return t('main.motisPanel.approval.actionEdit');
    case 'update':
      return t('main.motisPanel.approval.actionUpdate');
    default:
      return String(action ?? '');
  }
}

/** 提取审批条目的摘要文本（仅无 diff 条目兜底；内容截断，避免刷屏）。 */
function summary(item: PendingApproval): string {
  const input = item.input ?? {};
  const action = input.action;
  if (action === 'edit') {
    return `${truncate(String(input.old_string ?? ''))} → ${truncate(String(input.new_string ?? ''))}`;
  }
  const content = input.content;
  return content === undefined ? '' : truncate(String(content));
}

/** 截断长文本（保留前后，中间省略）。 */
function truncate(text: string, max = 160): string {
  if (text.length <= max) return text;
  const half = Math.floor(max / 2);
  return `${text.slice(0, half)}…${text.slice(-half)}`;
}

function apply(id: string): void {
  resolve(id, true);
}

function reject(id: string): void {
  resolve(id, false);
}
</script>

<template>
  <Transition name="approval-fade">
    <div v-if="visible" class="approval-overlay" @click.self="() => {}">
      <div class="approval-card">
        <div class="approval-header">
          <span class="approval-title">{{ t('main.motisPanel.approval.title') }}</span>
          <span class="approval-count">{{ approvals.length }}</span>
        </div>
        <p class="approval-desc">{{ t('main.motisPanel.approval.desc') }}</p>

        <div class="approval-list">
          <div v-for="item in approvals" :key="item.id" class="approval-item">
            <div class="approval-item-meta">
              <span class="badge action">{{ actionLabel(item.input?.action) }}</span>
              <span class="badge path" :title="String(item.input?.path ?? '')">
                {{ String(item.input?.path ?? '') }}
              </span>
              <template v-if="item.diff">
                <span class="badge stats">
                  <span class="stat add">+{{ item.diff.added }}</span>
                  <span class="stat remove" :class="{ danger: item.diff.removed > 0 }">
                    −{{ item.diff.removed }}
                  </span>
                </span>
                <button class="btn view-diff" @click="openDiffId = item.id">
                  {{ t('main.motisPanel.approval.viewDiff') }}
                </button>
              </template>
            </div>
            <pre v-if="!item.diff && summary(item)" class="approval-summary">{{ summary(item) }}</pre>
            <div class="approval-item-actions">
              <button class="btn apply" @click="apply(item.id)">
                {{ t('main.motisPanel.approval.apply') }}
              </button>
              <button class="btn reject" @click="reject(item.id)">
                {{ t('main.motisPanel.approval.reject') }}
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  </Transition>

  <ApprovalDiffModal
    v-if="openDiffApproval"
    :approval="openDiffApproval"
    @close="openDiffId = null"
    @resolve="resolve"
  />
</template>

<style scoped>
.approval-overlay {
  position: absolute;
  inset: 0;
  z-index: 20;
  display: flex;
  align-items: flex-end;
  padding: 12px;
  background: color-mix(in srgb, var(--fluen-scrim, rgba(0, 0, 0, 0.35)) 35%, transparent);
  pointer-events: auto;
}

.approval-card {
  width: 100%;
  max-height: 60%;
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 14px;
  border-radius: 12px;
  background: var(--fluen-surface, #fff);
  border: 1px solid var(--fluen-border, rgba(0, 0, 0, 0.12));
  box-shadow: 0 8px 28px rgba(0, 0, 0, 0.18);
  overflow: hidden;
}

.approval-header {
  display: flex;
  align-items: center;
  gap: 8px;
}

.approval-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--fluen-text, #1f2329);
}

.approval-count {
  min-width: 20px;
  height: 20px;
  padding: 0 6px;
  border-radius: 10px;
  font-size: 12px;
  line-height: 20px;
  text-align: center;
  background: var(--fluen-primary, #4a6cf7);
  color: #fff;
}

.approval-desc {
  margin: 0;
  font-size: 12px;
  color: var(--fluen-text-secondary, #646a73);
}

.approval-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  overflow-y: auto;
}

.approval-item {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px 12px;
  border-radius: 8px;
  background: var(--fluen-surface-2, rgba(0, 0, 0, 0.04));
}

.approval-item-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.badge {
  flex-shrink: 0;
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 11px;
}

.badge.action {
  background: var(--fluen-primary, #4a6cf7);
  color: #fff;
}

.badge.path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  background: rgba(0, 0, 0, 0.08);
  color: var(--fluen-text-secondary, #646a73);
}

.badge.stats {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 2px 6px;
}

.badge.stats .stat {
  font-family: var(--fluen-font-mono, ui-monospace, monospace);
  font-weight: 600;
}

.badge.stats .stat.add {
  color: var(--fluen-diff-add-fg, #1a7f37);
}

.badge.stats .stat.remove {
  color: var(--fluen-text-secondary, #646a73);
}

.badge.stats .stat.remove.danger {
  color: var(--fluen-danger, #cf222e);
}

.btn.view-diff {
  margin-left: auto;
  flex-shrink: 0;
  padding: 2px 10px;
  border: 1px solid var(--fluen-border, rgba(0, 0, 0, 0.12));
  border-radius: 4px;
  background: transparent;
  font-size: 11px;
  color: var(--fluen-text-secondary, #646a73);
  cursor: pointer;
}

.btn.view-diff:hover {
  color: var(--fluen-text, #1f2329);
  border-color: var(--fluen-text-secondary, #646a73);
}

.approval-summary {
  margin: 0;
  padding: 6px 8px;
  border-radius: 6px;
  font-size: 12px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-all;
  background: rgba(0, 0, 0, 0.05);
  color: var(--fluen-text, #1f2329);
  max-height: 120px;
  overflow-y: auto;
}

.approval-item-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

.btn {
  padding: 5px 16px;
  border: none;
  border-radius: 6px;
  font-size: 13px;
  cursor: pointer;
}

.btn.apply {
  background: var(--fluen-primary, #4a6cf7);
  color: #fff;
}

.btn.reject {
  background: transparent;
  color: var(--fluen-text-secondary, #646a73);
  border: 1px solid var(--fluen-border, rgba(0, 0, 0, 0.12));
}

.approval-fade-enter-active,
.approval-fade-leave-active {
  transition: opacity 0.18s ease;
}

.approval-fade-enter-from,
.approval-fade-leave-to {
  opacity: 0;
}
</style>
