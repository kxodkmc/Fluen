/**
 * useReferenceReader —— 文献阅读器核心状态管理。
 *
 * 职责：
 *   1. 加载文献内容（`reference_read_content`）
 *   2. 收集图片路径并批量解析为 data URL（`reference_resolve_assets`）
 *   3. 调用 MarkdownRenderer 渲染 HTML + BlockMap
 *   4. 管理加载/错误/主题选项状态
 *
 * 渲染流程：load MD → collectImageSrcs → resolve_assets → renderMarkdown
 *
 * @module composables/useReferenceReader
 */

import { reactive, readonly } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import {
  collectImageSrcs,
  renderMarkdown,
} from '../views/main/components/reader/MarkdownRenderer';
import {
  DEFAULT_READER_OPTIONS,
  type BlockMap,
  type ReaderContent,
  type ReaderFontSize,
  type ReaderLineHeight,
  type ReaderOptions,
  type ReaderThemeMode,
} from '../types/reader';
import { useProject } from './useProject';

// ---------------------------------------------------------------------------
// 状态
// ---------------------------------------------------------------------------

interface ReaderState {
  /** 是否正在加载。 */
  loading: boolean;
  /** 错误消息（null 表示无错误）。 */
  error: string | null;
  /** 当前加载的文献内容（MD + 元数据）。 */
  content: ReaderContent | null;
  /** 渲染后的 HTML 片段。 */
  html: string;
  /** 渲染生成的块映射表。 */
  blockMap: BlockMap | null;
  /** 阅读器选项。 */
  options: ReaderOptions;
}

// 模块级状态（单例，跨组件共享）
const _state = reactive<ReaderState>({
  loading: false,
  error: null,
  content: null,
  html: '',
  blockMap: null,
  options: { ...DEFAULT_READER_OPTIONS },
});

// ---------------------------------------------------------------------------
// composable
// ---------------------------------------------------------------------------

export function useReferenceReader() {
  const { currentProject } = useProject();

  /**
   * 加载并渲染文献。
   *
   * @param referenceId 文献 ID
   */
  async function loadReference(referenceId: string): Promise<void> {
    if (!currentProject.value) {
      _state.error = '未打开项目';
      return;
    }

    _state.loading = true;
    _state.error = null;
    _state.content = null;
    _state.html = '';
    _state.blockMap = null;

    try {
      // 1. 读取 MD 原文 + 元数据
      const content = await invoke<ReaderContent>('reference_read_content', {
        projectPath: currentProject.value.project_path,
        referenceId,
      });
      _state.content = content;

      // 2. 从 token 树收集图片路径（零正则，覆盖 reference-style 等）
      const imagePaths = collectImageSrcs(content.md);

      // 3. 批量解析图片资源为 data URL
      let assetMap = new Map<string, string>();
      if (imagePaths.length > 0) {
        const resolved = await invoke<Record<string, string>>(
          'reference_resolve_assets',
          {
            projectPath: currentProject.value.project_path,
            referenceId,
            paths: imagePaths,
          },
        );
        assetMap = new Map(Object.entries(resolved));
      }

      // 4. 渲染 MD → HTML + BlockMap
      const { html, blockMap } = renderMarkdown(content.md, assetMap);
      _state.html = html;
      _state.blockMap = blockMap;
    } catch (err) {
      _state.error = err instanceof Error ? err.message : String(err);
    } finally {
      _state.loading = false;
    }
  }

  /** 清空状态（关闭阅读器时调用）。 */
  function clear(): void {
    _state.loading = false;
    _state.error = null;
    _state.content = null;
    _state.html = '';
    _state.blockMap = null;
  }

  // ── 主题选项 ──

  function setThemeMode(mode: ReaderThemeMode): void {
    _state.options.themeMode = mode;
  }

  function setFontSize(size: ReaderFontSize): void {
    _state.options.fontSize = size;
  }

  function setLineHeight(lh: ReaderLineHeight): void {
    _state.options.lineHeight = lh;
  }

  return {
    state: readonly(_state),
    loadReference,
    clear,
    setThemeMode,
    setFontSize,
    setLineHeight,
  };
}
