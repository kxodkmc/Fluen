/**
 * 快捷键模块 — 键位字符串解析与事件匹配。
 *
 * 负责两类职责（纯逻辑，不触碰 DOM/业务）：
 * 1. `parseBinding`：把 `Mod-s`、`Ctrl+Shift+P`、`F5`、`Alt+ArrowUp` 等
 *    人类可读的键位字符串解析为规范 `KeyBinding`；
 * 2. `matches`：判断一次 `KeyboardEvent` 是否命中某绑定。
 *
 * 键位语法约定（与 CodeMirror 6 的 keymap 写法保持一致，降低心智负担）：
 * - 修饰键 token（不区分大小写）：`Ctrl` / `Control`、`Cmd` / `Command` / `Meta`、
 *   `Mod`（跨平台占位符）、`Shift`、`Alt` / `Option`；
 * - 多个修饰键用 `+` 或 `-` 连接（`Ctrl+Shift+P` 与 `Ctrl-Shift-P` 等价），
 *   最后一个 token 为主键；`Mod-s` 是 CodeMirror 风格的常见写法；
 * - 主键支持：单个字母（`s`、`P`）、数字（`1`）、F 键（`F5`）与规范特殊键名
 *   （`Enter`、`Escape`、`Tab`、`Space`、`ArrowUp` 等）。
 *
 * 匹配语义（VSCode 风格严格匹配）：绑定中**未声明**的修饰键在事件中必须处于
 * 未按下状态，因此 `Mod-s` 不会误命中 `Ctrl+Shift+S`。
 */

import type { KeyBinding, Modifier, ShortcutPlatform } from './types';

/** 修饰键 token → 修饰键（小写规范化）。`mod` 由调用方按平台二次映射。 */
const MODIFIER_TOKENS: Record<string, Modifier | 'mod'> = {
  ctrl: 'ctrl',
  control: 'ctrl',
  cmd: 'meta',
  command: 'meta',
  meta: 'meta',
  mod: 'mod',
  shift: 'shift',
  alt: 'alt',
  option: 'alt',
};

/** 特殊键名：小写别名 → 规范形式。 */
const SPECIAL_KEYS: Record<string, string> = {
  enter: 'Enter',
  return: 'Enter',
  esc: 'Escape',
  escape: 'Escape',
  tab: 'Tab',
  space: 'Space',
  spacebar: 'Space',
  backspace: 'Backspace',
  delete: 'Delete',
  insert: 'Insert',
  home: 'Home',
  end: 'End',
  pageup: 'PageUp',
  pagedown: 'PageDown',
  arrowup: 'ArrowUp',
  arrowdown: 'ArrowDown',
  arrowleft: 'ArrowLeft',
  arrowright: 'ArrowRight',
};

/** 事件 `code` 中可直接采用的规范特殊键名（其 code 值与规范名一致）。 */
const CODE_SPECIAL_KEYS = new Set([
  'Enter',
  'Escape',
  'Tab',
  'Space',
  'Backspace',
  'Delete',
  'Insert',
  'Home',
  'End',
  'PageUp',
  'PageDown',
  'ArrowUp',
  'ArrowDown',
  'ArrowLeft',
  'ArrowRight',
]);

/** 运行时平台检测：macOS（含 iOS 系 UA）判定为 mac。 */
function detectPlatform(): ShortcutPlatform {
  const ua = typeof navigator !== 'undefined' ? navigator.userAgent : '';
  return /Mac|iPhone|iPad/.test(ua) ? 'mac' : 'other';
}

/** 规范化主键 token（字母转小写、特殊键名统一）。 */
function normalizeBindingKey(token: string): string | null {
  if (/^[a-zA-Z]$/.test(token)) return token.toLowerCase();
  if (/^[0-9]$/.test(token)) return token;
  const special = SPECIAL_KEYS[token.toLowerCase()];
  if (special) return special;
  const fMatch = /^f([1-9][0-9]?)$/i.exec(token);
  if (fMatch) return `F${fMatch[1]}`;
  return null;
}

