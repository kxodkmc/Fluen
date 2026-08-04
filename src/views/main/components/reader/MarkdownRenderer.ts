/**
 * MarkdownRenderer —— 标准 MD 渲染器（文献阅读专用）。
 *
 * 基于 markdown-it，核心职责：
 *   1. 渲染标准 MD 为 HTML（代码高亮 via highlight.js）
 *   2. 为每个可标注块级元素注入 `data-block-key` + `data-line`（标记/翻译锚点）
 *   3. 生成 BlockMap（含 fingerprint，为阶段二标记功能准备）
 *   4. 图片 renderer hook：通过 token 树收集 + 替换 src 为 data URL（零正则）
 *
 * 设计要点：
 *   - 锚点三重保险：BlockKey(type+line+occurrence) + Fingerprint(hash+prefix+suffix) + 文本
 *   - occurrence 解决同行多 block 冲突
 *   - 资源替换基于 markdown-it 完整解析的 token 树，覆盖 inline / reference-style / title 语法
 *   - 渲染流程：parse → buildBlockMap（注入 attrs）→ render（图片 hook 替换 src）
 *
 * @module reader/MarkdownRenderer
 */

import MarkdownIt from 'markdown-it';
import type { Token } from 'markdown-it';
import hljs from 'highlight.js/lib/common';
import katexEngine from 'katex';
import texmath from 'markdown-it-texmath';
import type {
  BlockMap,
  BlockMapEntry,
  BlockType,
  Fingerprint,
} from '../../../../types/reader';

// ---------------------------------------------------------------------------
// 类型
// ---------------------------------------------------------------------------

/** 渲染结果：HTML + 块映射表。 */
export interface RenderResult {
  /** 完整 HTML 片段（不含 html/head/body 包裹）。 */
  html: string;
  /** 块映射表，供标记/翻译锚点解析。 */
  blockMap: BlockMap;
}

/** 渲染环境，携带图片资源映射。 */
interface RenderEnv {
  [key: string]: unknown;
  [key: symbol]: unknown;
  assetMap?: Map<string, string>;
}

// ---------------------------------------------------------------------------
// 可标注块类型白名单
// ---------------------------------------------------------------------------

/**
 * 可标注的 markdown-it token 类型白名单。
 *
 * 仅这些类型的 token 会被注入 data-block-key 并进入 BlockMap。
 * bullet_list_open / ordered_list_open 不在内（用户标注 list_item 而非整个 list）。
 */
const ANNOTATABLE_TYPES = new Set([
  'paragraph_open',
  'heading_open',
  'list_item_open',
  'blockquote_open',
  'table_open',
  'hr',
  'fence',
  'code_block',
]);

// ---------------------------------------------------------------------------
// markdown-it 实例（单例，无状态可复用）
// ---------------------------------------------------------------------------

/**
 * markdown-it 实例。
 *
 * 配置：
 * - `html: true` 允许原始 HTML 标签（文献 MD 常含 `<div>`/`<table>` 等，iframe 已 sandbox 隔离）
 * - `linkify: true` 自动识别链接
 * - `typographer: true` 排版优化（引号/破折号）
 * - `highlight` 代码高亮 via highlight.js common 子集
 * - texmath 插件：KaTeX 渲染 `$...$`（inline）与 `$$...$$`（display）数学公式
 */
const md = new MarkdownIt({
  html: true,
  linkify: true,
  typographer: true,
  highlight(str: string, lang: string): string {
    if (lang && hljs.getLanguage(lang)) {
      try {
        return (
          '<pre><code class="hljs language-' +
          lang +
          '">' +
          hljs.highlight(str, { language: lang }).value +
          '</code></pre>'
        );
      } catch {
        // fallthrough to default
      }
    }
    return ''; // 空串 → markdown-it 用默认 escape
  },
}).use(texmath, { engine: katexEngine, delimiters: 'dollars' });

// ── 图片 renderer hook（通过 env 传入 assetMap，零正则）──

const defaultImageRender =
  md.renderer.rules.image ??
  ((tokens, idx, options, _env, self) => self.renderToken(tokens, idx, options));

