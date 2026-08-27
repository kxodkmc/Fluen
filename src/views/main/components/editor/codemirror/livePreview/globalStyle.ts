/**
 * 受控的全局样式注入。
 *
 * 半预览在主文档 DOM 中渲染 KaTeX 公式，需要一次全局样式表。
 * 以 `<style id>` 幂等注入：重复装配不会产生重复节点，
 * 组件卸载不摘除（公式样式全局共享、体积小，避免频繁装卸抖动）。
 */

/** 已注入的 style 元素，按 key 记录以便幂等。 */
const injected = new Map<string, HTMLStyleElement>();

/**
 * 按 key 注入一段全局 CSS；同 key 已存在时为无操作。
 *
 * @param key  样式块唯一标识（同时也是 <style> 的 id）。
 * @param css  原始 CSS 文本。
 */
export function injectGlobalStyle(key: string, css: string): void {
  if (injected.has(key)) return;
  const doc = typeof document === 'undefined' ? undefined : document;
  if (!doc?.head) return;
  const el = doc.createElement('style');
  el.id = `fluen-global-${key}`;
  el.textContent = css;
  doc.head.appendChild(el);
  injected.set(key, el);
}

// ===== 单元测试（守卫：仅在 vitest 环境运行） =====

if (import.meta.vitest) {
  const { describe, it, expect } = import.meta.vitest;

  describe('globalStyle: 幂等注入', () => {
    it('同 key 只注入一次', () => {
      injectGlobalStyle('test-key-a', 'body{}');
      injectGlobalStyle('test-key-a', 'body{x}'); // 第二次应被忽略
      const nodes = document.querySelectorAll('#fluen-global-test-key-a');
      expect(nodes.length).toBe(1);
    });

    it('不同 key 各自注入', () => {
      injectGlobalStyle('test-key-b', '.b{}');
      expect(document.querySelector('#fluen-global-test-key-b')).not.toBeNull();
      expect(document.querySelector('#fluen-global-test-key-c')).toBeNull();
    });
  });
}
