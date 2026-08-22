/**
 * 大纲结构操作的本地事务纯函数层。
 *
 * 计算对编辑器文档的编辑片段（`ChangeSpec`），由调用方 dispatch 进 CM6 历史，
 * 从而支持 Ctrl+Z 撤销，且不依赖后端旧副本。所有函数不操作 DOM / 不调用 IPC。
 *
 * 结构操作的编辑锚点使用增量维护的 `line` + 文本校验双保险，防止失步错改。
 */

import type { ChangeSpec } from '@codemirror/state';

/** 结构操作结果。`error` 非空时操作失败，`changes` 为待 dispatch 的编辑片段。 */
export interface HeadingEditResult {
  changes?: ChangeSpec[];
  error?: string;
}

/** 标题行：1-6 个 `#` 后跟空白与文本。 */
const HEADING_RE = /^(#{1,6})\s+(.+)$/;

/** 代码围栏开关行。 */
const FENCE_RE = /^(`{3,}|~{3,})/;

/** 计算某一行的起始文档偏移（0-based 行号 → 该行首字符位置）。 */
function lineOffset(lines: string[], line: number): number {
  let offset = 0;
  for (let i = 0; i < line; i++) offset += lines[i].length + 1;
  return offset;
}

/** 校验标题输入：返回错误信息，合法时返回 null。 */
function titleError(title: string): string | null {
  const t = title.trim();
  if (!t) return '标题不能为空';
  if (t.includes('\n')) return '标题不能包含换行';
  return null;
}

/**
 * 重命名标题行。
 *
 * 校验目标行仍是 `level` + `text` 对应的标题（防御编辑器失步），否则返回错误、
 * 绝不静默错改。成功后返回对该行整行的替换片段。
 *
 * @param doc    当前文档全文。
 * @param anchor 目标标题（`line` / `level` / `text`，来自增量维护的扁平列表）。
 * @param newText 新标题文本。
 */
export function renameHeadingAt(
  doc: string,
  anchor: { line: number; level: number; text: string },
  newText: string,
): HeadingEditResult {
  const err = titleError(newText);
  if (err) return { error: err };
  const title = newText.trim();

  const lines = doc.split('\n');
  if (anchor.line < 0 || anchor.line >= lines.length) {
    return { error: '标题行号越界，请刷新大纲' };
  }

  // 重新解析该行，确认其仍是同样的标题（避免基于过期锚点覆盖）。
  const match = lines[anchor.line].match(HEADING_RE);
  if (!match || match[1].length !== anchor.level || match[2].trim() !== anchor.text) {
    return { error: '原标题已变化，请刷新后重试' };
  }

  const from = lineOffset(lines, anchor.line);
  const to = from + lines[anchor.line].length;
  return { changes: [{ from, to, insert: '#'.repeat(anchor.level) + ' ' + title }] };
}

/**
 * 在父节点作用域末尾插入子标题（不生成章节标记，归属父 section）。
 *
 * 定位父作用域末尾 = 下一个 `level <= parent.level` 的真实标题行之前（或文档尾），
 * 期间跳过代码围栏内的内容。插入 `childLevel 标题行 + 空行`。
 *
 * @param doc    当前文档全文。
 * @param parent 父标题锚点（`line` / `level`）。
 * @param title  子标题文本。
 */
export function insertChildAt(
  doc: string,
  parent: { line: number; level: number },
  title: string,
): HeadingEditResult {
  const textErr = titleError(title);
  if (textErr) return { error: textErr };
  if (parent.level >= 6) return { error: '已达到最大标题层级（H6）' };

  const childLevel = parent.level + 1;
  const heading = '#'.repeat(childLevel) + ' ' + title.trim();

  const lines = doc.split('\n');
  if (parent.line < 0 || parent.line >= lines.length) {
    return { error: '父标题行号越界，请刷新大纲' };
  }

  // 向后扫描，找到下一个不在围栏内的同层或更浅标题作为插入边界。
  let inFence = false;
  let siblingLine = -1;
  for (let i = parent.line + 1; i < lines.length; i++) {
    const line = lines[i];
    if (FENCE_RE.test(line.trim())) {
      inFence = !inFence;
      continue;
    }
    if (inFence) continue;
    const m = line.match(/^(#{1,6})\s+/);
    if (m && m[1].length <= parent.level) {
      siblingLine = i;
      break;
    }
  }

  if (siblingLine >= 0) {
    // 在兄弟标题行首插入 "\n子标题\n\n"，形成：内容空行、子标题、空行、兄弟标题。
    const from = lineOffset(lines, siblingLine);
    return { changes: [{ from, to: from, insert: `\n${heading}\n\n` }] };
  }

  // 无兄弟标题 → 追加到文档末尾。
  const insert = doc.length === 0
    ? `${heading}\n`
    : (doc.endsWith('\n') ? `\n${heading}\n` : `\n\n${heading}\n`);
  return { changes: [{ from: doc.length, to: doc.length, insert }] };
}