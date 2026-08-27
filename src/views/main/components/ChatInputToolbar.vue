<script setup lang="ts">
/**
 * ChatInputToolbar — 聊天输入区底部工具栏。
 *
 * 从左到右依次提供：添加文件、模式切换、模型切换、思考强度切换、发送/停止。
 * 仅负责展示与事件发射，状态由父组件或 useChatToolbar 管理。
 *
 * 响应式策略（完整排布约需 450px，按可用宽度阶梯降级）：
 *   - ≥420px：全部控件带文字
 *   - <420px：「添加文件」退化为纯图标（tooltip 提示）
 *   - <350px：隐藏思考强度下拉（低频操作）
 *   - <270px：「添加文件」与「思考强度」均隐藏，模型标签自由截断，
 *             保证发送键永不溢出；药丸按钮标签同时支持 flex 收缩截断兜底
 */
import { ref, computed, watch, nextTick, onMounted, onUnmounted } from 'vue';
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

/* ── 响应式断点（完整排布约需 450px） ───────────────────────────────── */
const toolbarWidth = ref(0);
/** 窄面板：「添加文件」退化为纯图标。 */
const isCompact = computed(() => toolbarWidth.value < 420);
/** 更窄：隐藏低频的思考强度下拉。 */
const isNarrow = computed(() => toolbarWidth.value < 350);
/** 超窄：只保留模式/模型/发送，标签自由截断。 */
const isUltraCompact = computed(() => toolbarWidth.value < 270);

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

/* ── 模型菜单：搜索 + 按提供商分组 ───────────────────────────────────── */
const modelSearch = ref('');
const modelSearchRef = ref<HTMLInputElement | null>(null);

const groupedModels = computed<{ provider: string; items: ToolbarModelOption[] }[]>(() => {
  const kw = modelSearch.value.trim().toLowerCase();
  const groups: { provider: string; items: ToolbarModelOption[] }[] = [];
  for (const opt of props.models) {
    if (
      kw &&
      !opt.name.toLowerCase().includes(kw) &&
      !opt.providerName.toLowerCase().includes(kw)
    ) {
      continue;
    }
    let group = groups.find((g) => g.provider === opt.providerName);
    if (!group) {
      group = { provider: opt.providerName, items: [] };
      groups.push(group);
    }
    group.items.push(opt);
  }
  return groups;
});

// 打开模型菜单时重置搜索词并聚焦输入框。
watch(openMenu, (menu) => {
  if (menu === 'model') {
    modelSearch.value = '';
    nextTick(() => modelSearchRef.value?.focus());
  }
});

function handleSend(): void {
  if (!props.canSend || props.isGenerating) return;
  emit('send');
}

/* ── 文案映射 ───────────────────────────────────────────────────────── */
const modeLabel = computed(() =>
  props.mode === 'motis' ? t('main.chatToolbar.modeMotis') : t('main.chatToolbar.modeAssistant'),
);

/** 当前模式对应的图标路径（超窄面板下按钮退化为纯图标时使用）。 */
const currentModeIcon = computed(
  () => modeOptions.find((opt) => opt.value === props.mode)?.icon ?? '',
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
    <!-- 添加文件（≥420px 显示文字，<270px 整体隐藏，为模型切换器让位） -->
    <button
      v-if="!isUltraCompact"
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
        :title="isUltraCompact ? modeLabel : undefined"
        @click="toggleMenu('mode')"
      >
        <svg
          v-if="isUltraCompact && currentModeIcon"
          viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"
        >
          <path :d="currentModeIcon" />
        </svg>
        <span v-else class="chat-toolbar__label">{{ modeLabel }}</span>
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

    <!-- 模型切换（任何宽度都保留，窄面板下仅截断标签） -->
    <div class="chat-toolbar__dropdown">
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
      <div v-if="openMenu === 'model'" class="chat-toolbar__menu chat-toolbar__menu--end chat-toolbar__menu--model" @click.stop>
        <div class="chat-toolbar__search">
          <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            <circle cx="11" cy="11" r="7" />
            <path d="M21 21l-4.35-4.35" />
          </svg>
          <input
            ref="modelSearchRef"
            v-model="modelSearch"
            class="chat-toolbar__search-input"
            type="text"
            :placeholder="t('main.chatToolbar.searchModel')"
          />
        </div>
        <div class="chat-toolbar__menu-scroll">
          <template v-for="group in groupedModels" :key="group.provider">
            <div class="chat-toolbar__menu-group">{{ group.provider }}</div>
            <button
              v-for="opt in group.items"
              :key="opt.id"
              class="chat-toolbar__menu-item"
              :class="{ 'chat-toolbar__menu-item--active': modelLabel === opt.name }"
              @click="selectModel(opt)"
            >
              {{ opt.name }}
            </button>
          </template>
          <div v-if="groupedModels.length === 0" class="chat-toolbar__menu-empty">
            {{ t('main.chatToolbar.noModels') }}
          </div>
        </div>
      </div>
    </div>

    <!-- 思考强度（<350px 隐藏，属低频操作） -->
    <div v-if="!isNarrow" class="chat-toolbar__dropdown">
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
      <div v-if="openMenu === 'thinking'" class="chat-toolbar__menu chat-toolbar__menu--end" @click.stop>
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
  /* 尺寸容器：供下拉菜单以 cqw 兜底宽度，确保任何窄面板下都不越界 */
  container-type: inline-size;
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

/* 药丸按钮允许收缩，标签随可用宽度截断（断点降级之外的兜底缓冲） */
.chat-toolbar__btn--pill {
  padding-right: 6px;
  min-width: 0;
  flex-shrink: 1;
}

.chat-toolbar__btn-text {
  margin-left: 2px;
}

.chat-toolbar__label {
  min-width: 0;
  flex: 0 1 auto;
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
  min-width: 0;
}

.chat-toolbar__menu {
  position: absolute;
  bottom: calc(100% + 6px);
  left: 0;
  z-index: 20;
  min-width: 160px;
  max-width: calc(100cqw - 16px);
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

/* 右锚定：模型/思考强度按钮位于工具栏偏右侧，菜单向左展开避免被面板右缘裁切 */
.chat-toolbar__menu--end {
  left: auto;
  right: 0;
}

.chat-toolbar__menu--model {
  min-width: min(220px, calc(100cqw - 16px));
  max-width: calc(100cqw - 16px);
  padding: 6px;
  gap: 4px;
  max-height: 320px;
  overflow: hidden;
}

.chat-toolbar__search {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 8px;
  height: 28px;
  border-radius: 6px;
  background: var(--fluen-canvas);
  color: var(--fluen-stone);
  flex-shrink: 0;
}

.chat-toolbar__search-input {
  flex: 1;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 12px;
}

.chat-toolbar__search-input::placeholder {
  color: var(--fluen-stone);
}

.chat-toolbar__menu-scroll {
  display: flex;
  flex-direction: column;
  gap: 2px;
  overflow-y: auto;
  min-height: 0;
  max-height: 210px;
}

.chat-toolbar__menu-group {
  padding: 6px 8px 2px;
  color: var(--fluen-stone);
  font-size: 11px;
  font-weight: 500;
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
