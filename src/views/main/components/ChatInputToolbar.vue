<script setup lang="ts">
/**
 * ChatInputToolbar — 聊天输入区底部工具栏。
 *
 * 从左到右依次提供：添加文件、模式切换、模型切换、思考强度切换、发送/停止。
 * 仅负责展示与事件发射，状态由父组件或 useChatToolbar 管理。
 *
 * 响应式策略：
 *   - 默认：文字 + 图标并列
 *   - 窄面板：仅显示图标，下拉按钮保留简短文字
 *   - 超窄：隐藏思考强度下拉，只保留添加文件/模式/发送
 */
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { useI18n } from '../../../i18n';
import type { RightPanelId } from '../types';
import type { ToolbarModelOption, ThinkingIntensity } from '../composables/useChatToolbar';

const props = defineProps<{
  /** 当前模式。 */
  mode: RightPanelId;
  /** 当前模型显示名称。 */
  modelLabel: string;
  /** 可选模型列表。 */
  models: ToolbarModelOption[];
  /** 当前思考强度。 */
  thinkingIntensity: ThinkingIntensity;
  /** 是否可发送。 */
  canSend: boolean;
  /** 是否正在生成。 */
  isGenerating: boolean;
}>();

const emit = defineEmits<{
  (e: 'add-file'): void;
  (e: 'update:mode', mode: RightPanelId): void;
  (e: 'select-model', option: ToolbarModelOption): void;
  (e: 'update:thinkingIntensity', intensity: ThinkingIntensity): void;
  (e: 'send'): void;
  (e: 'stop'): void;
}>();

const { t } = useI18n();

/* ── 响应式断点 ─────────────────────────────────────────────────────── */
const toolbarWidth = ref(0);
const isCompact = computed(() => toolbarWidth.value < 340);
const isUltraCompact = computed(() => toolbarWidth.value < 260);

let resizeObserver: ResizeObserver | null = null;
const rootRef = ref<HTMLElement | null>(null);

onMounted(() => {
  if (!rootRef.value) return;
  resizeObserver = new ResizeObserver((entries) => {
    toolbarWidth.value = entries[0]?.contentRect.width ?? 0;
  });
  resizeObserver.observe(rootRef.value);
});

onUnmounted(() => {
  resizeObserver?.disconnect();
});

/* ── 下拉菜单状态 ───────────────────────────────────────────────────── */
const openMenu = ref<'mode' | 'model' | 'thinking' | null>(null);

function toggleMenu(menu: 'mode' | 'model' | 'thinking'): void {
  openMenu.value = openMenu.value === menu ? null : menu;
}

function closeMenus(): void {
  openMenu.value = null;
}

function selectMode(m: RightPanelId): void {
  emit('update:mode', m);
  closeMenus();
}

function selectModel(option: ToolbarModelOption): void {
  emit('select-model', option);
  closeMenus();
}

function selectThinking(intensity: ThinkingIntensity): void {
  emit('update:thinkingIntensity', intensity);
  closeMenus();
}

function handleSend(): void {
  if (!props.canSend || props.isGenerating) return;
  emit('send');
}

/* ── 文案映射 ───────────────────────────────────────────────────────── */
const modeLabel = computed(() =>
  props.mode === 'motis' ? t('main.chatToolbar.modeMotis') : t('main.chatToolbar.modeAssistant'),
);

const thinkingLabelMap: Record<ThinkingIntensity, string> = {
  default: 'Default',
  deep: 'Deep',
  light: 'Light',
};

const modeOptions: { value: RightPanelId; label: string; icon: string }[] = [
  { value: 'motis', label: t('main.chatToolbar.modeMotis'), icon: 'M12 22c5.523 0 10-4.477 10-10S17.523 2 12 2 2 6.477 2 12s4.477 10 10 10zm-4-9a1.5 1.5 0 1 1 0-3 1.5 1.5 0 0 1 0 3zm8 0a1.5 1.5 0 1 1 0-3 1.5 1.5 0 0 1 0 3zm-6.9 2.8c.8.9 2.1.9 2.9 0' },
  { value: 'assistant', label: t('main.chatToolbar.modeAssistant'), icon: 'M4 19.5A2.5 2.5 0 0 1 6.5 17H20M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z' },
];