/**
 * 解析键位字符串为规范绑定。
 *
 * @param spec      键位字符串，如 `Mod-s` / `Ctrl+Shift+P`。
 * @param platform  平台覆盖（默认运行时检测）。用于 `Mod` 的映射：mac → meta，其余 → ctrl。
 * @returns 解析成功返回绑定，格式非法返回 `null`（由调用方记录日志并忽略）。
 */
export function parseBinding(
  spec: string,
  platform: ShortcutPlatform = detectPlatform(),
): KeyBinding | null {
  const tokens = spec
    .split(/[+\-]/)
    .map((t) => t.trim())
    .filter((t) => t.length > 0);
  if (tokens.length === 0) return null;

  const binding: KeyBinding = {
    key: '',
    ctrl: false,
    meta: false,
    shift: false,
    alt: false,
  };

  // 最后一个 token 必须是主键，其余必须是修饰键
  const rawKey = tokens[tokens.length - 1]!;
  const key = normalizeBindingKey(rawKey);
  if (!key) return null;
  binding.key = key;

  for (const token of tokens.slice(0, -1)) {
    const mod = MODIFIER_TOKENS[token.toLowerCase()];
    if (!mod) return null;
    if (mod === 'mod') {
      // 跨平台占位符：按平台落地为 ctrl / meta
      if (platform === 'mac') binding.meta = true;
      else binding.ctrl = true;
    } else {
      binding[mod] = true;
    }
  }
  return binding;
}

/**
 * 从事件中提取规范主键名。
 *
 * 优先使用 `e.code`（不受 Shift 影响，`Shift+1` 仍解析为 `1`），
 * 再回退到 `e.key` 归一化（兼容 code 缺失的环境，如部分测试容器）。
 */
export function eventKeyName(e: KeyboardEvent): string {
  const code = e.code;
  if (code) {
    const letter = /^Key([A-Z])$/.exec(code);
    if (letter) return letter[1]!.toLowerCase();
    const digit = /^Digit([0-9])$/.exec(code);
    if (digit) return digit[1]!;
    const fn = /^F([1-9][0-9]?)$/.exec(code);
    if (fn) return `F${fn[1]}`;
    if (CODE_SPECIAL_KEYS.has(code)) return code;
  }
  // 兜底：e.key 归一化
  const k = e.key;
  if (k.length === 1) return k.toLowerCase();
  const special = SPECIAL_KEYS[k.toLowerCase()];
  if (special) return special;
  if (k === ' ') return 'Space';
  return k;
}

/**
 * 判断事件是否命中绑定（修饰键严格匹配 + 主键相等）。
 *
 * 严格匹配的含义：绑定未声明的修饰键在事件中必须为未按下。
 * 因此 `Mod-s` 不会命中 `Ctrl+Shift+S`，`Ctrl+Shift+S` 需单独绑定。
 */
export function matches(binding: KeyBinding, e: KeyboardEvent): boolean {
  if (!!e.ctrlKey !== binding.ctrl) return false;
  if (!!e.metaKey !== binding.meta) return false;
  if (!!e.shiftKey !== binding.shift) return false;
  if (!!e.altKey !== binding.alt) return false;
  return eventKeyName(e) === binding.key;
}

// ===== 单元测试（守卫：仅在 vitest 环境运行，生产构建被 vite.config.ts define 树摇）=====

