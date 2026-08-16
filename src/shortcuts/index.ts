/**
 * 快捷键模块 — 对外门面（应用入口）。
 *
 * 职责：
 * 1. 全局 `keydown` 监听（capture 阶段）：命中绑定即消费事件——
 *    `preventDefault()` 拦截 WebView2/浏览器默认行为（如 Ctrl+S 保存页面对话框），
 *    `stopPropagation()` 保证全局快捷键优先于组件内/编辑器内处理，杜绝“被占用”；
 * 2. 以命令为单位的便捷注册 API：`registerCommand` / `bindShortcut`；
 * 3. 处理器异常与生命周期事件接入统一日志（scope: `shortcuts`）。
 *
 * 典型用法（业务侧）：
 * ```ts
 * import { registerCommand, bindShortcut } from '../../shortcuts';
 * registerCommand('save-document', () => { void editor.save(); });
 * bindShortcut('Mod-s', 'save-document');
 * ```
 *
 * 生命周期：应用入口 `startShortcuts()`（幂等），卸载时 `stopShortcuts()`。
 */

import { logger } from '../utils/logger';
import { eventKeyName } from './keybind';
import { shortcutRegistry } from './registry';
import type { CommandHandler, CommandId } from './types';

export type { CommandHandler, CommandId, KeyBinding, Modifier, ShortcutPlatform } from './types';
export { parseBinding, eventKeyName } from './keybind';

const LOG_SCOPE = 'shortcuts';

/** 全局 keydown 监听是否已挂载。 */
let attached = false;

/** 全局 keydown 处理器（capture 阶段优先于一切组件/编辑器处理）。 */
function onKeydown(event: KeyboardEvent): void {
  const consumed = shortcutRegistry.handle(event);
  if (consumed) {
    event.preventDefault();
    event.stopPropagation();
    // trace 级记录命中（dev 环境可见，生产由日志系统按级别过滤）
    logger.trace(LOG_SCOPE, '快捷键命中并消费', { key: eventKeyName(event) });
  }
}

/** 启动全局快捷键监听（幂等，重复调用无副作用）。应用入口调用一次。 */
export function startShortcuts(): void {
  if (attached) return;
  document.addEventListener('keydown', onKeydown, true);
  attached = true;
  logger.debug(LOG_SCOPE, '全局快捷键监听已启动');
}

/** 停止全局快捷键监听。应用卸载时调用。 */
export function stopShortcuts(): void {
  if (!attached) return;
  document.removeEventListener('keydown', onKeydown, true);
  attached = false;
  logger.debug(LOG_SCOPE, '全局快捷键监听已停止');
}

/**
 * 注册命令处理器。
 *
 * @param commandId 命令 id（业务语义标识，全局唯一）。
 * @param handler   处理器；命中即视为已消费事件。
 */
export function registerCommand(commandId: CommandId, handler: CommandHandler): void {
  const isNew = shortcutRegistry.registerCommand(commandId, handler);
  if (isNew) {
    logger.debug(LOG_SCOPE, '注册命令', { commandId });
  } else {
    logger.warn(LOG_SCOPE, '命令重复注册，已覆盖旧处理器', { commandId });
  }
}

/** 注销命令处理器（绑定保持不变；绑定到未注册命令的键位仍会消费事件）。 */
export function unregisterCommand(commandId: CommandId): void {
  shortcutRegistry.unregisterCommand(commandId);
  logger.debug(LOG_SCOPE, '注销命令', { commandId });
}

/**
 * 绑定键位字符串到命令。
 *
 * @param spec      键位字符串，如 `Mod-s`、`Ctrl+Shift+P`。
 * @param commandId 目标命令 id。
 * @returns `true` 绑定成功；键位字符串非法返回 `false`。
 */
export function bindShortcut(spec: string, commandId: CommandId): boolean {
  if (!shortcutRegistry.bind(spec, commandId)) {
    logger.error(LOG_SCOPE, '键位绑定失败（格式非法）', { spec, commandId });
    return false;
  }
  logger.debug(LOG_SCOPE, '绑定快捷键', { spec, commandId });
  return true;
}

