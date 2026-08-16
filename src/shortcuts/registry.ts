/**
 * 快捷键模块 — 命令/绑定注册表与派发。
 *
 * 纯逻辑单例：维护两张表
 * - 命令表：`CommandId → CommandHandler`（业务处理器）；
 * - 绑定表：`键位字符串 → KeyBinding + CommandId`（键位到命令的映射）。
 *
 * 不依赖 DOM 与日志，处理器异常通过 `onHandlerError` 回调外抛（由门面层接日志），
 * 保证注册表可独立测试、与业务彻底解耦。
 *
 * 语义：
 * - `handle` 按绑定表顺序匹配首个命中的事件，命中即消费（返回 true）；
 * - 命中但命令未注册：仍视为消费（阻止默认行为），由调用方决定是否告警；
 * - 同一键位字符串重复绑定：后者覆盖前者。
 */

import { matches, parseBinding } from './keybind';
import type { CommandHandler, CommandId, KeyBinding } from './types';

/** 绑定表条目：解析后的键位 + 目标命令。 */
interface BindingEntry {
  binding: KeyBinding;
  commandId: CommandId;
}

export class ShortcutRegistry {
  private readonly commands = new Map<CommandId, CommandHandler>();
  /** key 为原始键位字符串（trim 后），保证同键位绑定幂等覆盖。 */
  private readonly bindings = new Map<string, BindingEntry>();

  /** 命令处理器抛错时的回调（由门面层接入日志）。 */
  onHandlerError: ((commandId: CommandId, error: unknown) => void) | null = null;

  /** 命令是否已注册。 */
  hasCommand(id: CommandId): boolean {
    return this.commands.has(id);
  }

  /**
   * 注册命令处理器。
   *
   * @returns `true` 表示新注册；`false` 表示覆盖了已有命令（调用方可据此告警）。
   */
  registerCommand(id: CommandId, handler: CommandHandler): boolean {
    const isNew = !this.commands.has(id);
    this.commands.set(id, handler);
    return isNew;
  }

  /** 注销命令。命令对应的绑定保留（绑定到未注册命令的键位仍会消费事件）。 */
  unregisterCommand(id: CommandId): void {
    this.commands.delete(id);
  }

  /**
   * 绑定键位字符串到命令。
   *
   * @returns `true` 绑定成功；键位字符串非法返回 `false`。
   */
  bind(spec: string, commandId: CommandId): boolean {
    const trimmed = spec.trim();
    const binding = parseBinding(trimmed);
    if (!binding) return false;
    this.bindings.set(trimmed, { binding, commandId });
    return true;
  }

  /** 解绑键位（仅当当前绑定到指定命令时移除，避免误删其他命令的绑定）。 */
  unbind(spec: string, commandId: CommandId): void {
    const entry = this.bindings.get(spec.trim());
    if (entry && entry.commandId === commandId) {
      this.bindings.delete(spec.trim());
    }
  }

  /** 清空命令表与绑定表（诊断/测试用）。 */
  clear(): void {
    this.commands.clear();
    this.bindings.clear();
  }

  /** 当前绑定数量（供测试/诊断）。 */
  get bindingCount(): number {
    return this.bindings.size;
  }

  /** 当前命令数量（供测试/诊断）。 */
  get commandCount(): number {
    return this.commands.size;
  }

  /**
   * 匹配并派发一次键盘事件。
   *
   * @returns `true` 表示有绑定命中（事件已被消费，调用方应阻止默认行为）。
   */
  handle(event: KeyboardEvent): boolean {
    for (const { binding, commandId } of this.bindings.values()) {
      if (!matches(binding, event)) continue;
      const handler = this.commands.get(commandId);
      if (handler) {
        try {
          handler(event);
        } catch (err) {
          // 单条命令异常不中断监听；由门面层记录日志
          this.onHandlerError?.(commandId, err);
        }
      }
      return true;
    }
    return false;
  }
}

/** 全局唯一注册表实例。 */
export const shortcutRegistry = new ShortcutRegistry();

// ===== 单元测试（守卫：仅在 vitest 环境运行，生产构建被 vite.config.ts define 树摇）=====

