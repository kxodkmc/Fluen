<script setup lang="ts">
/**
 * ProviderSidebar — 模型设置的供应商侧栏（左栏）。
 *
 * 按预设 / 自定义分组展示已配置的供应商，底部提供「添加供应商」入口。
 * 纯展示组件，选中与添加事件由父级（LlmConfigSection）处理。
 */
import { computed } from 'vue';
import { useI18n } from '../../../../i18n';
import type { ProviderConfig } from '../../../../types/llm';
import { isPresetProvider, providerIcon, providerEnabled } from './shared';

const props = defineProps<{
  /** 已配置的提供商列表。 */
  providers: ProviderConfig[];
  /** 当前选中的提供商 ID。 */
  selectedId: string | null;
  /** 是否处于「添加供应商」模式。 */
  adding: boolean;
}>();

const emit = defineEmits<{
  select: [id: string];
  add: [];
}>();

const { t } = useI18n();

/** 预设供应商分组。 */
const presetProviders = computed(() => props.providers.filter(isPresetProvider));

/** 自定义供应商分组。 */
const customProviders = computed(() => props.providers.filter((p) => !isPresetProvider(p)));
</script>

<template>
  <aside class="sidebar">
    <div class="sidebar__scroll">
      <template v-if="providers.length > 0">
        <div v-if="presetProviders.length > 0" class="sidebar__group">
          <div class="sidebar__group-label">{{ t('settings.llmConfig.presetProviders') }}</div>
          <button
            v-for="provider in presetProviders"
            :key="provider.id"
            class="sidebar__item"
            :class="{ 'sidebar__item--selected': !adding && selectedId === provider.id }"
            type="button"
            @click="emit('select', provider.id)"
          >
            <svg
              v-if="providerIcon(provider)"
              class="sidebar__icon"
              viewBox="0 0 24 24"
              width="16"
              height="16"
              v-html="providerIcon(provider)"
            />
            <svg
              v-else
              class="sidebar__icon"
              viewBox="0 0 24 24"
              width="16"
              height="16"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" />
            </svg>
            <span class="sidebar__name">{{ provider.name }}</span>
            <span
              class="sidebar__dot"
              :class="{ 'sidebar__dot--on': providerEnabled(provider) }"
            />
          </button>
        </div>

        <div v-if="customProviders.length > 0" class="sidebar__group">
          <div class="sidebar__group-label">{{ t('settings.llmConfig.customProviders') }}</div>
          <button
            v-for="provider in customProviders"
            :key="provider.id"
            class="sidebar__item"
            :class="{ 'sidebar__item--selected': !adding && selectedId === provider.id }"
            type="button"
            @click="emit('select', provider.id)"
          >
            <svg
              class="sidebar__icon"
              viewBox="0 0 24 24"
              width="16"
              height="16"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" />
            </svg>
            <span class="sidebar__name">{{ provider.name }}</span>
            <span
              class="sidebar__dot"
              :class="{ 'sidebar__dot--on': providerEnabled(provider) }"
            />
          </button>
        </div>
      </template>

      <div v-else class="sidebar__empty">{{ t('settings.llmConfig.noProviders') }}</div>
    </div>

    <button
      class="sidebar__add"
      :class="{ 'sidebar__add--active': adding }"
      type="button"
      @click="emit('add')"
    >
      <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
        <path d="M12 5v14M5 12h14" />
      </svg>
      {{ t('settings.llmConfig.addProvider') }}
    </button>
  </aside>
</template>

<style scoped>
.sidebar {
  display: flex;
  flex-direction: column;
  width: 230px;
  flex-shrink: 0;
  border-right: 1px solid var(--fluen-hairline);
  min-height: 0;
}

.sidebar__scroll {
  flex: 1;
  overflow-y: auto;
  padding: 0.75rem;
}

.sidebar__group + .sidebar__group {
  margin-top: 1rem;
}

.sidebar__group-label {
  padding: 0 0.5rem 0.4rem;
  font-size: 0.72rem;
  font-weight: 500;
  color: var(--fluen-stone);
  letter-spacing: 0.03em;
}

.sidebar__item {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  width: 100%;
  padding: 8px 10px;
  border: 1px solid transparent;
  border-radius: 8px;
  background: transparent;
  color: var(--fluen-charcoal);
  font-family: var(--fluen-font-sans);
  font-size: 0.85rem;
  text-align: left;
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease;
}

.sidebar__item:hover {
  background: var(--fluen-hover);
}

.sidebar__item--selected {
  background: var(--fluen-surface);
  border-color: var(--fluen-hairline);
}

.sidebar__icon {
  flex-shrink: 0;
  color: var(--fluen-steel);
}

.sidebar__name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sidebar__dot {
  flex-shrink: 0;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--fluen-hairline);
}

.sidebar__dot--on {
  background: var(--fluen-success-text, #34c77b);
}

.sidebar__empty {
  padding: 1rem 0.5rem;
  font-size: 0.8rem;
  color: var(--fluen-stone);
}

.sidebar__add {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  margin: 0.5rem 0.75rem 0.75rem;
  padding: 8px 12px;
  border: 1px dashed var(--fluen-hairline);
  border-radius: 8px;
  background: transparent;
  color: var(--fluen-steel);
  font-family: var(--fluen-font-sans);
  font-size: 0.82rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
}

.sidebar__add:hover,
.sidebar__add--active {
  border-color: var(--fluen-accent);
  color: var(--fluen-accent);
}
</style>
