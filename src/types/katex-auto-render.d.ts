/**
 * KaTeX auto-render 类型声明。
 *
 * katex 包未为 contrib/auto-render 提供 TypeScript 类型，
 * 此处提供最小可用声明。
 *
 * @see https://katex.org/docs/browser.html#auto-render
 */

declare module 'katex/contrib/auto-render' {
  /** auto-render 分隔符配置。 */
  interface AutoRenderDelimiter {
    left: string;
    right: string;
    /** true → display mode（块级），false → inline mode。 */
    display: boolean;
  }

  /** renderMathInElement 选项。 */
  interface AutoRenderOptions {
    /** 分隔符列表。 */
    delimiters?: AutoRenderDelimiter[];
    /** 忽略的 HTML 标签（内容不处理）。 */
    ignoredTags?: string[];
    /** 忽略的 CSS 类名（祖先含此类名的元素不处理）。 */
    ignoredClasses?: string[];
    /** 错误回调。 */
    errorCallback?: (msg: string, err: Error) => void;
    /** KaTeX 宏定义。 */
    macros?: Record<string, string>;
    /** KaTeX 选项（透传）。 */
    throwOnError?: boolean;
    /** 预处理公式内容。 */
    preProcess?: (str: string) => string;
    [key: string]: unknown;
  }

  function renderMathInElement(element: HTMLElement, options?: AutoRenderOptions): void;
  export default renderMathInElement;
}
