/**
 * 半预览中外部链接的安全打开。
 *
 * 安全边界：
 *   - 仅放行显式声明且属于白名单的协议：http/https/mailto
 *   - 无协议的相对地址视为站内资源（当前阶段不处理，静默忽略）
 *   - 一切构造失败、含控制字符的输入一律拒绝，最终走 @tauri-apps/plugin-opener
 */

/** 允许通过系统方式打开的协议白名单。 */
const ALLOWED_PROTOCOLS = new Set(['http:', 'https:', 'mailto:']);

/** 显式协议形态的前缀（如 `https:`、`mailto:`）。 */
const EXPLICIT_SCHEME = /^[a-zA-Z][a-zA-Z0-9+.-]*:/;

/** 判定外链是否安全可开（纯函数，可测）。 */
export function isSafeExternalHref(href: string): boolean {
  const trimmed = href.trim();
  if (!trimmed || trimmed.length > 4096) return false;
  // 含控制字符直接拒绝（协议混淆攻击面）
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001F\u007F]/.test(trimmed)) return false;
  if (!EXPLICIT_SCHEME.test(trimmed)) return false;
  try {
    return ALLOWED_PROTOCOLS.has(new URL(trimmed).protocol);
  } catch {
    return false;
  }
}

/** 打开外链；非白名单地址为静默无操作（防探测，不给错误反馈）。 */
export async function openExternalLink(href: string): Promise<void> {
  if (!isSafeExternalHref(href)) return;
  const { openUrl } = await import('@tauri-apps/plugin-opener');
  await openUrl(href.trim());
}

// ===== 单元测试（守卫：仅在 vitest 环境运行） =====

if (import.meta.vitest) {
  const { describe, it, expect } = import.meta.vitest;

  describe('linkOpen: isSafeExternalHref', () => {
    it.each(['https://example.com', 'http://a.b/c?d=1#f', 'mailto:a@b.c'])(
      '放行 %j',
      (href) => {
        expect(isSafeExternalHref(href)).toBe(true);
      },
    );

    it.each([
      'javascript:alert(1)',
      'data:text/html,<script>',
      'file:///C:/Windows',
      'ftp://x',
      'vbscript:x',
      '/relative/path.md',
      './assets/a.png',
      '',
      '   ',
      '\u0001javascript:alert(1)',
      'java\u0000script:alert(1)',
    ])('拒绝 %j', (href) => {
      expect(isSafeExternalHref(href)).toBe(false);
    });
  });
}
