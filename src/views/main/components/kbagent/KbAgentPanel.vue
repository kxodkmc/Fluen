<script setup lang="ts">
/**
 * KbAgentPanel — 知识库构建对话面板（Kb Agent）。
 *
 * 以 Motis 同款对话样式实时展示知识库构建过程：
 *   ┌──────────────────────────────┐
 *   │ 头部（标题 + 关闭）            │
 *   ├──────────────────────────────┤
 *   │ 任务切换条（活跃优先 + 最近完成）│
 *   ├──────────────────────────────┤
 *   │ MotisChatMessageList（复用）   │  思考/工具折叠时间线
 *   └──────────────────────────────┘
 *
 * 纯可视化面板：无输入区，构建由文献右键「加入知识库」触发，
 * 状态由 useKbAgentChat 模块级单例驱动（与 ReferencesPanel 共享）。
 *
 * @emits close - 关闭按钮点击时触发（由 RightPanel 收起面板）
 */
import { computed, onMounted } from 'vue';
import { useKbAgentChat } from '../../composables/useKbAgentChat';
import { useI18n } from '../../../../i18n';
import MotisChatMessageList from '../motis/MotisChatMessageList.vue';

const emit = defineEmits<{
  (e: 'close'): void;
}>();

const { t } = useI18n();

/* ── 共享会话状态（模块级单例，ReferencesPanel 触发时已注册监听） ──────── */
const kbChat = useKbAgentChat();
const { sessions, activeTaskId, focusTask } = kbChat;

/** 挂载时确保事件监听已注册（直接打开面板的场景）。 */
onMounted(() => {
  void kbChat.setupEventListeners();
});

/** 最新会话在前（活跃任务优先展示）。 */
const sortedSessions = computed(() =>
  [...sessions.value].sort((a, b) => b.startedAt - a.startedAt),
);

/** 当前聚焦的会话。 */
const activeSession = computed(
  () => sessions.value.find((s) => s.taskId === activeTaskId.value) ?? null,
);

/** 当前会话是否构建中（驱动尾部思考指示器）。 */
const isGenerating = computed(() => activeSession.value?.status === 'running');

/** 会话状态徽标文案。 */
function statusLabel(status: KbAgentSessionLike['status']): string {
  return t(`main.kbAgent.status.${status}`);
}

/** 会话状态类型别名（仅供 statusLabel 参数标注）。 */
type KbAgentSessionLike = { status: 'running' | 'completed' | 'failed' | 'cancelled' };
</script>

<template>
  <div class="kb-agent-panel">
    <!-- 头部 -->
    <div class="kb-agent-panel__header">
      <span class="kb-agent-panel__title">{{ t('main.kbAgent.title') }}</span>
      <button
        class="kb-agent-panel__close"
        :title="t('main.kbAgent.close')"
        @click="$emit('close')"
      >
        <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <line x1="18" y1="6" x2="6" y2="18" />
          <line x1="6" y1="6" x2="18" y2="18" />
        </svg>
      </button>
    </div>

    <!-- 任务切换条 -->
    <div v-if="sortedSessions.length > 0" class="kb-agent-panel__tasks">
      <button
        v-for="session in sortedSessions"
        :key="session.taskId"
        class="kb-agent-panel__task"
        :class="{
          'kb-agent-panel__task--active': session.taskId === activeTaskId,
          [`kb-agent-panel__task--${session.status}`]: true,
        }"
        :title="session.title"
        @click="focusTask(session.taskId)"
      >
        <span class="kb-agent-panel__task-dot" />
        <span class="kb-agent-panel__task-title">{{ session.title }}</span>
        <span class="kb-agent-panel__task-status">{{ statusLabel(session.status) }}</span>
      </button>
    </div>

    <!-- 对话区（复用 Motis 消息列表） -->
    <MotisChatMessageList
      v-if="activeSession && activeSession.messages.length > 0"
      :messages="activeSession.messages"
      :is-generating="isGenerating"
    />

    <!-- 空状态 -->
    <div v-else class="kb-agent-panel__empty">
      <div class="kb-agent-panel__empty-icon">
        <svg viewBox="0 0 24 24" width="32" height="32" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
          <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
        </svg>
      </div>
      <p class="kb-agent-panel__empty-hint">{{ t('main.kbAgent.emptyHint') }}</p>
    </div>
  </div>
</template>

<style scoped>
.kb-agent-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--fluen-surface);
  overflow: hidden;
  position: relative;
}

/* ── 头部 ─────────────────────────────────────────────────────────────── */
.kb-agent-panel__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
  border-bottom: 1px solid var(--fluen-hairline);
  flex-shrink: 0;
}

.kb-agent-panel__title {
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  font-weight: 600;
  color: var(--fluen-ink);
}

.kb-agent-panel__close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
}

.kb-agent-panel__close:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

/* ── 任务切换条 ───────────────────────────────────────────────────────── */
.kb-agent-panel__tasks {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 6px 8px;
  border-bottom: 1px solid var(--fluen-hairline);
  max-height: 132px;
  overflow-y: auto;
  flex-shrink: 0;
}

.kb-agent-panel__task {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 5px 8px;
  border: none;
  border-radius: 8px;
  background: transparent;
  cursor: pointer;
  text-align: left;
}

.kb-agent-panel__task:hover {
  background: var(--fluen-hover);
}

.kb-agent-panel__task--active {
  background: var(--fluen-hover);
}

.kb-agent-panel__task-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
  background: var(--fluen-stone);
}

.kb-agent-panel__task--running .kb-agent-panel__task-dot {
  background: var(--fluen-brand-coral);
  animation: kb-agent-pulse 1.4s ease-in-out infinite;
}

.kb-agent-panel__task--completed .kb-agent-panel__task-dot {
  background: var(--fluen-success, #4caf7d);
}

.kb-agent-panel__task--failed .kb-agent-panel__task-dot {
  background: var(--fluen-error);
}

@keyframes kb-agent-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.35; }
}

.kb-agent-panel__task-title {
  flex: 1;
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  color: var(--fluen-ink);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.kb-agent-panel__task-status {
  font-family: var(--fluen-font-sans);
  font-size: 10px;
  color: var(--fluen-stone);
  flex-shrink: 0;
}

/* ── 空状态 ───────────────────────────────────────────────────────────── */
.kb-agent-panel__empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  text-align: center;
}

.kb-agent-panel__empty-icon {
  color: var(--fluen-brand-coral);
  opacity: 0.6;
  margin-bottom: 4px;
}

.kb-agent-panel__empty-hint {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  color: var(--fluen-stone);
  max-width: 220px;
  line-height: 1.5;
}
</style>
