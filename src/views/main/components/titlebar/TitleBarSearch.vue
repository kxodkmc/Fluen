<script setup lang="ts">
/**
 * TitleBarSearch — 顶部标题栏中央 Dock。
 *
 * 搜索/命令输入框，支持 `@` 智能体快捷对话：
 *
 *   1. 输入 `@` → 下方弹出智能体列表
 *   2. 输入 `@M` → 自动匹配 M 开头的智能体；
 *      若唯一匹配，输入框显示虚影补齐（如 `otis`），按 Tab 补全
 *   3. 点击列表项 → 补全为 `@<名称> `
 *   4. 输入 `@Motis 你好！` → 回车触发：
 *      展开右侧 Motis 面板 + 预填"你好！"到输入框 + 清空搜索栏
 *
 * 响应式：
 *   - 宽屏：完整输入框（图标 + 输入 + ⌘K 快捷键提示）
 *   - ≤ searchCollapse 断点：收缩为纯图标按钮
 */
import { ref, computed, watch, inject, nextTick } from 'vue';
import { useI18n } from '../../../../i18n';
import { MOTIS_CHAT_KEY } from '../motis/symbols';
import { useAgentRegistry } from '../../composables/useAgentRegistry';
import type { AgentEntry } from '../../composables/useAgentRegistry';

const { t } = useI18n();

const props = defineProps<{
  /** 是否处于窄屏图标态（隐藏输入框与占位文案）。 */
  compact?: boolean;
}>();

const emit = defineEmits<{
  (e: 'activate'): void;
}>();

/* ── Motis 状态（注入 MainView 提供的共享实例） ──────────────────── */
const motisChat = inject(MOTIS_CHAT_KEY, null);

/* ── 智能体注册表 ─────────────────────────────────────────────────── */
const { agents } = useAgentRegistry();

/* ── 搜索输入 ─────────────────────────────────────────────────────── */
const inputRef = ref<HTMLInputElement | null>(null);
const inputValue = ref('');

/* ── 智能体匹配状态 ───────────────────────────────────────────────── */

/** 是否正在匹配智能体（输入以 @ 开头且未含空格）。 */
const isMatchingAgent = computed(() => {
  return inputValue.value.startsWith('@') && !inputValue.value.includes(' ');
});

/** @ 后的查询文本（小写）。 */
const agentQuery = computed(() => {
  if (!isMatchingAgent.value) return '';
  return inputValue.value.slice(1);
});

/** 按前缀过滤的智能体列表。 */
const filteredAgents = computed<AgentEntry[]>(() => {
  if (!isMatchingAgent.value) return [];
  const query = agentQuery.value.toLowerCase();
  if (!query) return agents.value;
  return agents.value.filter((a) =>
    a.triggerName.toLowerCase().startsWith(query),
  );
});

/** 唯一匹配时的补齐信息。 */
const uniqueMatch = computed<{ agent: AgentEntry; remaining: string } | null>(() => {
  if (filteredAgents.value.length !== 1) return null;
  const agent = filteredAgents.value[0]!;
  const query = agentQuery.value;
  // 大小写精确匹配时才显示虚影（避免输入 m 时虚影显示 otis 但实际是 Motis）
  const full = agent.triggerName;
  const lowerQuery = query.toLowerCase();
  const lowerFull = full.toLowerCase();
  if (lowerFull.startsWith(lowerQuery) && lowerFull !== lowerQuery) {
    // 虚影显示 triggerName 在当前大小写下的剩余部分
    // 用 triggerName 的原始大小写，截取 query 长度
    return { agent, remaining: full.slice(query.length) };
  }
  return null;
});

/** 虚影文本（显示在输入文本后面的灰色补齐）。 */
const ghostText = computed(() => {
  if (!isMatchingAgent.value || !uniqueMatch.value) return '';
  return uniqueMatch.value.remaining;
});

/** 是否显示智能体列表。 */
const showAgentList = ref(false);

/** 列表选中项索引。 */
const selectedIndex = ref(0);

/* ── 监听输入，更新列表状态 ───────────────────────────────────────── */
watch(inputValue, () => {
  if (isMatchingAgent.value) {
    showAgentList.value = true;
    selectedIndex.value = 0;
  } else {
    showAgentList.value = false;
  }
});

/* ── 虚影显示用的完整文本（inputValue + ghostText） ──────────────── */
const ghostFullText = computed(() => {
  if (!ghostText.value) return '';
  return inputValue.value + ghostText.value;
});