/** 解绑键位（仅当当前绑定到指定命令时生效）。 */
export function unbindShortcut(spec: string, commandId: CommandId): void {
  shortcutRegistry.unbind(spec, commandId);
  logger.debug(LOG_SCOPE, '解绑快捷键', { spec, commandId });
}

// ── 命令处理器异常接入统一日志（模块加载时挂接一次） ──────────────────
shortcutRegistry.onHandlerError = (commandId, error) => {
  logger.error(LOG_SCOPE, '命令处理器执行异常', {
    commandId,
    error: error instanceof Error ? error.message : String(error),
  });
};

// ===== 单元测试（守卫：仅在 vitest 环境运行，生产构建被 vite.config.ts define 树摇）=====

if (import.meta.vitest) {
  const { describe, it, expect, beforeEach, afterEach } = import.meta.vitest;

  /** 构造可冒泡/可取消的键盘事件。 */
  function ev(init: { key: string; code?: string; ctrlKey?: boolean; }): KeyboardEvent {
    return new KeyboardEvent('keydown', {
      key: init.key,
      code: init.code,
      ctrlKey: init.ctrlKey ?? false,
      metaKey: false,
      shiftKey: false,
      altKey: false,
      bubbles: true,
      cancelable: true,
    });
  }

  /** 用例间隔离：重置监听与注册表状态。 */
  function reset(): void {
    stopShortcuts();
    shortcutRegistry.clear();
  }

  beforeEach(() => {
    reset();
  });

  afterEach(() => {
    reset();
  });

  describe('startShortcuts / stopShortcuts: 全局监听生命周期', () => {
    it('startShortcuts 幂等，重复调用不叠加监听', () => {
      startShortcuts();
      startShortcuts();
      let called = 0;
      registerCommand('cmd', () => { called += 1; });
      bindShortcut('Mod-s', 'cmd');
      document.body.dispatchEvent(ev({ key: 's', code: 'KeyS', ctrlKey: true }));
      expect(called).toBe(1);
    });

    it('stopShortcuts 后事件不再被消费', () => {
      startShortcuts();
      registerCommand('cmd', () => {});
      bindShortcut('Mod-s', 'cmd');
      stopShortcuts();

      const e = ev({ key: 's', code: 'KeyS', ctrlKey: true });
      document.body.dispatchEvent(e);
      expect(e.defaultPrevented).toBe(false);
    });
  });

  describe('全局派发', () => {
    it('命中绑定：处理器被调用，事件被 preventDefault 且停止冒泡', () => {
      startShortcuts();
      let called = 0;
      registerCommand('save', () => { called += 1; });
      bindShortcut('Mod-s', 'save');

      // window 级监听用于验证 stopPropagation（document 监听之后不应再收到）
      let windowHeard = false;
      const onWindowKey = (): void => { windowHeard = true; };
      window.addEventListener('keydown', onWindowKey);

      const e = ev({ key: 's', code: 'KeyS', ctrlKey: true });
      document.body.dispatchEvent(e);

      expect(called).toBe(1);
      expect(e.defaultPrevented).toBe(true);
      expect(windowHeard).toBe(false);
      window.removeEventListener('keydown', onWindowKey);
    });

    it('未绑定任何键位的事件不被消费', () => {
      startShortcuts();
      registerCommand('save', () => {});
      bindShortcut('Mod-s', 'save');

      const e = ev({ key: 'k', code: 'KeyK', ctrlKey: true });
      document.body.dispatchEvent(e);
      expect(e.defaultPrevented).toBe(false);
    });

    it('capture 阶段优先于 bubble 阶段监听（编辑器内 Ctrl+S 也被全局接管）', () => {
      startShortcuts();
      registerCommand('save', () => {});
      bindShortcut('Mod-s', 'save');

      // 模拟编辑器/组件在 bubble 阶段注册的处理器：不应被触发（事件已在 capture 阶段被消费）
      let bubbleHeard = false;
      const onBubbleKey = (): void => { bubbleHeard = true; };
      document.body.addEventListener('keydown', onBubbleKey);

      const e = ev({ key: 's', code: 'KeyS', ctrlKey: true });
      document.body.dispatchEvent(e);
      expect(bubbleHeard).toBe(false);
      document.body.removeEventListener('keydown', onBubbleKey);
    });
  });
}
