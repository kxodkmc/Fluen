<script setup lang="ts">
/**
 * LlmConfigSection — 模型设置分区。
 *
 * 采用双栏布局：左栏为供应商列表（预设 / 自定义分组 + 添加入口），
 * 右栏为选中供应商的配置详情或「添加供应商」表单。
 * 知识库（Embedding / 构建模型）配置仍保留在下方。
 *
 * 状态管理委托给 useLlmSettings composable，组件仅负责布局与选中逻辑。
 */
import { ref, computed, onMounted } from 'vue';
import { useI18n } from '../../../i18n';
import { useLlmSettings } from '../composables/useLlmSettings';
import type { ProviderConfig } from '../../../types/llm';
import ProviderSidebar from '../components/llm/ProviderSidebar.vue';
import ProviderDetailPanel from '../components/llm/ProviderDetailPanel.vue';
import ProviderAddPanel from '../components/llm/ProviderAddPanel.vue';
import KnowledgeBaseSection from './KnowledgeBaseSection.vue';

const { t } = useI18n();
const { providers, isLoading, load } = useLlmSettings();

onMounted(() => {
  load();
});

/* ── 选中状态 ───────────────────────────────────────────────────────── */

/** 选中的供应商 ID；null 表示右侧显示占位。 */
const selectedId = ref<string | null>(null);

/** 是否处于「添加供应商」模式。 */
const adding = ref(false);

/** 选中的供应商对象（从已配置列表解析；被删除后自动回退为 null）。 */
const selectedProvider = computed<ProviderConfig | null>(
  () => providers.value.find((p) => p.id === selectedId.value) ?? null,
);

/** 选中供应商（显示详情面板）。 */
function selectProvider(id: string): void {
  selectedId.value = id;
  adding.value = false;
}

/** 进入添加模式。 */
function startAdding(): void {
  adding.value = true;
  selectedId.value = null;
}

/** 取消添加。 */
function cancelAdding(): void {
  adding.value = false;
}

/** 添加完成：选中新供应商并退出添加模式。 */
function onAdded(id: string): void {
  adding.value = false;
  selectedId.value = id;
}

/** 刷新配置。 */
async function refresh(): Promise<void> {
  await load();
  adding.value = false;
  if (!selectedProvider.value) selectedId.value = null;
}
</script>

<template>
  <div class="section">
    <div class="section__header">
      <div>
        <h2 class="section__title">{{ t('settings.sections.llmConfig') }}</h2>
        <p class="section__desc">{{ t('settings.llmConfig.description') }}</p>
      </div>
      <button class="section__refresh" :title="t('settings.llmConfig.refresh')" @click="refresh">
        <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 12a9 9 0 1 1-2.64-6.36M21 3v6h-6" />
        </svg>
      </button>
    </div>

    <!-- 加载中 -->
    <div v-if="isLoading" class="section__loading">
      {{ t('settings.llmConfig.loading') }}
    </div>

    <!-- 双栏主体 -->
    <div v-else class="panel">
      <ProviderSidebar
        :providers="providers"
        :selected-id="selectedId"
        :adding="adding"
        @select="selectProvider"
        @add="startAdding"
      />

      <div class="panel__body">
        <!-- 添加供应商 -->
        <ProviderAddPanel v-if="adding" @cancel="cancelAdding" @added="onAdded" />

        <!-- 供应商详情 -->
        <ProviderDetailPanel
          v-else-if="selectedProvider"
          :key="selectedProvider.id"
          :provider="selectedProvider"
        />

        <!-- 占位 -->
        <div v-else class="panel__placeholder">
          <p class="panel__placeholder-text">{{ t('settings.llmConfig.selectProviderHint') }}</p>
        </div>
      </div>
    </div>

    <!-- ── 知识库配置 ───────────────────────────────────────────────── -->
    <KnowledgeBaseSection v-if="!isLoading && providers.length > 0" />
  </div>
</template>

<style scoped>
.section {
  padding: 0;
}

.section__header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
}

.section__title {
  margin: 0 0 0.25rem;
  font-family: var(--fluen-font-sans);
  font-size: 1.4rem;
  font-weight: 600;
  color: var(--fluen-ink);
  letter-spacing: -0.5px;
}

.section__desc {
  margin: 0 0 1.5rem;
  font-family: var(--fluen-font-sans);
  font-size: 0.85rem;
  color: var(--fluen-stone);
}

.section__refresh {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
  transition: all 0.15s ease;
}

.section__refresh:hover {
  border-color: var(--fluen-stone);
  color: var(--fluen-ink);
}

.section__loading {
  padding: 2rem 0;
  text-align: center;
  font-size: 0.85rem;
  color: var(--fluen-stone);
}

/* ── 双栏面板 ───────────────────────────────────────────────────────── */
.panel {
  display: flex;
  min-height: 480px;
  max-width: 860px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 14px;
  background: var(--fluen-surface);
  overflow: hidden;
}

.panel__body {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
}

.panel__placeholder {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  min-height: 400px;
}

.panel__placeholder-text {
  font-size: 0.85rem;
  color: var(--fluen-stone);
}

@media (max-width: 768px) {
  .panel {
    flex-direction: column;
  }

  .panel :deep(.sidebar) {
    width: 100%;
    border-right: none;
    border-bottom: 1px solid var(--fluen-hairline);
  }
}
</style>