if (import.meta.vitest) {
  const { describe, it, expect } = import.meta.vitest;

  /** 构造测试事件（jsdom 可能缺失 code，主键兜底走 e.key 归一化，两条路径均可覆盖）。 */
  function ev(init: { key: string; code?: string; ctrlKey?: boolean; metaKey?: boolean; shiftKey?: boolean; altKey?: boolean; }): KeyboardEvent {
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

  describe('parseBinding: 键位字符串解析', () => {
    it('Mod 在非 mac 平台映射为 ctrl', () => {
      const b = parseBinding('Mod-s', 'other')!;
      expect(b).toEqual({ key: 's', ctrl: true, meta: false, shift: false, alt: false });
    });

    it('Mod 在 mac 平台映射为 meta', () => {
      const b = parseBinding('Mod-s', 'mac')!;
      expect(b.meta).toBe(true);
      expect(b.ctrl).toBe(false);
    });

    it('Ctrl+Shift+P 多修饰键 + 大写主键归一化', () => {
      const b = parseBinding('Ctrl+Shift+P', 'other')!;
      expect(b).toEqual({ key: 'p', ctrl: true, meta: false, shift: true, alt: false });
    });

    it('Cmd+1 数字主键', () => {
      const b = parseBinding('Cmd+1', 'mac')!;
      expect(b).toEqual({ key: '1', ctrl: false, meta: true, shift: false, alt: false });
    });

    it('Mod-1（连字符分隔 + 数字主键，视图切换用）', () => {
      const b = parseBinding('Mod-1', 'other')!;
      expect(b).toEqual({ key: '1', ctrl: true, meta: false, shift: false, alt: false });
    });

    it('Option/Alt 别名归一', () => {
      expect(parseBinding('Option+ArrowUp', 'mac')!.alt).toBe(true);
      expect(parseBinding('alt+ArrowUp', 'mac')!.alt).toBe(true);
    });

    it('特殊键名：Esc/Return/空格 别名 → 规范形式', () => {
      expect(parseBinding('Esc', 'other')!.key).toBe('Escape');
      expect(parseBinding('Return', 'other')!.key).toBe('Enter');
      expect(parseBinding('Space', 'other')!.key).toBe('Space');
      expect(parseBinding('F5', 'other')!.key).toBe('F5');
      expect(parseBinding('f12', 'other')!.key).toBe('F12');
    });

    it('非法键位返回 null', () => {
      expect(parseBinding('', 'other')).toBeNull();
      expect(parseBinding('Ctrl+', 'other')).toBeNull();
      expect(parseBinding('Ctrl+NotAKey', 'other')).toBeNull();
      expect(parseBinding('Foo+s', 'other')).toBeNull();
    });
  });

  describe('matches: 事件匹配', () => {
    it('Ctrl+S 命中 Mod-s（other 平台）', () => {
      const b = parseBinding('Mod-s', 'other')!;
      expect(matches(b, ev({ key: 's', code: 'KeyS', ctrlKey: true }))).toBe(true);
    });

    it('无修饰的 Ctrl+S 不命中（未声明修饰键必须未按下）', () => {
      const b = parseBinding('Mod-s', 'other')!;
      expect(matches(b, ev({ key: 's', code: 'KeyS' }))).toBe(false);
      expect(matches(b, ev({ key: 's', code: 'KeyS', ctrlKey: true, shiftKey: true }))).toBe(false);
    });

    it('Ctrl+Shift+S 命中显式绑定，但不命中 Mod-s', () => {
      const plain = parseBinding('Mod-s', 'other')!;
      const shifted = parseBinding('Ctrl+Shift+S', 'other')!;
      const e = ev({ key: 'S', code: 'KeyS', ctrlKey: true, shiftKey: true });
      expect(matches(shifted, e)).toBe(true);
      expect(matches(plain, e)).toBe(false);
    });

    it('F5 单键命中（无修饰键）', () => {
      const b = parseBinding('F5', 'other')!;
      expect(matches(b, ev({ key: 'F5', code: 'F5' }))).toBe(true);
      expect(matches(b, ev({ key: 'F5', code: 'F5', ctrlKey: true }))).toBe(false);
    });

    it('Alt+ArrowUp 命中', () => {
      const b = parseBinding('Alt+ArrowUp', 'other')!;
      expect(matches(b, ev({ key: 'ArrowUp', code: 'ArrowUp', altKey: true }))).toBe(true);
      expect(matches(b, ev({ key: 'ArrowUp', code: 'ArrowUp' }))).toBe(false);
    });

    it('Shift+1 用 code 解析主键为数字 1（而非 key 的 !）', () => {
      const b = parseBinding('Shift+1', 'other')!;
      expect(matches(b, ev({ key: '!', code: 'Digit1', shiftKey: true }))).toBe(true);
    });
  });
}
