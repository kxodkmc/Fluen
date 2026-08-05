/**
 * 知识库网状图模块导出。
 *
 * 模块结构：
 *   - `KnowledgeGraphView`：子窗口顶层视图
 *   - `openKnowledgeGraphWindow`：在独立 Tauri 子窗口中打开网状图
 */

export { default as KnowledgeGraphView } from './KnowledgeGraphView.vue';
export { openKnowledgeGraphWindow } from './composables/useKnowledgeGraphWindow';