md.renderer.rules.image = (tokens, idx, options, env, self) => {
  const token = tokens[idx];
  // markdown-it 15 的 attrGet 可能返回 string | number | null，统一转为 string
  const srcRaw = token.attrGet('src');
  const src = srcRaw != null ? String(srcRaw) : null;
  const assetMap = (env as RenderEnv)?.assetMap ?? new Map<string, string>();

  if (src) {
    if (assetMap.has(src)) {
      // 命中：替换为 data URL
      token.attrSet('src', assetMap.get(src)!);
    } else if (!isRemoteUrl(src) && !src.startsWith('data:')) {
      // 未命中的本地相对路径：标记失败，CSS 显示占位
      token.attrSet('data-failed-src', src);
    }
  }
  return defaultImageRender(tokens, idx, options, env, self);
};

// ── HTML <img> 标签正则 ──────────────────────────────────────────────
//
// `html: true` 下 OCR 产物常含 HTML `<img>` 标签（如 PaddleOCR 输出），
// markdown-it 将其解析为 html_inline / html_block token，而非 image token，
// 上面的 renderer hook 无法覆盖。此处用正则补充收集与替换。
//
// 捕获组：
//   1. prefix — `<img ... src=` (含属性前缀)
//   2. quote — 引号字符 `"` 或 `'`
//   3. src — 图片路径
//   \2 — 反向引用确保开闭引号一致

