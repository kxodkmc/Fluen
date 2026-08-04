/**
 * markdown-it-texmath 类型声明。
 *
 * 该包未自带 TypeScript 类型，此处提供最小可用声明。
 * KaTeX 作为 engine 传入，delimiters 支持 'dollars' / 'brackets' / 'gitlab' 等。
 *
 * @see https://github.com/goessner/markdown-it-texmath
 */

declare module 'markdown-it-texmath' {
  export interface TexmathOptions {
    /** KaTeX 实例（渲染引擎）。 */
    engine?: object;
    /** 分隔符模式，默认 'dollars'。可传数组合并多种模式。 */
    delimiters?: string | string[];
    /** inline $...$ 要求前后有空格（防误识别），默认 false。 */
    outerSpace?: boolean;
    /** 透传给 KaTeX renderToString 的选项。 */
    katexOptions?: Record<string, unknown>;
    /** KaTeX 宏定义（旧版兼容，等同 katexOptions.macros）。 */
    macros?: Record<string, string>;
  }

  const texmath: (md: import('markdown-it').MarkdownIt, options?: TexmathOptions) => void;
  export default texmath;
}
