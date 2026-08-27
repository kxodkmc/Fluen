/**
 * 打开知识库网状图子窗口的 composable。
 *
 * 通过 Tauri `WebviewWindow` API 创建独立 OS 窗口，加载同一前端的
 * `index.html?view=graph&project=<encoded>`，由 App.vue 路由到
 * `KnowledgeGraphView`。
 *
 * 设计要点：
 *   - **单例窗口**：同一项目只允许打开一个网状图窗口（label 唯一），
 *     重复点击聚焦已存在的窗口
 *   - **URL 传参**：项目路径通过 query string 传递，新窗口通过
 *     `URLSearchParams` 读取，避免跨窗口状态共享
 *   - **窗口特性**：无边框（decorations: false），由页面内自定义标题栏
 *     提供拖拽区与最小化 / 最大化 / 关闭控件
 */

import { WebviewWindow } from '@tauri-apps/api/webviewWindow';

/** 窗口 label 前缀，配合 capabilities 中的 `graph*` 通配权限。 */
const WINDOW_LABEL_PREFIX = 'graph-';

/** 已打开窗口的 label → WebviewWindow 映射（模块级单例）。 */
const openedWindows = new Map<string, WebviewWindow>();

/**
 * 为指定项目打开知识库网状图子窗口。
 *
 * 若该项目已有网状图窗口，则聚焦该窗口而不重复创建。
 *
 * @param projectPath 项目根路径
 * @returns 创建或聚焦的窗口；失败时返回 null
 */
export async function openKnowledgeGraphWindow(
  projectPath: string,
): Promise<WebviewWindow | null> {
  if (!projectPath) return null;

  const label = `${WINDOW_LABEL_PREFIX}${hashPath(projectPath)}`;

  // 已存在则聚焦
  const existing = openedWindows.get(label);
  if (existing) {
    try {
      await existing.setFocus();
      await existing.show();
      return existing;
    } catch {
      // 窗口可能已关闭但映射未清理，继续走新建流程
      openedWindows.delete(label);
    }
  }

  // 构造 URL：相对路径，由 Tauri 解析到 devURL / dist 根
  const params = new URLSearchParams({
    view: 'graph',
    project: projectPath,
  });
  const url = `index.html?${params.toString()}`;

  try {
    const window = new WebviewWindow(label, {
      url,
      title: 'Fluen · 知识库网状图',
      width: 1200,
      height: 800,
      minWidth: 720,
      minHeight: 480,
      decorations: false,
      transparent: false,
      shadow: true,
      center: true,
    });

    openedWindows.set(label, window);

    // 窗口关闭时清理映射
    window.once('tauri://destroyed', () => {
      openedWindows.delete(label);
    });

    return window;
  } catch (err) {
    console.error('[openKnowledgeGraphWindow] 创建子窗口失败:', err);
    return null;
  }
}

/**
 * 将项目路径映射为短哈希，用作窗口 label 后缀。
 *
 * 使用简单 FNV-1a 变体，足以避免不同项目冲突；不需要密码学强度。
 */
function hashPath(path: string): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < path.length; i++) {
    hash ^= path.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  // 转为无符号 32 位十六进制
  return (hash >>> 0).toString(16);
}