const IMG_SRC_REGEX = /(<img\b[^>]*?\bsrc\s*=\s*)(["'])([^"']+)\2/gi;

// ---------------------------------------------------------------------------
// 渲染入口
// ---------------------------------------------------------------------------

/**
 * 渲染 MD 为 HTML + BlockMap。
 *
 * 流程：
 *   1. `normalizeMathDelimiters()` 归一化 `$ content $` → `$content$`
 *   2. `md.parse()` 生成 token 树
 *   3. `buildBlockMap()` 遍历 token 注入 attrs + 构建 BlockMap
 *   4. `md.renderer.render()` 输出 HTML（图片 hook 自动替换 src）
 *
 * @param mdText MD 原文
 * @param assetMap 图片资源映射（相对路径 → data URL），由后端 resolve_assets 返回
 */
export function renderMarkdown(
  mdText: string,
  assetMap: Map<string, string>,
): RenderResult {
  const normalized = normalizeMathDelimiters(mdText);
  const env: RenderEnv = { assetMap };
  const tokens = md.parse(normalized, env);
  const blockMap = buildBlockMap(tokens);
  let html = md.renderer.render(tokens, md.options, env);
  // HTML <img> 标签的 src 替换（image renderer hook 仅覆盖 MD 语法图片）
  html = replaceHtmlImgSrcs(html, assetMap);
  return { html, blockMap };
}

/**
 * 从 MD 文本收集所有需要后端解析的图片 src（相对路径）。
 *
 * 两个来源：
 *   1. markdown-it token 树中的 `image` token（`![](path)` 语法）
 *   2. 原始文本中的 HTML `<img src="path">` 标签（`html: true` 下 OCR 产物常见）
 *
 * 在渲染前调用，批量发给后端解析为 data URL。
 */
export function collectImageSrcs(mdText: string): string[] {
  const tokens = md.parse(mdText, {});
  const paths = new Set<string>();
  collectImageSrcsFromTokens(tokens, paths);
  collectImageSrcsFromHtml(mdText, paths);
  return [...paths];
}

/**
 * 收集 token 树中的图片 src（递归）。
 *
 * 覆盖：inline `![](path)`、reference-style `![][ref]`（markdown-it 已解析为 src）、
 * 带 title `![](path "title")`。
 */
function collectImageSrcsFromTokens(tokens: Token[], paths: Set<string>): void {
  for (const t of tokens) {
    if (t.type === 'image') {
      const srcRaw = t.attrGet('src');
      const src = srcRaw != null ? String(srcRaw) : null;
      if (src && !isRemoteUrl(src) && !src.startsWith('data:')) {
        paths.add(src);
      }
    }
    // 图片在 inline tokens 中，递归 children
    if (t.children && t.children.length > 0) {
      collectImageSrcsFromTokens(t.children, paths);
    }
  }
}

/**
 * 正则收集 HTML `<img>` 标签的 src。
 *
 * `html: true` 下 `<img>` 标签解析为 html_inline / html_block token，
 * 不被 image renderer hook 覆盖，需正则补充收集。
 */
function collectImageSrcsFromHtml(mdText: string, paths: Set<string>): void {
  for (const match of mdText.matchAll(IMG_SRC_REGEX)) {
    const src = match[3];
    if (src && !isRemoteUrl(src) && !src.startsWith('data:')) {
      paths.add(src);
    }
  }
}

/**
 * 替换渲染后 HTML 中 `<img>` 标签的 src 为 data URL。
 *
 * image renderer hook 仅覆盖 MD `![](...)` 语法图片；
 * `html: true` 下 `<img>` 标签原样输出，需后处理替换。
 * 未命中的本地路径添加 `data-failed-src` 标记，与 MD 语法图片行为一致。
 */
function replaceHtmlImgSrcs(html: string, assetMap: Map<string, string>): string {
  return html.replace(IMG_SRC_REGEX, (match, prefix: string, quote: string, src: string) => {
    if (assetMap.has(src)) {
      return `${prefix}${quote}${assetMap.get(src)}${quote}`;
    }
    if (!isRemoteUrl(src) && !src.startsWith('data:')) {
      return `${match} data-failed-src=${quote}${src}${quote}`;
    }
    return match;
  });
}

// ---------------------------------------------------------------------------
// BlockMap 构建
// ---------------------------------------------------------------------------

/**
 * 遍历 token 树，为可标注块注入 attrs 并构建 BlockMap。
 *
 * - 注入 `data-block-key="{type}:{line}:{occurrence}"` 到 token attrs
 * - 注入 `data-line="{sourceLine}"` 到 token attrs
 * - 计算 fingerprint（hash + prefix + suffix）
 *
 * occurrence 计数：同 (type, line) 的出现序号，解决同行多 block 冲突。
 */
function buildBlockMap(tokens: Token[]): BlockMap {
  const entries: BlockMapEntry[] = [];
  const byKey = new Map<string, BlockMapEntry>();
  const byFingerprint = new Map<string, BlockMapEntry[]>();
  const occurrenceCounter = new Map<string, number>();

  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i];
    if (!ANNOTATABLE_TYPES.has(token.type)) continue;
    if (!token.map) continue;

    const blockType = mapTokenType(token.type);
    if (blockType === 'other') continue;

    const sourceLine = token.map[0];
    const occKey = `${blockType}:${sourceLine}`;
    const occurrence = occurrenceCounter.get(occKey) ?? 0;
    occurrenceCounter.set(occKey, occurrence + 1);

    const blockKey = `${blockType}:${sourceLine}:${occurrence}`;
    const text = extractBlockText(tokens, i);
    const fingerprint = makeFingerprint(text);
    const selector = `[data-block-key='${blockKey}']`;

    // 注入 attrs（markdown-it 默认 renderer 会输出所有 attrs）
    token.attrSet('data-block-key', blockKey);
    token.attrSet('data-line', String(sourceLine));

    const entry: BlockMapEntry = {
      key: blockKey,
      block_type: blockType,
      source_line: sourceLine,
      occurrence,
      text,
      fingerprint,
      selector,
    };
    entries.push(entry);
    byKey.set(blockKey, entry);
    const fpList = byFingerprint.get(fingerprint.hash) ?? [];
    fpList.push(entry);
    byFingerprint.set(fingerprint.hash, fpList);
  }

  return { entries, byKey, byFingerprint };
}

/**
 * 将 markdown-it token type 映射到 BlockType。
 */
function mapTokenType(type: string): BlockType {
  switch (type) {
    case 'paragraph_open':
      return 'paragraph';
    case 'heading_open':
      return 'heading';
    case 'list_item_open':
      return 'list_item';
    case 'blockquote_open':
      return 'block_quote';
    case 'table_open':
      return 'table';
    case 'hr':
      return 'hr';
    case 'fence':
    case 'code_block':
      return 'code_block';
    default:
      return 'other';
  }
}

/**
 * 提取块级元素的纯文本内容。
 *
 * - 自闭合 token（hr/fence/code_block）：直接取 content
 * - open/close 对：收集到匹配 close 之间的所有 inline children 文本
 */
