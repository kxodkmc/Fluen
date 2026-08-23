<script setup lang="ts">
/**
 * ApprovalDialog — 工具写操作确认弹窗（共享组件）。
 *
 * 当智能体（Motis / 学术助手）通过写工具（project_write / project_edit /
 * manuscript）发起操作时，后端推送审批请求事件，本组件将请求
 * 展示为浮动卡片；用户点击「应用」才真正写入，点击「拒绝」则拦截。
 *
 * 纯展示组件：待审批列表由 props 注入，用户决策通过 `resolve` 事件上抛，
 * 由父组件（持有 useMotisChat / useAIAssistant 实例）回传后端。
 */
import { computed } from 'vue';
import { useI18n } from '../../../i18n';

/** 待审批条目（结构兼容 useMotisChat / useAIAssistant 的 PendingApproval）。 */
export interface ApprovalItem {
  id: string;
  toolName: string;
  /** 输入参数（含 path / action / content 等）。 */
  input: Record<string, unknown>;
}

const props = defineProps<{
  /** 待审批的工具写操作列表。 */
  approvals: readonly ApprovalItem[];
}>();

const emit = defineEmits<{
  (e: 'resolve', id: string, approved: boolean): void;
}>();

const { t } = useI18n();

/** 是否展示确认弹窗（有待审批项时）。 */
const visible = computed(() => props.approvals.length > 0);

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

/** 提取审批条目的摘要文本（内容截断，避免刷屏）。 */
function summary(item: ApprovalItem): string {
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
  emit('resolve', id, true);
}

function reject(id: string): void {
  emit('resolve', id, false);
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
            </div>
            <pre v-if="summary(item)" class="approval-summary">{{ summary(item) }}</pre>
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
