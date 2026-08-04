/**
 * 作用域日志 composable。
 *
 * 绑定一个 `scope` 标签，返回便捷方法，调用方无需每次重复传入 scope。
 * 底层委托给 [`logger`](../utils/logger)。
 *
 * @example
 * ```ts
 * const log = useLogger('editor');
 * log.info('保存成功', { path });
 * log.error('渲染失败', err);
 * ```
 */

import { logger } from '../utils/logger';

export interface ScopedLogger {
  trace: (message: string, context?: unknown) => void;
  debug: (message: string, context?: unknown) => void;
  info: (message: string, context?: unknown) => void;
  warn: (message: string, context?: unknown) => void;
  error: (message: string, context?: unknown) => void;
}

export function useLogger(scope: string): ScopedLogger {
  return {
    trace: (message, context) => logger.trace(scope, message, context),
    debug: (message, context) => logger.debug(scope, message, context),
    info: (message, context) => logger.info(scope, message, context),
    warn: (message, context) => logger.warn(scope, message, context),
    error: (message, context) => logger.error(scope, message, context),
  };
}