/* ── 键盘处理 ─────────────────────────────────────────────────────── */

function onKeydown(e: KeyboardEvent): void {
  // Tab 补齐
  if (e.key === 'Tab' && ghostText.value && uniqueMatch.value) {
    e.preventDefault();
    inputValue.value = '@' + uniqueMatch.value.agent.triggerName + ' ';
    return;
  }

  // 智能体列表导航
  if (showAgentList.value && filteredAgents.value.length > 0) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectedIndex.value = (selectedIndex.value + 1) % filteredAgents.value.length;
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectedIndex.value =
        (selectedIndex.value - 1 + filteredAgents.value.length) % filteredAgents.value.length;
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      showAgentList.value = false;
      return;
    }
  }

  // 回车触发
  if (e.key === 'Enter') {
    e.preventDefault();
    handleEnter();
  }
}

/** 回车处理：解析 @Agent 消息 或 触发命令面板。 */
function handleEnter(): void {
  // 尝试匹配 @<AgentName> <message>
  const match = inputValue.value.match(/^@(\S+)\s([\s\S]*)$/);
  if (match) {
    const [, agentName, message] = match;
    const agent = agents.value.find(
      (a) => a.triggerName.toLowerCase() === agentName!.toLowerCase(),
    );
    if (agent) {
      triggerAgent(agent, message!);
      return;
    }
  }

  // 列表选中时回车 → 补全前缀
  if (showAgentList.value && filteredAgents.value[selectedIndex.value]) {
    selectAgent(filteredAgents.value[selectedIndex.value]!);
    return;
  }

  // 无匹配 → 触发命令面板
  emit('activate');
}

/** 触发智能体：直接发送消息 + 清空搜索栏（不跳转到侧边栏，共享同一对话）。 */
function triggerAgent(_agent: AgentEntry, message: string): void {
  // 直接发送消息，不切换面板——搜索栏与侧边栏共享同一对话，仅发送渠道不同
  motisChat?.send(message);
  inputValue.value = '';
  showAgentList.value = false;
}

/** 点击列表项：补全为 @<名称> 。 */
function selectAgent(agent: AgentEntry): void {
  inputValue.value = '@' + agent.triggerName + ' ';
  showAgentList.value = false;
  nextTick(() => {
    inputRef.value?.focus();
    // 将光标移到末尾
    const len = inputValue.value.length;
    inputRef.value?.setSelectionRange(len, len);
  });
}

/** 输入框失焦时延迟隐藏列表（允许点击列表项）。 */
function handleBlur(): void {
  setTimeout(() => {
    showAgentList.value = false;
  }, 150);
}

/** 输入框聚焦时，若正在匹配则显示列表。 */
function handleFocus(): void {
  if (isMatchingAgent.value) {
    showAgentList.value = true;
  }
}

/** 紧凑模式下点击触发命令面板；非紧凑模式聚焦输入框。 */
function handleTriggerClick(): void {
  if (props.compact) {
    emit('activate');
  } else {
    inputRef.value?.focus();
  }
}
</script>

<template>
  <div class="dock-search" data-tauri-drag-region>
    <div
      class="dock-search__trigger"
      :class="{ 'dock-search__trigger--compact': compact }"
      :title="compact ? t('main.titleBar.searchPlaceholder') : undefined"
      data-tauri-drag-region="false"
      @click="handleTriggerClick"
    >
      <svg
        class="dock-search__icon"
        viewBox="0 0 24 24"
        width="14"
        height="14"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
      >
        <circle cx="11" cy="11" r="7" />
        <path d="m20 20-3.5-3.5" />
      </svg>

      <div v-if="!compact" class="dock-search__input-wrapper">
        <!-- 虚影层：显示 inputValue + ghostText，灰色 -->
        <span v-if="ghostFullText" class="dock-search__ghost">{{ ghostFullText }}</span>
        <input
          ref="inputRef"
          v-model="inputValue"
          class="dock-search__input"
          :placeholder="t('main.titleBar.searchPlaceholder')"
          spellcheck="false"
          autocomplete="off"
          @keydown="onKeydown"
          @blur="handleBlur"
          @focus="handleFocus"
        />
      </div>

      <span v-if="!compact && !inputValue" class="dock-search__shortcut">⌘K</span>
    </div>

    <!-- 智能体下拉列表 -->
    <Transition name="agent-list">
      <ul
        v-if="showAgentList && filteredAgents.length > 0"
        class="agent-list"
        data-tauri-drag-region="false"
      >
        <li
          v-for="(agent, idx) in filteredAgents"
          :key="agent.id"
          class="agent-list__item"
          :class="{ 'agent-list__item--active': idx === selectedIndex }"
          @mousedown.prevent="selectAgent(agent)"
          @mouseenter="selectedIndex = idx"
        >
          <svg
            class="agent-list__icon"
            viewBox="0 0 24 24"
            width="20"
            height="20"
            fill="currentColor"
            v-html="agent.icon"
          />
          <div class="agent-list__info">
            <span class="agent-list__name">{{ agent.label }}</span>
            <span v-if="agent.description" class="agent-list__desc">{{ agent.description }}</span>
          </div>
          <kbd class="agent-list__hint">Tab</kbd>
        </li>
      </ul>
    </Transition>
  </div>