const thinkingOptions: { value: ThinkingIntensity; label: string }[] = [
  { value: 'default', label: 'Default' },
  { value: 'deep', label: 'Deep' },
  { value: 'light', label: 'Light' },
];
</script>

<template>
  <div ref="rootRef" class="chat-toolbar">
    <!-- 添加文件 -->
    <button
      class="chat-toolbar__btn chat-toolbar__btn--icon"
      :title="t('main.chatToolbar.addFile')"
      @click="$emit('add-file')"
    >
      <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
        <path d="M12 5v14M5 12h14" />
      </svg>
      <span v-if="!isCompact" class="chat-toolbar__btn-text">{{ t('main.chatToolbar.addFile') }}</span>
    </button>

    <!-- 模式切换 -->
    <div class="chat-toolbar__dropdown">
      <button
        class="chat-toolbar__btn chat-toolbar__btn--pill"
        :class="{ 'chat-toolbar__btn--active': openMenu === 'mode' }"
        @click="toggleMenu('mode')"
      >
        <span class="chat-toolbar__label">{{ modeLabel }}</span>
        <svg class="chat-toolbar__caret" viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M6 9l6 6 6-6" />
        </svg>
      </button>
      <div v-if="openMenu === 'mode'" class="chat-toolbar__menu" @click.stop>
        <button
          v-for="opt in modeOptions"
          :key="opt.value"
          class="chat-toolbar__menu-item"
          :class="{ 'chat-toolbar__menu-item--active': props.mode === opt.value }"
          @click="selectMode(opt.value)"
        >
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            <path :d="opt.icon" />
          </svg>
          <span>{{ opt.label }}</span>
        </button>
      </div>
    </div>

    <!-- 模型切换 -->
    <div v-if="!isUltraCompact" class="chat-toolbar__dropdown">
      <button
        class="chat-toolbar__btn chat-toolbar__btn--pill"
        :class="{ 'chat-toolbar__btn--active': openMenu === 'model' }"
        @click="toggleMenu('model')"
      >
        <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" />
        </svg>
        <span class="chat-toolbar__label chat-toolbar__label--truncate">{{ modelLabel }}</span>
        <svg class="chat-toolbar__caret" viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M6 9l6 6 6-6" />
        </svg>
      </button>
      <div v-if="openMenu === 'model'" class="chat-toolbar__menu" @click.stop>
        <button
          v-for="opt in models"
          :key="opt.id"
          class="chat-toolbar__menu-item"
          :class="{ 'chat-toolbar__menu-item--active': modelLabel === opt.name }"
          @click="selectModel(opt)"
        >
          <span class="chat-toolbar__menu-title">{{ opt.name }}</span>
          <span class="chat-toolbar__menu-sub">{{ opt.providerName }}</span>
        </button>
        <div v-if="models.length === 0" class="chat-toolbar__menu-empty">
          {{ t('main.chatToolbar.noModels') }}
        </div>
      </div>
    </div>

    <!-- 思考强度 -->
    <div v-if="!isUltraCompact" class="chat-toolbar__dropdown">
      <button
        class="chat-toolbar__btn chat-toolbar__btn--pill"
        :class="{ 'chat-toolbar__btn--active': openMenu === 'thinking' }"
        @click="toggleMenu('thinking')"
      >
        <span class="chat-toolbar__label">{{ thinkingLabelMap[thinkingIntensity] }}</span>
        <svg class="chat-toolbar__caret" viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M6 9l6 6 6-6" />
        </svg>
      </button>
      <div v-if="openMenu === 'thinking'" class="chat-toolbar__menu" @click.stop>
        <button
          v-for="opt in thinkingOptions"
          :key="opt.value"
          class="chat-toolbar__menu-item"
          :class="{ 'chat-toolbar__menu-item--active': props.thinkingIntensity === opt.value }"
          @click="selectThinking(opt.value)"
        >
          {{ opt.label }}
        </button>
      </div>
    </div>

    <div class="chat-toolbar__spacer" />

    <!-- 发送 / 停止 -->
    <button
      v-if="!isGenerating"
      class="chat-toolbar__send"
      :disabled="!canSend"
      :title="t('main.chatToolbar.send')"
      @click="handleSend"
    >
      <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M12 19V5M5 12l7-7 7 7" />
      </svg>
    </button>
    <button
      v-else
      class="chat-toolbar__send chat-toolbar__send--stop"
      :title="t('main.chatToolbar.stop')"
      @click="$emit('stop')"
    >
      <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
        <rect x="6" y="6" width="12" height="12" rx="2" />
      </svg>
    </button>

    <!-- 点击外部关闭下拉 -->
    <div v-if="openMenu" class="chat-toolbar__backdrop" @click="closeMenus" />
  </div>