function extractBlockText(tokens: Token[], openIdx: number): string {
  const token = tokens[openIdx];

  // 自闭合 token
  if (token.type === 'hr') return '';
  if (token.type === 'fence' || token.type === 'code_block') return token.content;

  // open/close 对：收集到匹配 close
  const closeType = token.type.replace('_open', '_close');
  let text = '';
  let depth = 0;
  for (let i = openIdx + 1; i < tokens.length; i++) {
    const t = tokens[i];
    if (t.type === token.type) {
      depth++;
    } else if (t.type === closeType) {
      if (depth === 0) break;
      depth--;
    }

    if (t.type === 'inline' && t.children) {
      // html: true 下 inline children 可能含 html_inline token（原始 HTML 标签），
      // 跳过它们以获得纯文本指纹（避免标签属性变化导致 hash 不稳定）。
      text += t.children
        .filter((c) => c.type !== 'html_inline')
        .map((c) => c.content)
        .join('');
    } else if (t.type === 'fence' || t.type === 'code_block') {
      text += t.content;
    }
  }
  return text;
}

// ---------------------------------------------------------------------------
// 指纹计算
// ---------------------------------------------------------------------------

/**
 * 生成块内容指纹。
 *
 * - hash：cyrb53（64 位非密码学 hash），16 hex，碰撞率足够低
 * - prefix/suffix：首尾 32 字符，可读快照，用于迁移搜索
 *
 * 归一化：空白合并为单空格并 trim，避免无关空白变化导致 hash 不一致。
 */
function makeFingerprint(text: string): Fingerprint {
  const normalized = text.replace(/\s+/g, ' ').trim();
  const hash = cyrb53(normalized);
  const prefix = normalized.slice(0, 32);
  const suffix = normalized.slice(-32);
  return { hash, prefix, suffix };
}

/**
 * cyrb53 —— 64 位非密码学 hash 函数。
 *
 * 输出 16 位 hex 字符串。碰撞率远低于 FNV-1a，分布均匀。
 * 仅用作内容指纹（校验/迁移依据），非密码学用途。
 */
function cyrb53(str: string, seed = 0): string {
  let h1 = 0xdeadbeef ^ seed;
  let h2 = 0x41c6ce57 ^ seed;
  for (let i = 0; i < str.length; i++) {
    const ch = str.charCodeAt(i);
    h1 = Math.imul(h1 ^ ch, 2654435761);
    h2 = Math.imul(h2 ^ ch, 1597334677);
  }
  h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909);
  h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909);
  return (
    (h2 >>> 0).toString(16).padStart(8, '0') +
    (h1 >>> 0).toString(16).padStart(8, '0')
  );
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/**
 * 归一化 inline 数学公式分隔符。
 *
 * PDF 转 MD 工具常生成 `$ \alpha $`（$ 与内容间有空格），
 * 而 texmath 的 dollars 模式要求 `$` 紧跟非空白字符，否则不识别为公式。
 * 此函数将 `$ content $`（首尾各至少一个空格）归一化为 `$content$`。
 *
 * 仅处理单 `$` 分隔的 inline math：
 * - `$$...$$`（display math）不受影响（texmath 的 math_inline_double 已允许空格）
 * - `$content$`（无空格）不受影响（已被 texmath 正常识别）
 *
 * 不跨行处理（内容不含换行符），不影响 BlockMap 的 source_line 映射。
 */
function normalizeMathDelimiters(mdText: string): string {
  // (^|[^$\n])  捕获 $ 前面的字符（行首 ^ 或非 $ 非换行字符），替换时保留
  // \$\s+       $ 后跟 1+ 空格
  // ([^$\n]+?)  公式内容（惰性匹配，不含 $ 和换行）
  // \s+\$       1+ 空格后跟 $
  // (?!\$)      确保 $ 后不是另一个 $（不破坏 $$）
  return mdText.replace(
    /(^|[^$\n])\$\s+([^$\n]+?)\s+\$(?!\$)/gm,
    (_match, prefix: string, content: string) => `${prefix}$${content}$`,
  );
}

/** 判断是否为远程/嵌入式 URL（无需本地解析）。 */
function isRemoteUrl(s: string): boolean {
  return (
    s.startsWith('http://') ||
    s.startsWith('https://') ||
    s.startsWith('data:') ||
    s.startsWith('asset:') ||
    s.startsWith('blob:') ||
    s.startsWith('file://')
  );
}
