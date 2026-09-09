/**
 * KaTeX 安全渲染（纯函数，无 DOM 依赖）。
 *
 * 从 widgets.ts 抽出以消除循环依赖（widgets ↔ tableEditing ↔ cellMath
 * 均需 KaTeX 渲染能力，本模块作为无依赖的底层供各方引用）。
 *
 * 安全约定：KaTeX 使用默认信任配置（trust:false），禁用 \href、
 * \includegraphics 等；渲染失败时回退为转义后的纯文本原文——
 * 绝无原始用户输入直接 innerHTML 的注入面。
 */

import katex from 'katex';

/** KaTeX 安全渲染选项：throwOnError 关闭 + 失败回退由调用方处理。 */
const KATEX_OPTIONS = {
  throwOnError: false,
  strict: 'ignore' as const,
  // trust 默认即 false：\href/\includegraphics 等不被执行，防 TeX 注入
};

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
