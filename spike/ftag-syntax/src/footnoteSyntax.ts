// Fluen 脚注语法识别 spike —— CodeMirror 6 / @lezer/markdown 扩展
// 路径选型：MarkdownConfig（defineNodes + parseBlock + parseInline）
// 验证目标：识别 [^id] 行内引用 + [^id]: 定义块（含 4 空格/tab 缩进的多段落续行）

import {
  MarkdownConfig,
  BlockContext,
  Line,
  BlockParser,
  InlineParser,
  InlineContext,
} from '@lezer/markdown';

/**
 * 脚注 id side-table（沿用 ftagSyntax.ts 的 attrsTable 模式）。
 *
 * 坑点：MarkdownConfig 路径下 Element 不支持 per-node props（见 ftagSyntax
 * 坑点 3.1），故用模块级 Map 以节点 from 为 key 存 id。生产代码需重新设计。
 */
const footnoteDefIdTable = new Map<number, string>();
const footnoteRefIdTable = new Map<number, string>();

export function getFootnoteDefId(from: number): string | undefined {
  return footnoteDefIdTable.get(from);
}

export function getFootnoteRefId(from: number): string | undefined {
  return footnoteRefIdTable.get(from);
}

export function clearFootnoteAttrs(): void {
  footnoteDefIdTable.clear();
  footnoteRefIdTable.clear();
}

/**
 * 续行判定：行首是 tab 或 4+ 空格。
 * spike 限制：仅识别 top-level 缩进，不处理 blockquote/list 等 composite
 * 上下文里的相对缩进（spec 允许降级）。
 */
function isContinuation(text: string): boolean {
  return /^\t|^    /.test(text);
}

// ===== SubTask 4.1 + 4.2: 定义块识别（单行 + 多段落续行）=====

const footnoteDefBlockParser: BlockParser = {
  name: 'FootnoteDefinition',
  before: 'LinkReference', // [^id]: 形似 [ref]:，抢在 LinkReference 之前认领
  parse(cx: BlockContext, line: Line): boolean {
    const rest = line.text.slice(line.pos);
    // 匹配 [^id]: 后紧跟空格或 tab（spec 规范要求冒号后有空格）
    const m = /^\[\^([^\]]+)\]:[\t ]/.exec(rest);
    if (!m) return false;

    const id = m[1];
    const from = cx.lineStart + line.pos;
    footnoteDefIdTable.set(from, id);

    // 消费第一行（定义起始行）
    cx.nextLine();
    let to = cx.prevLineEnd(); // 第一行内容末尾（不含 \n）

    // 扫描续行：4 空格/tab 缩进的段落归入同一脚注定义块。
    // 策略：检查当前行（line.text，已由 nextLine 读入）。
    //  - 当前行空：peek 下一行；若缩进，消费"当前空行 + 缩进行"；否则停止。
    //  - 当前行缩进：直接消费。
    //  - 当前行非空非缩进：停止，不消费（让出给后续 block parser）。
    while (true) {
      if (line.text.trim() === '') {
        // 当前行空。peek 下一行决定是否续行。
        const peeked = cx.peekLine();
        if (peeked === '' || !isContinuation(peeked)) {
          // 下一行空/EOF/非缩进：停止。不消费当前空行（让出给后续 parser）。
          break;
        }
        // 下一行缩进：消费"当前空行 + 缩进行"
        cx.nextLine(); // 消费空行，前进到缩进行
        cx.nextLine(); // 消费缩进行，前进到其后
        to = cx.prevLineEnd(); // 缩进行末尾
        continue;
      }

      if (isContinuation(line.text)) {
        // 当前行缩进：直接消费
        cx.nextLine();
        to = cx.prevLineEnd();
        continue;
      }

      // 当前行非空非缩进：停止，不消费
      break;
    }

    cx.addElement(cx.elt('FootnoteDefinition', from, to, []));
    return true;
  },
};

// ===== SubTask 4.1: 行内引用识别 =====

const footnoteRefInlineParser: InlineParser = {
  name: 'FootnoteRef',
  before: 'Link', // [^id] 形似 [text]，抢在标准 Link parser 之前认领
  parse(cx: InlineContext, next: number, pos: number): number {
    if (next !== 91 /* '[' */) return -1;
    if (cx.char(pos + 1) !== 94 /* '^' */) return -1;
    // 找闭合 ]，不允许跨行
    let end = pos + 2;
    while (end < cx.end) {
      const c = cx.char(end);
      if (c === 93 /* ']' */) break;
      if (c === 10 /* \n */ || c === 13 /* \r */) return -1;
      end++;
    }
    if (end >= cx.end || cx.char(end) !== 93) return -1;
    const id = cx.slice(pos + 2, end);
    if (!id) return -1; // [^] 空 id 不认领
    footnoteRefIdTable.set(pos, id);
    // 区间仅覆盖 [^id] 本身（pos 到 end+1，end 是 ] 的位置）
    return cx.addElement(cx.elt('FootnoteRef', pos, end + 1));
  },
};

export const footnoteExtension: MarkdownConfig = {
  defineNodes: [
    { name: 'FootnoteDefinition', block: true },
    { name: 'FootnoteRef' }, // 行内节点，非 block
  ],
  parseBlock: [footnoteDefBlockParser],
  parseInline: [footnoteRefInlineParser],
};
