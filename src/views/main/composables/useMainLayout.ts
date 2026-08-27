/**
 * 主界面布局状态管理 composable。
 *
 * 集中管理三段布局的面板可见性、尺寸、活动栏选中项、
 * 以及内容区标签页的打开/关闭/激活。
 *
 * 组件树中只需调用一次，通过 provide/inject 或直接引用共享状态。
 *
 * @example
 * ```ts
 * const layout = useMainLayout();
 * layout.toggleAIPanel();
 * layout.openTab({ id: '1', title: 'doc.md', type: 'file' });
 * ```
 */

import { ref, readonly, computed } from 'vue';
import type { InjectionKey } from 'vue';
import type { ContentTab, EditorLayoutMode, PanelId, RightPanelId } from '../types';
import { DEFAULT_PANEL_SIZES } from '../constants';

export function useMainLayout() {
  /* ── 面板可见性 ──────────────────────────────────────────────────────── */
  const functionPanelCollapsed = ref(false);
  /** 右侧面板可见性（原 aiPanelVisible，演进为通用右侧面板开关）。 */
  const rightPanelVisible = ref(true);

  /* ── 右侧面板切换 ───────────────────────────────────────────────────── */
  /** 当前激活的右侧面板（motis 对话 / 学术助手），null 表示未选中任何面板。 */
  const activeRightPanel = ref<RightPanelId | null>('motis');

  /* ── 面板尺寸 ───────────────────────────────────────────────────────── */
  const functionPanelWidth = ref<number>(DEFAULT_PANEL_SIZES.functionPanel);
  const aiPanelWidth = ref<number>(DEFAULT_PANEL_SIZES.aiPanel);

  /* ── 编辑器视图模式 ─────────────────────────────────────────────────── */
  /** 默认进入预览视图（仅渲染 HTML），源码/半预览可通过切换控件或 Mod+1/2 进入。 */
  const editorLayout = ref<EditorLayoutMode>('preview');

  /* ── 活动栏 ─────────────────────────────────────────────────────────── */
  const activeActivity = ref<string>('outline');

  /* ── 内容区标签页 ───────────────────────────────────────────────────── */
  const tabs = ref<ContentTab[]>([]);
  const activeTabId = ref<string | null>(null);

  /* ── 兼容别名 ───────────────────────────────────────────────────────── */
  /** aiPanelVisible 别名（兼容旧引用，指向 rightPanelVisible）。 */
  const aiPanelVisible = computed(() => rightPanelVisible.value);

  /* ── 面板切换 ───────────────────────────────────────────────────────── */

  /** 切换指定面板状态。function 面板在折叠/展开之间切换，ai 面板显示/隐藏。 */
  function togglePanel(panel: PanelId): void {
    if (panel === 'function') {
      toggleFunctionPanelCollapsed();
    } else if (panel === 'ai') {
      toggleRightPanel();
    }
  }

  /** 切换功能区折叠状态。 */
  function toggleFunctionPanelCollapsed(): void {
    functionPanelCollapsed.value = !functionPanelCollapsed.value;
  }

  /** 设置功能区是否折叠。 */
  function setFunctionPanelCollapsed(collapsed: boolean): void {
    functionPanelCollapsed.value = collapsed;
  }

  /** 切换右侧面板展开/收起。 */
  function toggleRightPanel(): void {
    rightPanelVisible.value = !rightPanelVisible.value;
  }

  /**
   * 幂等地激活并展开右侧面板（程序化同步用，永不收起）。
   * 已激活且可见时为无操作；已激活但被收起时重新展开。
   */
  function showRightPanel(panel: RightPanelId): void {
    if (activeRightPanel.value === panel && rightPanelVisible.value) return;
    activeRightPanel.value = panel;
    rightPanelVisible.value = true;
  }

  /**
   * 设置激活的右侧面板并展开面板。
   * 若点击已激活的面板，则切换收起/展开。
   */
  function setActiveRightPanel(panel: RightPanelId): void {
    if (activeRightPanel.value === panel && rightPanelVisible.value) {
      // 已激活且可见 → 收起
      rightPanelVisible.value = false;
    } else {
      showRightPanel(panel);
    }
  }

  /** 切换 AI 区可见性（兼容别名，等价于 toggleRightPanel）。 */
  function toggleAIPanel(): void {
    toggleRightPanel();
  }

  /* ── 活动栏 ─────────────────────────────────────────────────────────── */

  /** 切换编辑器视图模式（source 仅源码 / live 半预览 / preview 仅渲染）。 */
  function setEditorLayout(mode: EditorLayoutMode): void {
    editorLayout.value = mode;
  }

  /** 设置当前激活的活动栏项。 */
  function setActiveActivity(id: string): void {
    // 点击已激活项时折叠面板；点击新项时确保面板展开
    if (activeActivity.value === id) {
      toggleFunctionPanelCollapsed();
    } else {
      activeActivity.value = id;
      if (functionPanelCollapsed.value) {
        functionPanelCollapsed.value = false;
      }
    }
  }

  /* ── 标签页管理 ─────────────────────────────────────────────────────── */

  /** 打开一个标签页（若已存在则激活它）。 */
  function openTab(tab: ContentTab): void {
    const existing = tabs.value.find((t) => t.id === tab.id);
    if (existing) {
      activeTabId.value = existing.id;
      return;
    }
    tabs.value.push(tab);
    activeTabId.value = tab.id;
  }

  /** 关闭指定标签页。 */
  function closeTab(id: string): void {
    const idx = tabs.value.findIndex((t) => t.id === id);
    if (idx === -1) return;

    tabs.value.splice(idx, 1);

    // 若关闭的是当前激活的标签页，切换到相邻标签
    if (activeTabId.value === id) {
      if (tabs.value.length === 0) {
        activeTabId.value = null;
      } else {
        const nextIdx = Math.min(idx, tabs.value.length - 1);
        activeTabId.value = tabs.value[nextIdx].id;
      }
    }
  }

  /** 激活指定标签页。 */
  function setActiveTab(id: string): void {
    activeTabId.value = id;
  }

  /* ── 面板尺寸 ───────────────────────────────────────────────────────── */

  /** 更新功能区宽度。 */
  function setFunctionPanelWidth(width: number): void {
    functionPanelWidth.value = width;
  }

  /** 更新 AI 区宽度。 */
  function setAIPanelWidth(width: number): void {
    aiPanelWidth.value = width;
  }

  return {
    // 状态（只读）
    functionPanelCollapsed: readonly(functionPanelCollapsed),
    rightPanelVisible: readonly(rightPanelVisible),
    activeRightPanel: readonly(activeRightPanel),
    aiPanelVisible,
    functionPanelWidth: readonly(functionPanelWidth),
    aiPanelWidth: readonly(aiPanelWidth),
    activeActivity: readonly(activeActivity),
    editorLayout: readonly(editorLayout),
    tabs: readonly(tabs),
    activeTabId: readonly(activeTabId),

    // 面板操作
    setEditorLayout,
    togglePanel,
    toggleFunctionPanelCollapsed,
    setFunctionPanelCollapsed,
    toggleRightPanel,
    showRightPanel,
    setActiveRightPanel,
    toggleAIPanel,

    // 活动栏
    setActiveActivity,

    // 标签页
    openTab,
    closeTab,
    setActiveTab,

    // 尺寸
    setFunctionPanelWidth,
    setAIPanelWidth,
  };
}

/** useMainLayout 返回值类型（供 provide/inject 推导）。 */
export type UseMainLayoutReturn = ReturnType<typeof useMainLayout>;

/**
 * useMainLayout 实例注入键。
 *
 * 由 MainView 创建并通过 provide 注入，供 TitleBar、TitleBarSearch 等
 * 子组件共享同一份布局状态（与 MOTIS_CHAT_KEY 模式一致）。
 */
export const MAIN_LAYOUT_KEY: InjectionKey<UseMainLayoutReturn> = Symbol('main-layout');
