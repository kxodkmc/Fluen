/**
 * “关于”对话框的版本信息收集。
 *
 * 应用版本与依赖库版本以 package.json 为唯一维护点：
 *   - 前端：构建期由 vite 注入 `__APP_VERSION__` / `__APP_LIB_VERSIONS__`（见 vite.config.ts）
 *   - Tauri：运行时通过 `@tauri-apps/api/app` 读取（应用版本以 tauri.conf.json 为准）
 * 浏览器/vitest 等无 Tauri 环境下接口会失败，统一降级为构建期常量或空串。
 */
import { getVersion, getTauriVersion } from '@tauri-apps/api/app';

/** 单条版本信息（库名 + 版本号）。 */
export interface VersionEntry {
  /** 展示名称。 */
  name: string;
  /** 版本号（已去除 semver 范围前缀）。 */
  version: string;
}

/** “关于”页展示的核心前端依赖（package.json 包名 → 展示名）。 */
const ABOUT_LIBRARIES: ReadonlyArray<readonly [string, string]> = [
  ['vue', 'Vue'],
  ['vue-i18n', 'Vue I18n'],
  ['@tauri-apps/api', 'Tauri JS API'],
  ['@tauri-apps/plugin-dialog', 'Dialog Plugin'],
  ['@tauri-apps/plugin-opener', 'Opener Plugin'],
  ['@codemirror/state', 'CodeMirror'],
  ['@lezer/markdown', 'Lezer Markdown'],
  ['markdown-it', 'markdown-it'],
  ['katex', 'KaTeX'],
  ['highlight.js', 'highlight.js'],
];

/** 去除 package.json 中 semver 范围的 `^` / `~` 等前缀。 */
function stripRange(raw: string): string {
  return raw.replace(/^[\^~><=\s]+/, '').trim();
}

/** 构建期注入的依赖版本映射（无注入环境返回空对象）。 */
function injectedLibVersions(): Record<string, string> {
  return typeof __APP_LIB_VERSIONS__ === 'undefined' ? {} : __APP_LIB_VERSIONS__;
}

/** 应用版本号：优先运行时（tauri.conf.json），降级为构建期常量。 */
export async function getAppVersion(): Promise<string> {
  try {
    return await getVersion();
  } catch {
    return typeof __APP_VERSION__ === 'undefined' ? '' : __APP_VERSION__;
  }
}

/** Tauri 运行时（Rust core crate）版本号；非 Tauri 环境返回空串。 */
export async function getTauriRuntimeVersion(): Promise<string> {
  try {
    return await getTauriVersion();
  } catch {
    return '';
  }
}

/** 核心前端依赖库的版本列表（package.json 中缺失的包自动跳过）。 */
export function getLibraryVersions(): VersionEntry[] {
  const deps = injectedLibVersions();
  return ABOUT_LIBRARIES.map(([pkgName, displayName]) => ({
    name: displayName,
    version: stripRange(deps[pkgName] ?? ''),
  })).filter((entry) => entry.version !== '');
}
