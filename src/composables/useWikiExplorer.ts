/**
 * 知识库浏览与检索 composable。
 *
 * 封装 Tauri `knowledge_*` 只读命令，提供：
 *   - 条目列表加载（`knowledge_list_entries`）
 *   - 条目详情获取（`knowledge_get_entry`，含正文/标签名/关联标题）
 *   - 关键词/语义/混合检索（`knowledge_query`）
 *   - 元信息查询（`knowledge_meta`：overview / tags / recent）
 *
 * 设计要点：
 *   - **模块级单例状态**：`entries` / `meta` 跨组件共享，
 *     KnowledgeBasePanel 卸载重建后列表不丢失。
 *   - **无状态查询**：`loadEntry` / `search` 不写入模块级状态，
 *     由调用方（WikiReader 等）自行管理局部状态，避免多实例冲突。
 *   - 非错误：知识库未初始化时 `loadEntries` 返回空数组并标记 `notInitialized`。
 *
 * @example
 * ```ts
 * const { entries, loading, loadEntries, loadEntry, search } = useWikiExplorer();
 *
 * // 加载条目列表
 * await loadEntries(projectPath);
 *
 * // 获取详情（WikiReader 内部）
 * const detail = await loadEntry(projectPath, wikiId);
 *
 * // 搜索
 * const result = await search(projectPath, '注意力机制', { method: 'hybrid' });
 * ```
 */

import { ref, readonly } from 'vue';
import { invoke } from '@tauri-apps/api/core';

import type {
  MetaData,
  MetaQueryType,
  MetaResult,
  QueryResult,
  RetrievalMethod,
  WikiEntry,
  WikiEntryDetail,
} from '../types/knowledgeBase';

/**
 * 检测当前是否运行在 Tauri 环境中。
 */
function isTauriEnvironment(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

// ---------------------------------------------------------------------------
// 模块级单例状态（跨组件共享）
// ---------------------------------------------------------------------------

/** 知识库条目列表（不含正文）。 */
const entries = ref<WikiEntry[]>([]);

/** 是否正在加载列表。 */
const loading = ref(false);

/** 最近一次错误信息（空字符串表示无错误）。 */
const error = ref<string>('');

/** 知识库是否未初始化（loadEntries 时检测）。 */
const notInitialized = ref(false);

/** 元信息缓存（overview 查询结果）。 */
const meta = ref<MetaData | null>(null);

// ---------------------------------------------------------------------------
// Composable
// ---------------------------------------------------------------------------

export function useWikiExplorer() {
  /* ── 命令封装 ───────────────────────────────────────────────────────── */

  /**
   * 加载项目知识库的所有条目（不含正文）。
   *
   * 知识库未初始化时静默返回空数组，并设置 `notInitialized=true`。
   * 其他错误设置 `error` 并返回空数组。
   *
   * @param projectPath 项目根路径
   * @returns 条目数组（失败时为空）
   */
  async function loadEntries(projectPath: string): Promise<WikiEntry[]> {
    if (!isTauriEnvironment()) {
      entries.value = [];
      return [];
    }
    loading.value = true;
    error.value = '';
    notInitialized.value = false;
    try {
      const list = await invoke<WikiEntry[]>('knowledge_list_entries', {
        projectPath,
      });
      entries.value = list;
      return list;
    } catch (err) {
      const msg = typeof err === 'string' ? err : String(err);
      // 知识库未初始化时后端返回特定错误，宽松匹配
      if (msg.includes('未初始化') || msg.includes('not initialized') || msg.includes('index.db')) {
        notInitialized.value = true;
        entries.value = [];
      } else {
        error.value = msg;
        console.error('[useWikiExplorer] 加载知识库条目失败:', err);
      }
      return [];
    } finally {
      loading.value = false;
    }
  }

  /**
   * 获取指定条目的详情（含正文、标签名、关联条目标题）。
   *
   * 无状态函数：不写入模块级状态，由调用方管理局部状态。
   *
   * @param projectPath 项目根路径
   * @param wikiId 条目 ID
   * @returns 条目详情；不存在时返回 null
   */
  async function loadEntry(
    projectPath: string,
    wikiId: string,
  ): Promise<WikiEntryDetail | null> {
    if (!isTauriEnvironment()) return null;
    try {
      return await invoke<WikiEntryDetail | null>('knowledge_get_entry', {
        projectPath,
        wikiId,
      });
    } catch (err) {
      const msg = typeof err === 'string' ? err : String(err);
      console.error('[useWikiExplorer] 加载条目详情失败:', err);
      throw new Error(msg);
    }
  }

  /**
   * 检索知识库条目。
   *
   * @param projectPath 项目根路径
   * @param query 检索文本（标题关键词、ID 或语义描述）
   * @param options 检索选项
   * @returns 检索结果（含匹配项与使用的检索方式）
   */
  async function search(
    projectPath: string,
    query: string,
    options?: {
      /** 限定条目类型。 */
      wikiType?: WikiEntry['wiki_type'];
      /** 检索方式（默认 hybrid）。 */
      method?: RetrievalMethod;
      /** 返回结果数上限（默认 10）。 */
      topK?: number;
    },
  ): Promise<QueryResult> {
    if (!isTauriEnvironment()) {
      return { success: false, retrieval_method_used: 'keyword', results: [] };
    }
    try {
      return await invoke<QueryResult>('knowledge_query', {
        projectPath,
        query,
        wikiType: options?.wikiType ?? null,
        method: options?.method ?? null,
        topK: options?.topK ?? null,
      });
    } catch (err) {
      console.error('[useWikiExplorer] 检索失败:', err);
      return { success: false, retrieval_method_used: 'keyword', results: [] };
    }
  }

  /**
   * 查询知识库元信息。
   *
   * @param projectPath 项目根路径
   * @param queryType 查询类型（overview / tags / recent）
   * @param limit 结果数上限（仅 tags / recent 有效）
   * @returns 元信息结果
   */
  async function loadMeta(
    projectPath: string,
    queryType: MetaQueryType,
    limit?: number,
  ): Promise<MetaResult | null> {
    if (!isTauriEnvironment()) return null;
    try {
      const result = await invoke<MetaResult>('knowledge_meta', {
        projectPath,
        queryType,
        limit: limit ?? null,
      });
      // overview 结果缓存到模块级状态
      if (queryType === 'overview' && result.success) {
        meta.value = result.data;
      }
      return result;
    } catch (err) {
      console.error('[useWikiExplorer] 加载元信息失败:', err);
      return null;
    }
  }

  /** 清空列表状态（项目切换时调用）。 */
  function reset(): void {
    entries.value = [];
    error.value = '';
    notInitialized.value = false;
    meta.value = null;
  }

  return {
    // 响应式状态（只读）
    entries: readonly(entries),
    loading: readonly(loading),
    error: readonly(error),
    notInitialized: readonly(notInitialized),
    meta: readonly(meta),

    // 命令
    loadEntries,
    loadEntry,
    search,
    loadMeta,
    reset,
  };
}