</template>

<style scoped>
.chat-toolbar {
  position: relative;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 10px;
  border-top: 1px solid var(--fluen-hairline);
  background: var(--fluen-surface);
  flex-shrink: 0;
  min-width: 0;
}

.chat-toolbar__spacer {
  flex: 1;
  min-width: 0;
}

.chat-toolbar__btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 28px;
  padding: 0 8px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-canvas);
  color: var(--fluen-slate);
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease, border-color 0.15s ease;
  white-space: nowrap;
  flex-shrink: 0;
}

.chat-toolbar__btn:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.chat-toolbar__btn--active {
  border-color: var(--fluen-brand-coral);
  color: var(--fluen-brand-coral);
}

.chat-toolbar__btn--pill {
  padding-right: 6px;
}

.chat-toolbar__btn-text {
  margin-left: 2px;
}

.chat-toolbar__label {
  max-width: 110px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.chat-toolbar__label--truncate {
  max-width: 90px;
}

.chat-toolbar__caret {
  flex-shrink: 0;
  opacity: 0.7;
}

.chat-toolbar__dropdown {
  position: relative;
  display: inline-flex;
  flex-shrink: 0;
}

.chat-toolbar__menu {
  position: absolute;
  bottom: calc(100% + 6px);
  left: 0;
  z-index: 20;
  min-width: 160px;
  max-width: 240px;
  max-height: 240px;
  overflow-y: auto;
  padding: 4px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 10px;
  background: var(--fluen-surface);
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.1);
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.chat-toolbar__menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 8px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
  transition: background 0.12s ease;
}

.chat-toolbar__menu-item:hover {
  background: var(--fluen-hover);
}

.chat-toolbar__menu-item--active {
  color: var(--fluen-brand-coral);
  background: color-mix(in srgb, var(--fluen-brand-coral) 8%, transparent);
}

.chat-toolbar__menu-title {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.chat-toolbar__menu-sub {
  flex-shrink: 0;
  color: var(--fluen-stone);
  font-size: 11px;
}

.chat-toolbar__menu-empty {
  padding: 8px;
  color: var(--fluen-stone);
  font-size: 12px;
  text-align: center;
}

.chat-toolbar__backdrop {
  position: fixed;
  inset: 0;
  z-index: 10;
}

.chat-toolbar__send {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  flex-shrink: 0;
  border: none;
  border-radius: 8px;
  background: var(--fluen-brand-coral);
  color: var(--fluen-on-dark);
  cursor: pointer;
  transition: opacity 0.15s ease, background 0.15s ease;
}

.chat-toolbar__send:hover:not(:disabled) {
  opacity: 0.85;
}

.chat-toolbar__send:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.chat-toolbar__send--stop {
  background: var(--fluen-surface);
  color: var(--fluen-slate);
  border: 1px solid var(--fluen-hairline);
}

.chat-toolbar__send--stop:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}
</style>