if (import.meta.vitest) {
  const { describe, it, expect } = import.meta.vitest;

  function ev(init: { key: string; code?: string; ctrlKey?: boolean; shiftKey?: boolean; altKey?: boolean; metaKey?: boolean; }): KeyboardEvent {
    return new KeyboardEvent('keydown', {
      key: init.key,
      code: init.code,
      ctrlKey: init.ctrlKey ?? false,
      metaKey: init.metaKey ?? false,
      shiftKey: init.shiftKey ?? false,
      altKey: init.altKey ?? false,
      bubbles: true,
      cancelable: true,
    });
  }

  /** 每个用例使用独立注册表，避免用例间状态污染。 */
  function freshRegistry(): ShortcutRegistry {
    return new ShortcutRegistry();
  }

  describe('ShortcutRegistry: 命令注册/注销', () => {
    it('注册返回是否为新命令，重复注册返回 false 且覆盖', () => {
      const r = freshRegistry();
      expect(r.registerCommand('a', () => {})).toBe(true);
      expect(r.registerCommand('a', () => {})).toBe(false);
      expect(r.commandCount).toBe(1);
      expect(r.hasCommand('a')).toBe(true);
    });

    it('注销后命令不可再派发', () => {
      const r = freshRegistry();
      let called = 0;
      r.registerCommand('a', () => { called += 1; });
      r.unregisterCommand('a');
      expect(r.hasCommand('a')).toBe(false);
      expect(r.commandCount).toBe(0);
      // 无命令时也不报错
      r.unregisterCommand('a');
    });
  });

  describe('ShortcutRegistry: 键位绑定', () => {
    it('非法键位绑定返回 false，不产生条目', () => {
      const r = freshRegistry();
      expect(r.bind('NotAKey', 'a')).toBe(false);
      expect(r.bindingCount).toBe(0);
    });

    it('同键位重复绑定被后者覆盖', () => {
      const r = freshRegistry();
      r.registerCommand('a', () => {});
      r.registerCommand('b', () => {});
      expect(r.bind('Mod-s', 'a')).toBe(true);
      expect(r.bind('Mod-s', 'b')).toBe(true);
      expect(r.bindingCount).toBe(1);

      let called: string | null = null;
      r.registerCommand('a', () => { called = 'a'; });
      r.registerCommand('b', () => { called = 'b'; });
      r.handle(ev({ key: 's', code: 'KeyS', ctrlKey: true }));
      expect(called).toBe('b');
    });

    it('unbind 仅解绑指定命令的绑定', () => {
      const r = freshRegistry();
      r.registerCommand('a', () => {});
      r.bind('Mod-s', 'a');
      r.unbind('Mod-s', 'other');
      expect(r.bindingCount).toBe(1);
      r.unbind('Mod-s', 'a');
      expect(r.bindingCount).toBe(0);
    });
  });

  describe('ShortcutRegistry: 派发', () => {
    it('命中绑定并派发命令，返回 true 消费事件', () => {
      const r = freshRegistry();
      let called = 0;
      r.registerCommand('save', () => { called += 1; });
      r.bind('Mod-s', 'save');
      expect(r.handle(ev({ key: 's', code: 'KeyS', ctrlKey: true }))).toBe(true);
      expect(called).toBe(1);
    });

    it('未命中任何绑定返回 false（不消费）', () => {
      const r = freshRegistry();
      r.registerCommand('save', () => {});
      r.bind('Mod-s', 'save');
      expect(r.handle(ev({ key: 'k', code: 'KeyK', ctrlKey: true }))).toBe(false);
      expect(r.handle(ev({ key: 's', code: 'KeyS' }))).toBe(false);
    });

    it('命中但命令未注册：仍消费事件，不抛错', () => {
      const r = freshRegistry();
      r.bind('Mod-s', 'missing-command');
      expect(r.handle(ev({ key: 's', code: 'KeyS', ctrlKey: true }))).toBe(true);
    });

    it('命令注销后绑定保留，命中仍消费事件（组件卸载场景）', () => {
      const r = freshRegistry();
      let called = 0;
      r.registerCommand('save', () => { called += 1; });
      r.bind('Mod-s', 'save');
      r.unregisterCommand('save');

      // 不再触发处理器，但事件仍被消费（阻止默认行为）
      expect(r.handle(ev({ key: 's', code: 'KeyS', ctrlKey: true }))).toBe(true);
      expect(called).toBe(0);
      expect(r.bindingCount).toBe(1);
    });

    it('处理器抛错时消费事件并回调 onHandlerError，不中断', () => {
      const r = freshRegistry();
      const errors: Array<[string, unknown]> = [];
      r.onHandlerError = (id, err) => { errors.push([id, err]); };
      r.registerCommand('boom', () => { throw new Error('boom'); });
      r.registerCommand('ok', () => {});
      r.bind('Mod-s', 'boom');
      r.bind('Mod-k', 'ok');

      expect(r.handle(ev({ key: 's', code: 'KeyS', ctrlKey: true }))).toBe(true);
      expect(errors.length).toBe(1);
      expect(errors[0]![0]).toBe('boom');

      // 后续绑定仍可正常派发
      expect(r.handle(ev({ key: 'k', code: 'KeyK', ctrlKey: true }))).toBe(true);
      expect(errors.length).toBe(1);
    });
  });
}
