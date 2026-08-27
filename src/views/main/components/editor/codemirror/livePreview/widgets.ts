/**
 * 半预览模式的视图 Widget。
 *
 * 安全约定：
 *   - KaTeX 使用默认信任配置（trust:false），禁用 \href、\includegraphics 等；
 *     只有本文件导出的 `katexMathHtml()` 的返回值允许进入 widget DOM，
 *     渲染失败时回退为转义后的纯文本原文——绝无原始用户输入直接 innerHTML。
 *   - 列表圆点 / 任务框为静态字符，无动态内容。
 */

import { WidgetType } from '@codemirror/view';
import katex from 'katex';

/** KaTeX 安全渲染选项：throwOnError 关闭 + 失败回退由调用方处理。 */
const KATEX_OPTIONS = {
  throwOnError: false,
  strict: 'ignore' as const,
  // trust 默认即 false：\href/\includegraphics 等不被执行，防 TeX 注入
};

/**
 * 渲染 LaTeX 为 KaTeX HTML 字符串（纯函数，无 DOM 依赖，可测）。
 *
 * @param tex         LaTeX 源文本（定界符之间的内容）。
 * @param displayMode true 为块级展示式排版。
 * @returns 成功时 `{ ok: true, html }`；失败（KaTeX 抛错）时
 *          `{ ok: false, html }`，其中 html 为已 HTML 转义的原文文本，
 *          调用方可作为降级内容直接插入。
 */
export function katexMathHtml(
  tex: string,
  displayMode: boolean,
): { ok: boolean; html: string } {
  try {
    return {
      ok: true,
      html: katex.renderToString(tex, { ...KATEX_OPTIONS, displayMode }),
    };
  } catch {
    // 双保险：即便 throwOnError:false 仍可能因未知异常抛出（如非法嵌套）
    return { ok: false, html: escapeHtml(tex) };
  }
}

/** HTML 实体转义（与 FluenPreview 错误展示相同的防护等级）。 */
function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

/**
 * 数学公式 widget。默认通过原子区间交互：光标落在边缘即揭示原文编辑
 * （见 decorations.ts 的揭示判定），widget 本身不拦截任何事件。
 */
export class MathWidget extends WidgetType {
  constructor(
    readonly tex: string,
    readonly displayMode: boolean,
  ) {
    super();
  }

  override eq(other: MathWidget): boolean {
    return other.tex === this.tex && other.displayMode === this.displayMode;
  }

  override toDOM(): HTMLElement {
    const host = document.createElement('span');
    host.className = this.displayMode
      ? 'fluen-lp-math fluen-lp-math--display'
      : 'fluen-lp-math';
    const { ok, html } = katexMathHtml(this.tex, this.displayMode);
    host.innerHTML = html;
    if (!ok) host.classList.add('fluen-lp-math--invalid');
    return host;
  }
}

/** 无序列表圆点 widget（替换 `-`/`*`/`+` 标记）。 */
export class BulletWidget extends WidgetType {
  constructor(readonly nestedDepth = 0) {
    super();
  }

  override eq(other: BulletWidget): boolean {
    return other.nestedDepth === this.nestedDepth;
  }

  override toDOM(): HTMLElement {
    const el = document.createElement('span');
    el.className = 'fluen-lp-bullet';
    // 圆点随层级变化视觉重量：第一层实心、第二层空心圆、更深层小方块
    el.textContent = this.nestedDepth === 0 ? '•' : this.nestedDepth === 1 ? '◦' : '▪';
    el.setAttribute('aria-hidden', 'true');
    return el;
  }
}

/** 任务列表复选框 widget（只读视觉态；勾选状态的修改走源码编辑）。 */
export class TaskCheckboxWidget extends WidgetType {
  constructor(readonly checked: boolean) {
    super();
  }

  override eq(other: TaskCheckboxWidget): boolean {
    return other.checked === this.checked;
  }

  override toDOM(): HTMLElement {
    const el = document.createElement('span');
    el.className = 'fluen-lp-taskbox' + (this.checked ? ' fluen-lp-taskbox--on' : '');
    el.textContent = this.checked ? '☑' : '☐';
    el.setAttribute('aria-hidden', 'true');
    return el;
  }
}

// ===== 单元测试（守卫：仅在 vitest 环境运行，生产构建被 define 树摇） =====

if (import.meta.vitest) {
  const { describe, it, expect } = import.meta.vitest;

  describe('widgets: katexMathHtml 安全渲染', () => {
    it('普通公式正常渲染', () => {
      const r = katexMathHtml('E=mc^2', false);
      expect(r.ok).toBe(true);
      expect(r.html).toContain('katex');
    });

    it('\\href 在 trust:false 下不可产生超链接', () => {
      const r = katexMathHtml(String.raw`\href{https://evil.example}{x}`, false);
      // 要么整体失败回退纯文本，要么成功但不含 href 属性——两者都安全
      if (r.ok) {
        expect(r.html).not.toMatch(/href\s*=/);
      }
    });

    it('HTML 注入 payload 不产生可执行标签', () => {
      const r = katexMathHtml('<img src=x onerror=alert(1)>', false);
      // KaTeX 将整串当作文本排版：不得出现真实 <img> 标签或 onerror 属性形态
      expect(r.html.toLowerCase()).not.toMatch(/<img/i);
      expect(r.html.toLowerCase()).not.toMatch(/<[a-z][^>]*\sonerror/i);
    });

    it('脚本注入 payload 被转义', () => {
      const r = katexMathHtml('<script>alert(1)</script>', false);
      expect(r.html.toLowerCase()).not.toMatch(/<script/i);
    });

    it('引号注入被转义为实体', () => {
      const r = katexMathHtml(String.raw`\text{" onload="alert(1)" x="`, false);
      expect(r.html).not.toContain('" onload');
    });
  });
}
