/**
 * 统一前端日志器。
 *
 * 将前端日志批量桥接到后端 `log_frontend` 命令，与后端日志写入同一文件
 * （`{cache}/logs/fluen.log.YYYY-MM-DD`），实现全应用单一日志视图。
 *
 * 设计要点：
 * - **缓冲批量上报**：环形缓冲 500 条，每 500ms 或满 50 条 flush 一次，减少 IPC 开销。
 * - **静默回退**：IPC 失败时回退到 `console.*`，绝不抛错打断业务。
 * - **Dev 镜像**：开发模式或非 Tauri 环境同时输出到 `console.*`，保留 DevTools 习惯。
 *
 * @example
 * ```ts
 * import { logger } from '../utils/logger';
 * logger.info('editor', '保存成功', { path });
 * logger.error('kb', '构建失败', err);
 * ```
 *
 * 或通过 composable 绑定作用域：
 * ```ts
 * const log = useLogger('editor');
 * log.info('保存成功');
 * ```
 */

import { invoke } from '@tauri-apps/api/core';

export type LogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error';

export interface LogEntry {
  level: LogLevel;
  scope: string;
  message: string;
  context?: string;
  timestamp: string;
}

const FLUSH_INTERVAL_MS = 500;
const FLUSH_THRESHOLD = 50;
const BUFFER_LIMIT = 500;

let buffer: LogEntry[] = [];
let flushTimer: ReturnType<typeof setInterval> | null = null;

/** 是否运行在 Tauri 环境（v2 注入 `window.__TAURI_INTERNALS__`）。 */
const inTauri = detectTauri();
const isDev = import.meta.env.DEV;

function detectTauri(): boolean {
  try {
    return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
  } catch {
    return false;
  }
}

function nowIso(): string {
  return new Date().toISOString();
}

function safeStringify(v: unknown): string {
  if (typeof v === 'string') return v;
  try {
    return JSON.stringify(v);
  } catch {
    return String(v);
  }
}

function toConsole(e: LogEntry): void {
  const label = `[${e.scope}]`;
  const args = e.context ? [label, e.message, e.context] : [label, e.message];
  switch (e.level) {
    case 'trace':
    case 'debug':
      console.debug(...args);
      break;
    case 'info':
      console.info(...args);
      break;
    case 'warn':
      console.warn(...args);
      break;
    case 'error':
      console.error(...args);
      break;
  }
}

/** 批量 flush 缓冲区到后端。失败时静默回退到 console。 */
async function flush(): Promise<void> {
  if (buffer.length === 0) return;
  const entries = buffer.splice(0, buffer.length);
  if (!inTauri) return; // 非 Tauri 环境仅走 console（已在 log() 中镜像）
  try {
    await invoke('log_frontend', { entries });
  } catch {
    // IPC 失败，回退到 console，保证日志不丢
    for (const e of entries) toConsole(e);
  }
}

function push(entry: LogEntry): void {
  buffer.push(entry);
  if (buffer.length > BUFFER_LIMIT) {
    // 超出上限丢弃最旧条目，防止内存无限增长
    buffer.splice(0, buffer.length - BUFFER_LIMIT);
  }
  if (buffer.length >= FLUSH_THRESHOLD) {
    void flush();
  }
}

function log(
  level: LogLevel,
  scope: string,
  message: string,
  context?: unknown,
): void {
  const entry: LogEntry = {
    level,
    scope,
    message,
    context: context !== undefined ? safeStringify(context) : undefined,
    timestamp: nowIso(),
  };
  // dev 或非 Tauri 环境镜像到 console，保留 DevTools 调试习惯
  if (isDev || !inTauri) toConsole(entry);
  push(entry);
}

export const logger = {
  trace: (scope: string, message: string, context?: unknown): void =>
    log('trace', scope, message, context),
  debug: (scope: string, message: string, context?: unknown): void =>
    log('debug', scope, message, context),
  info: (scope: string, message: string, context?: unknown): void =>
    log('info', scope, message, context),
  warn: (scope: string, message: string, context?: unknown): void =>
    log('warn', scope, message, context),
  error: (scope: string, message: string, context?: unknown): void =>
    log('error', scope, message, context),
};

/**
 * 启动定时 flush。在应用入口调用一次。重复调用幂等。
 *
 * 未调用时仍可工作（靠阈值触发 flush），但尾部日志可能延迟至下一次阈值；
 * 启动定时器保证低频日志也能及时落盘。
 */
export function startLogger(): void {
  if (flushTimer) return;
  flushTimer = setInterval(() => {
    void flush();
  }, FLUSH_INTERVAL_MS);
}

/** 停止定时 flush 并刷盘剩余日志。应用卸载时调用。 */
export async function stopLogger(): Promise<void> {
  if (flushTimer) {
    clearInterval(flushTimer);
    flushTimer = null;
  }
  await flush();
}
