<script setup lang="ts">
/**
 * ApprovalDiffModal — 审批变更全屏 diff 视图。
 *
 * 论文文本量大，本视图永远只渲染变更块（hunk）：未变更文本折叠为
 * `··· n 行未变 ···` 占位行，绝不铺开。数据来自后端预演算的行级 diff
 * （见 composables/approvalTypes.ts 的 ApprovalDiff）。
 *
 * 底部「应用 / 拒绝」直通审批决策（resolve 事件上抛，由父组件回传后端）；
 * 「关闭」仅收起视图、不决策，条目保留在审批卡片中。
 */
import { computed, onBeforeUnmount, onMounted } from 'vue';
import { useI18n } from '../../../i18n';
import type { PendingApproval } from '../composables/approvalTypes';

const props = defineProps<{
  /** 审批条目（调用方保证 diff 必存在）。 */
  approval: PendingApproval;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'resolve', id: string, approved: boolean): void;
}>();

const { t } = useI18n();

const diff = computed(() => props.approval.diff);

/** 空行用空格占位，防止行高塌陷。 */
function lineText(text: string): string {
  return text === '' ? ' ' : text;
}

function handleKeydown(e: KeyboardEvent): void {
  if (e.key === 'Escape') emit('close');
}

function handleOverlayClick(e: MouseEvent): void {
  if (e.target === e.currentTarget) emit('close');
}

onMounted(() => window.addEventListener('keydown', handleKeydown));
onBeforeUnmount(() => window.removeEventListener('keydown', handleKeydown));
</script>

<template>
  <Transition name="diff-overlay">
    <div v-if="diff" class="dialog-overlay" tabindex="-1" @click="handleOverlayClick">
      <Transition name="diff-dialog" appear>
        <div class="diff-dialog" role="dialog" :aria-label="t('main.motisPanel.approval.diffTitle')">
          <!-- 头部：路径 + 增删统计 + 截断提示 -->
          <div class="diff-dialog__header">
            <div class="diff-dialog__heading">
              <span class="diff-dialog__title">{{ t('main.motisPanel.approval.diffTitle') }}</span>
              <span class="diff-dialog__path" :title="diff.path">{{ diff.path }}</span>
            </div>
            <div class="diff-dialog__stats">
              <span v-if="diff.truncated" class="diff-dialog__truncated">
                {{ t('main.motisPanel.approval.truncated') }}
              </span>
              <span class="stat add">+{{ diff.added }}</span>
              <span class="stat remove" :class="{ danger: diff.removed > 0 }">−{{ diff.removed }}</span>
              <button class="diff-dialog__close" :aria-label="t('main.motisPanel.approval.close')" @click="emit('close')">
                <svg viewBox="0 0 12 12" width="14" height="14">
                  <path d="M1 1l10 10M11 1L1 11" stroke="currentColor" stroke-width="1.5" />
                </svg>
              </button>
            </div>
          </div>

          <!-- 主体：仅渲染变更块，等宽字体、可滚动 -->
          <div class="diff-dialog__body">
            <template v-for="(hunk, hi) in diff.hunks" :key="hi">
              <div v-if="hunk.gap_before > 0" class="diff-gap">
                ··· {{ t('main.motisPanel.approval.unchangedLines', { n: hunk.gap_before }) }} ···
              </div>
              <div v-for="(line, li) in hunk.lines" :key="`${hi}-${li}`" class="diff-line" :class="line.kind">
                <span class="diff-line__sign">{{ line.kind === 'add' ? '+' : line.kind === 'del' ? '−' : ' ' }}</span>
                <span class="diff-line__text">{{ lineText(line.text) }}</span>
              </div>
            </template>
          </div>

          <!-- 底部：应用 / 拒绝 + 关闭（仅收起） -->
          <div class="diff-dialog__footer">
            <button class="btn close" @click="emit('close')">
              {{ t('main.motisPanel.approval.close') }}
            </button>
            <div class="diff-dialog__footer-actions">
              <button class="btn reject" @click="emit('resolve', approval.id, false)">
                {{ t('main.motisPanel.approval.reject') }}
              </button>
              <button class="btn apply" @click="emit('resolve', approval.id, true)">
                {{ t('main.motisPanel.approval.apply') }}
              </button>
            </div>
          </div>
        </div>
      </Transition>
    </div>
  </Transition>
</template>

<style scoped>
/* ── 遮罩层（遵循 AboutDialog 模态规范） ─────────────────────────────── */
.dialog-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.4);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
  z-index: 2000;
  display: flex;
  align-items: center;
  justify-content: center;
  outline: none;
}