</template>

<style scoped>
.dock-search {
  position: relative;
  display: flex;
  align-items: center;
  height: 32px;
  padding: 0 6px;
  background: var(--fluen-surface);
  border: 1px solid var(--fluen-hairline);
  border-radius: 12px;
  box-shadow: var(--fluen-shadow-card);
  -webkit-app-region: drag;
  transition: border-color 0.15s ease, box-shadow 0.15s ease;
}

.dock-search:hover {
  border-color: var(--fluen-accent);
}

.dock-search__trigger {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  min-width: 28px;
  height: 26px;
  padding: 0 10px;
  color: var(--fluen-stone);
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  cursor: pointer;
  border-radius: 5px;
  transition: color 0.15s ease, background 0.15s ease;
  -webkit-app-region: no-drag;
}

.dock-search__trigger:hover {
  color: var(--fluen-ink);
  background: var(--fluen-hover);
}

.dock-search__trigger--compact {
  justify-content: center;
  padding: 0;
  width: 28px;
}

.dock-search__icon {
  flex-shrink: 0;
}

/* ── 输入框 + 虚影 ─────────────────────────────────────────────────── */
.dock-search__input-wrapper {
  position: relative;
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
}

.dock-search__ghost {
  position: absolute;
  left: 0;
  top: 50%;
  transform: translateY(-50%);
  color: var(--fluen-stone);
  opacity: 0.45;
  pointer-events: none;
  white-space: pre;
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  line-height: 1;
  user-select: none;
}

.dock-search__input {
  position: relative;
  width: 100%;
  border: none;
  outline: none;
  background: transparent;
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  cursor: text;
  -webkit-app-region: no-drag;
  z-index: 1;
}

.dock-search__input::placeholder {
  color: var(--fluen-stone);
}

.dock-search__shortcut {
  flex-shrink: 0;
  padding: 1px 5px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 4px;
  background: var(--fluen-canvas);
  color: var(--fluen-stone);
  font-family: var(--fluen-font-sans);
  font-size: 10px;
  font-weight: 500;
  line-height: 1.4;
}

/* ── 智能体下拉列表 ────────────────────────────────────────────────── */
.agent-list {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  right: 0;
  margin: 0;
  padding: 4px;
  list-style: none;
  background: var(--fluen-canvas);
  border: 1px solid var(--fluen-hairline);
  border-radius: 12px;
  box-shadow: var(--fluen-shadow-card);
  z-index: 100;
  -webkit-app-region: no-drag;
}

.agent-list__item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border-radius: 8px;
  cursor: pointer;
  transition: background 0.12s ease;
}

.agent-list__item--active {
  background: var(--fluen-hover);
}

.agent-list__icon {
  flex-shrink: 0;
  color: var(--fluen-accent);
}

.agent-list__info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.agent-list__name {
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  font-weight: 500;
  color: var(--fluen-ink);
  line-height: 1.3;
}

.agent-list__desc {
  font-family: var(--fluen-font-sans);
  font-size: 11px;
  font-weight: 400;
  color: var(--fluen-stone);
  line-height: 1.3;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.agent-list__hint {
  flex-shrink: 0;
  padding: 1px 5px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 4px;
  background: var(--fluen-surface);
  color: var(--fluen-stone);
  font-family: var(--fluen-font-sans);
  font-size: 10px;
  font-weight: 500;
  line-height: 1.4;
}

/* ── 列表过渡动画 ──────────────────────────────────────────────────── */
.agent-list-enter-active,
.agent-list-leave-active {
  transition:
    opacity 0.15s ease,
    transform 0.15s ease;
}

.agent-list-enter-from,
.agent-list-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
