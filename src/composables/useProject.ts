/**
 * 项目状态管理 composable（单例模式）。
 *
 * 响应式状态声明在模块级别，确保所有调用 `useProject()`
 * 的组件共享同一份状态——TitleBar、FunctionPanel、ContentPanel
 * 中的项目状态始终同步。
 *
 * @example
 * ```ts
 * const { currentProject, openProject, hasProject, createSection } = useProject();
 * if (!hasProject.value) {
 *   await openProject('/path/to/project');
 * }
 * await createSection('引言');
 * ```
 */

import { ref, computed, readonly } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type {
  CreateSectionRequest,
  InsertHeadingRequest,
  OpenProjectResult,
  ProjectErrorResponse,
  RenameHeadingRequest,
  SaveTempMdRequest,
} from '../types/project';
import type { RecentProjectEntry } from '../types/recentProjects';
import { useRecentProjects } from './useRecentProjects';

// ── 模块级状态（单例） ──────────────────────────────────────────────

const _currentProject = ref<OpenProjectResult | null>(null);
const _isLoading = ref(false);
const _isSaving = ref(false);
const _error = ref<ProjectErrorResponse | null>(null);

// ── composable ─────────────────────────────────────────────────────

export function useProject() {
  /** 最近打开项目列表 composable（单例）。 */
  const { recordOpen, removeEntry } = useRecentProjects();

  /** 打开项目。成功返回 true，失败返回 false 并设置 error。 */
  async function openProject(path: string): Promise<boolean> {
    _isLoading.value = true;
    _error.value = null;
    try {
      _currentProject.value = await invoke<OpenProjectResult>('open_project', {
        projectPath: path,
      });
      // 记录到最近打开项目列表（异步，不阻塞打开流程）
      const cfg = _currentProject.value.config;
      const entry: RecentProjectEntry = {
        project_path: path,
        title: cfg.title,
        author: cfg.author,
        opened_at: new Date().toISOString(),
      };
      void recordOpen(entry);
      return true;
    } catch (err) {
      _error.value = err as ProjectErrorResponse;
      _currentProject.value = null;
      // 项目路径可能失效，从最近列表移除
      void removeEntry(path);
      return false;
    } finally {
      _isLoading.value = false;
    }
  }

  /** 关闭项目，清空状态。 */
  function closeProject(): void {
    _currentProject.value = null;
    _error.value = null;
  }

  /**
   * 创建新章节（一级标题）。
   *
   * 后端生成 `sec-{UUID4}.md` 文件并更新 `sections.json`，
   * 返回更新后的 `OpenProjectResult`。
   */
  async function createSection(title: string): Promise<boolean> {
    if (_isSaving.value || !_currentProject.value) return false;
    _isSaving.value = true;
    _error.value = null;
    try {
      const request: CreateSectionRequest = {
        project_path: _currentProject.value.project_path,
        title,
      };
      _currentProject.value = await invoke<OpenProjectResult>('create_section', { request });
      return true;
    } catch (err) {
      _error.value = err as ProjectErrorResponse;
      return false;
    } finally {
      _isSaving.value = false;
    }
  }

  /**
   * 重命名标题（任意层级）。
   *
   * 后端通过 `section_id` + `level` + `old_text` 文本匹配定位标题行，
   * 不依赖行号。
   */
  async function renameHeading(
    sectionId: string,
    level: number,
    oldText: string,
    newText: string,
  ): Promise<boolean> {
    if (_isSaving.value || !_currentProject.value) return false;
    _isSaving.value = true;
    _error.value = null;
    try {
      const request: RenameHeadingRequest = {
        project_path: _currentProject.value.project_path,
        section_id: sectionId,
        level,
        old_text: oldText,
        new_text: newText,
      };
      _currentProject.value = await invoke<OpenProjectResult>('rename_heading', { request });
      return true;
    } catch (err) {
      _error.value = err as ProjectErrorResponse;
      return false;
    } finally {
      _isSaving.value = false;
    }
  }

  /**
   * 插入子标题。
   *
   * 在锚点标题（`anchorLevel` + `anchorText`）的作用域末尾插入新标题。
   * 锚点作用域 = 锚点标题行之后、下一个同级或更浅标题之前的区域。
   */
  async function insertHeading(
    sectionId: string,
    anchorLevel: number,
    anchorText: string,
    newLevel: number,
    newText: string,
  ): Promise<boolean> {
    if (_isSaving.value || !_currentProject.value) return false;
    _isSaving.value = true;
    _error.value = null;
    try {
      const request: InsertHeadingRequest = {
        project_path: _currentProject.value.project_path,
        section_id: sectionId,
        anchor_level: anchorLevel,
        anchor_text: anchorText,
        new_level: newLevel,
        new_text: newText,
      };
      _currentProject.value = await invoke<OpenProjectResult>('insert_heading', { request });
      return true;
    } catch (err) {
      _error.value = err as ProjectErrorResponse;
      return false;
    } finally {
      _isSaving.value = false;
    }
  }

  /**
   * 保存 `.temp.md` 内容。
   *
   * 后端将编辑后的 `.temp.md` 全文拆分回各 `sec-{id}.md` 源文件。
   * **安全校验**：拆分后的块数量必须与 `sections.json` 条目数一致。
   */
  async function saveTempMd(content: string): Promise<boolean> {
    if (_isSaving.value || !_currentProject.value) return false;
    _isSaving.value = true;
    _error.value = null;
    try {
      const request: SaveTempMdRequest = {
        project_path: _currentProject.value.project_path,
        content,
      };
      _currentProject.value = await invoke<OpenProjectResult>('save_temp_md', { request });
      return true;
    } catch (err) {
      _error.value = err as ProjectErrorResponse;
      return false;
    } finally {
      _isSaving.value = false;
    }
  }

  /**
   * 保存编辑器内容（原子保存）。
   *
   * 新的原子保存命令，替代 `editor_replace_text` + `editor_save` 的两步流程：
   * 后端一次性接收完整内容并持久化，返回更新后的 `OpenProjectResult`。
   */
  async function saveContent(content: string): Promise<boolean> {
    if (!_currentProject.value) {
      return false;
    }
    _isSaving.value = true;
    _error.value = null;
    try {
      _currentProject.value = await invoke<OpenProjectResult>('editor_save_content', {
        projectPath: _currentProject.value.project_path,
        content,
      });
      return true;
    } catch (err) {
      _error.value = err as ProjectErrorResponse;
      return false;
    } finally {
      _isSaving.value = false;
    }
  }

  return {
    // 状态（只读）
    currentProject: readonly(_currentProject),
    isLoading: readonly(_isLoading),
    isSaving: readonly(_isSaving),
    error: readonly(_error),

    // 计算属性
    hasProject: computed(() => _currentProject.value !== null),
    config: computed(() => _currentProject.value?.config ?? null),
    sections: computed(() => _currentProject.value?.sections ?? []),
    tempMd: computed(() => _currentProject.value?.temp_md ?? ''),
    warnings: computed(() => _currentProject.value?.warnings ?? []),

    // 操作
    openProject,
    closeProject,
    createSection,
    renameHeading,
    insertHeading,
    saveTempMd,
    saveContent,
  };
}