/* ── 全屏对话框 ─────────────────────────────────────────────────────── */
.diff-dialog {
  width: min(960px, calc(100vw - 48px));
  height: calc(100vh - 64px);
  display: flex;
  flex-direction: column;
  background: var(--fluen-surface, #f7f8fa);
  border: 1px solid var(--fluen-hairline, #e5e7eb);
  border-radius: 12px;
  box-shadow: var(--fluen-shadow-modal, rgba(36, 36, 36, 0.08) 0px 12px 16px -4px);
  overflow: hidden;
  -webkit-app-region: no-drag;
}

/* ── 头部 ───────────────────────────────────────────────────────────── */
.diff-dialog__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--fluen-hairline, #e5e7eb);
  background: var(--fluen-surface, #f7f8fa);
}

.diff-dialog__heading {
  display: flex;
  align-items: baseline;
  gap: 10px;
  min-width: 0;
}

.diff-dialog__title {
  flex-shrink: 0;
  font-family: var(--fluen-font-sans, 'DM Sans', sans-serif);
  font-size: 14px;
  font-weight: 600;
  color: var(--fluen-ink, #0a0a0a);
}

.diff-dialog__path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--fluen-font-mono, ui-monospace, monospace);
  font-size: 12px;
  color: var(--fluen-slate, #646a73);
}

.diff-dialog__stats {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.diff-dialog__truncated {
  font-size: 12px;
  color: var(--fluen-stone, #8a8f98);
}

.stat {
  padding: 2px 8px;
  border-radius: 4px;
  font-family: var(--fluen-font-mono, ui-monospace, monospace);
  font-size: 12px;
  font-weight: 600;
}

.stat.add {
  background: var(--fluen-diff-add-bg, #e6f4ea);
  color: var(--fluen-diff-add-fg, #1a7f37);
}

.stat.remove {
  background: var(--fluen-surface-2, rgba(0, 0, 0, 0.06));
  color: var(--fluen-slate, #646a73);
}

.stat.remove.danger {
  background: var(--fluen-diff-del-bg, #fdecea);
  color: var(--fluen-danger, #cf222e);
}

.diff-dialog__close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--fluen-stone, #8a8f98);
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease;
}

.diff-dialog__close:hover {
  background: var(--fluen-hover, rgba(0, 0, 0, 0.06));
  color: var(--fluen-ink, #0a0a0a);
}

/* ── 主体：hunk 列表 ────────────────────────────────────────────────── */
.diff-dialog__body {
  flex: 1;
  overflow-y: auto;
  padding: 8px 0 16px;
  font-family: var(--fluen-font-mono, ui-monospace, monospace);
  font-size: 12.5px;
  line-height: 1.6;
  background: var(--fluen-canvas, #fff);
}

.diff-gap {
  padding: 2px 16px;
  font-size: 11px;
  color: var(--fluen-stone, #8a8f98);
  background: var(--fluen-surface-2, rgba(0, 0, 0, 0.03));
  user-select: none;
}

.diff-line {
  display: flex;
  align-items: flex-start;
  padding: 0 16px;
}

.diff-line__sign {
  flex-shrink: 0;
  width: 14px;
  user-select: none;
}

.diff-line__text {
  white-space: pre-wrap;
  word-break: break-all;
  color: var(--fluen-ink, #0a0a0a);
}

.diff-line.ctx .diff-line__text {
  color: var(--fluen-slate, #646a73);
}

.diff-line.add {
  background: var(--fluen-diff-add-bg, #e6f4ea);
}

.diff-line.add .diff-line__sign {
  color: var(--fluen-diff-add-fg, #1a7f37);
}

.diff-line.del {
  background: var(--fluen-diff-del-bg, #fdecea);
}

.diff-line.del .diff-line__sign {
  color: var(--fluen-danger, #cf222e);
}

/* ── 底部 ───────────────────────────────────────────────────────────── */
.diff-dialog__footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-top: 1px solid var(--fluen-hairline, #e5e7eb);
  background: var(--fluen-surface, #f7f8fa);
}

.diff-dialog__footer-actions {
  display: flex;
  gap: 8px;
}

/* 按钮规格复用 ApprovalDialog 的 .btn.apply / .btn.reject */
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

.btn.close {
  background: transparent;
  color: var(--fluen-text-secondary, #646a73);
  border: 1px solid var(--fluen-border, rgba(0, 0, 0, 0.12));
}

/* ── 过渡动画 ───────────────────────────────────────────────────────── */
.diff-overlay-enter-active,
.diff-overlay-leave-active {
  transition: opacity 0.2s ease;
}

.diff-overlay-enter-from,
.diff-overlay-leave-to {
  opacity: 0;
}

.diff-dialog-enter-active,
.diff-dialog-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}

.diff-dialog-enter-from,
.diff-dialog-leave-to {
  opacity: 0;
  transform: scale(0.98) translateY(-8px);
}
</style>
