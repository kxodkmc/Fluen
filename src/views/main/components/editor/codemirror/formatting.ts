/**
 * Markdown 行内格式命令（纯函数层）。
 *
 * 不依赖 CodeMirror / Vue：输入文档文本与选区，输出格式化的文档变更
 * （changes）与变化后文档中的选区位置，便于单元测试、复用与后续扩展
 * （如快捷键、更多格式）。
 *
 * 行为模型（与主流 Markdown 编辑器一致，覆盖工具栏三态交互）：
 *
 *   - 有选区：将选区内同类标记剥离后整体包裹；若选区恰好整体被同类
 *     标记包裹，则取消包裹。例如对 `我是天之骄子` 选中 `是天` 加粗 →
 *     `我**是天**之骄子`；再框选 `我**是天**之` 加粗（内部 `**` 被剥离
 *     后整体包裹）→ `**我是天之**骄子`。
 *   - 选区与既有标记对相邻/重叠时自动合并：若选区紧贴或吞掉了同类标记
 *     对的一侧，则把整个标记对并入本次包裹范围，保证结果标记始终成对、
 *     连续。例如对 `你好我是**Kimi**` 选中 `是` 加粗 → `你好我**是Kimi**`
 *     （而非 `你好我**是****Kimi**`）；选中 `是**K` 加粗 → 同样
 *     `你好我**是Kimi**`。
 *   - 无选区（光标）：若光标位于同类标记对内，移除该对标记；否则插入
 *     空的标记对并将光标置于中间。例如光标处于 `**我是天之**` 内部时
 *     点击加粗 → 取消加粗。
 *   - heading：对选区覆盖的所有行（空选区 = 光标所在行）统一设为目标
 *     级别（1-6 级）前缀；所有行均已处于目标级别时切换为移除前缀。
 */

export type MarkdownFormatKind = 'bold' | 'italic' | 'underline' | 'heading';

/** 标题级别（1-6 级，对应 Markdown `#` 至 `######`）。 */
export type HeadingLevel = 1 | 2 | 3 | 4 | 5 | 6;

/** 单条文档变更（与 CodeMirror ChangeSpec 兼容的子集）。 */
export interface DocChange {
  from: number;
  to: number;
  insert: string;
}

/** 格式化结果：文档变更 + 变化后文档中的光标/选区位置。 */
export interface FormatResult {
  changes: DocChange[];
  selection: { anchor: number; head: number };
}

/** 行内格式的包裹标记。heading 使用行前缀而非包裹标记。 */
const MARKERS: Record<Exclude<MarkdownFormatKind, 'heading'>, string> = {
  bold: '**',
  italic: '*',
  underline: '++',
};

/** 行首标题前缀：可选前导空白 + 1-6 个 `#` + 空格/制表符或行尾，避免误伤 `#tag` 等文本。 */
const HEADING_RE = /^(\s*)(#{1,6})(?=\s|$)/;

/* ── 标记查找 ─────────────────────────────────────────────────────── */

/**
 * 在 [0, pos) 内从后向前查找最近的 marker。
 *
 * 对斜体（`*`）自动跳过 `**` 序列，避免把加粗标记误判为斜体标记；
 * 同时跳过跨过 pos 的标记（如 `****` 中位置 1 的 `**` 越过光标 2），
 * 保证返回的标记完整位于光标左侧。
 *
 * @returns 命中位置；未找到返回 -1。
 */
function findMarkerBefore(doc: string, pos: number, marker: string): number {
  for (let i = pos - 1; i >= 0; i--) {
    if (doc.startsWith(marker, i)) {
      if (marker === '*' && doc[i + 1] === '*') {
        i -= 1; // 与循环步进合计跳过 `**` 两个字符
        continue;
      }
      if (i + marker.length > pos) {
        continue; // 该标记跨过光标，跳过此候选（循环步进后检查 i-1）
      }
      return i;
    }
  }
  return -1;
}

/**
 * 在 [pos, len) 内从前往后查找最近的 marker。
 *
 * 规则同 {@link findMarkerBefore}。
 *
 * @returns 命中位置；未找到返回 -1。
 */
function findMarkerAfter(doc: string, pos: number, marker: string): number {
  for (let i = pos; i < doc.length; i++) {
    if (doc.startsWith(marker, i)) {
      if (marker === '*' && doc[i + 1] === '*') {
        i += 1; // 与循环步进合计跳过 `**` 两个字符
        continue;
      }
      return i;
    }
  }
  return -1;
}

/**
 * 统计 [0, pos) 内完整出现的同类标记数量（用于判断该位置是否“成对平衡”）。
 *
 * 对斜体（`*`）自动跳过 `**` 序列，统计口径与 {@link findMarkerBefore} 一致。
 * 若计数为奇数，说明存在一个尚未闭合的同类标记跨越 pos —— 其闭合标记必在
 * pos 之后；反之 pos 位于普通文本边界，标记结构完整。
 *
 * @returns 完整标记的数量。
 */
function markerBalanceBefore(doc: string, pos: number, marker: string): number {
  const m = marker.length;
  let count = 0;
  for (let i = 0; i + m <= pos; i++) {
    if (doc.startsWith(marker, i)) {
      if (marker === '*' && doc[i + 1] === '*') {
        i += 1; // 与循环步进合计跳过 `**` 整体
        continue;
      }
      count++;
    }
  }
  return count;
}

/**
 * 剥离文本中的同类标记。
 *
 *   - 通用（`**` / `++` 等）：移除全部标记序列。
 *   - italic（`*`）：仅移除独立的 `*`（不属于 `**` 的部分），保留加粗标记。
 */
function stripMarkers(text: string, marker: string): string {
  if (marker !== '*') {
    return text.split(marker).join('');
  }
  let out = '';
  for (let i = 0; i < text.length; i++) {
    if (text[i] === '*') {
      if (text[i + 1] === '*') {
        out += '**';
        i += 1; // 保留 `**` 整体
      }
      // 独立 `*` 被剥离
    } else {
      out += text[i];
    }
  }
  return out;
}

/* ── 位置映射 ─────────────────────────────────────────────────────── */

/**
 * 将原文档中的位置映射到应用 changes（按 from 升序）后的文档。
 *
 * 遵循 CodeMirror 默认 assoc=1 语义：位置恰在插入点/替换区起点时吸附到
 * 插入文本之后。
 *   - 删除/替换（to > from）：位置在删除区之后整体偏移；落在删除区内
 *     （含起点）时映射到插入文本末尾。
 *   - 纯插入（to === from）：位置在插入点或之后时整体偏移。
 */
function shiftPosition(pos: number, changes: DocChange[]): number {
  let p = pos;
  for (let i = changes.length - 1; i >= 0; i--) {
    const { from, to, insert } = changes[i];
    if (to > from) {
      const delta = insert.length - (to - from);
      if (p >= to) {
        p += delta;
      } else if (p >= from) {
        p = from + insert.length;
      }
    } else if (p >= from) {
      p += insert.length;
    }
  }
  return p;
}

/**
 * 判断文本内部是否还存在同类标记。
 *
 *   - 通用（`**` / `++` 等）：是否存在任意标记序列。
 *   - italic（`*`）：是否存在独立 `*`（`**` 视为加粗标记整体跳过）。
 * 用于“选区整体被包裹”判定：首尾为标记且内部无同类标记才算包裹，
 * 避免 `甲**乙**丙**丁**` 这类首尾恰好是标记的选区被误判为取消包裹。
 */
function hasInnerMarker(text: string, marker: string): boolean {
  if (marker !== '*') {
    return text.includes(marker);
  }
  for (let i = 0; i < text.length; i++) {
    if (text[i] === '*') {
      if (text[i + 1] === '*') {
        i += 1; // 跳过 `**` 整体
      } else {
        return true;
      }
    }
  }
  return false;
}

/* ── 行内格式（加粗 / 斜体 / 下划线） ─────────────────────────────── */

function applyInline(doc: string, from: number, to: number, marker: string): FormatResult {
  const m = marker.length;

  // ── 有选区：剥离选区内同类标记后整体包裹；整体已被包裹则取消 ──
  if (from !== to) {
    const selText = doc.slice(from, to);
    // 斜体的“整体被包裹”要求首尾是独立 `*`（不属于 `**` 加粗标记），
    // 避免把 `**bold**` 全选点斜体误判为取消斜体包裹；
    // 且选区内部不得再含同类标记（见 {@link hasInnerMarker}）。
    const wrapped =
      selText.length >= m * 2 &&
      selText.startsWith(marker) &&
      selText.endsWith(marker) &&
      (marker !== '*' || (selText[1] !== '*' && selText[selText.length - 2] !== '*')) &&
      !hasInnerMarker(selText.slice(m, selText.length - m), marker);

    if (wrapped) {
      // 取消包裹：删除首尾标记，选区收缩到内部文本（不含标记）
      const changes: DocChange[] = [
        { from, to: from + m, insert: '' },
        { from: to - m, to, insert: '' },
      ];
      return {
        changes,
        selection: { anchor: shiftPosition(from, changes), head: shiftPosition(to, changes) },
      };
    }

    // 计算包裹范围 [wrapFrom, wrapTo)。若选区与既有同类标记对相邻或重叠，
    // 则扩展范围将其整体并入，避免产生碎片化（`**是****Kimi**`）或
    // 不对称（`**是K**imi**`）的标记结果。
    let wrapFrom = from;
    let wrapTo = to;

    // 右侧扩展：选区终点紧贴/吞掉开启标记（或存在跨越选区终点的未闭合
    // 标记）时，吸收到该标记对的闭合标记末尾。
    if (markerBalanceBefore(doc, to, marker) % 2 === 1) {
      const closeAt = findMarkerAfter(doc, to, marker);
      if (closeAt >= 0) wrapTo = closeAt + m;
    } else if (doc.startsWith(marker, to)) {
      const closeAt = findMarkerAfter(doc, to + m, marker);
      if (closeAt >= 0) wrapTo = closeAt + m;
    }

    // 左侧扩展：选区起点紧贴/吞掉闭合标记（或存在跨越选区起点的未闭合
    // 标记）时，回溯到该标记对的开启标记。
    if (markerBalanceBefore(doc, from, marker) % 2 === 1) {
      const openAt = findMarkerBefore(doc, from, marker);
      if (openAt >= 0) wrapFrom = openAt;
    } else if (from >= m && doc.slice(from - m, from) === marker) {
      const openAt = findMarkerBefore(doc, from - m, marker);
      if (openAt >= 0) wrapFrom = openAt;
    }

    // 剥离整个包裹范围内的同类标记后整体包裹一次，选区保持选中内部文本
    // （便于连续追加格式）。
    const body = stripMarkers(doc.slice(wrapFrom, wrapTo), marker);
    return {
      changes: [{ from: wrapFrom, to: wrapTo, insert: marker + body + marker }],
      selection: { anchor: wrapFrom + m, head: wrapFrom + m + body.length },
    };
  }

  // 无选区（光标）：位于标记对内 → 取消；否则插入空标记对。
  // findMarkerBefore 已保证返回的标记完整位于光标左侧（不跨过光标），
  // 因此光标不可能落在开标记字符内，删除段不会相互重叠。
  const head = from;
  const openAt = findMarkerBefore(doc, head, marker);
  const closeAt = findMarkerAfter(doc, head, marker);

  if (openAt >= 0 && closeAt >= 0 && head <= closeAt) {
    const changes: DocChange[] = [
      { from: openAt, to: openAt + m, insert: '' },
      { from: closeAt, to: closeAt + m, insert: '' },
    ];
    const pos = shiftPosition(head, changes);
    return { changes, selection: { anchor: pos, head: pos } };
  }

  return {
    changes: [{ from: head, to: head, insert: marker + marker }],
    selection: { anchor: head + m, head: head + m },
  };
}

/* ── 标题 ─────────────────────────────────────────────────────────── */

/**
 * 检测行文本的标题级别（前缀 `#` 个数）；非标题行返回 0。
 */
function headingLevelOf(text: string): number {
  return HEADING_RE.exec(text)?.[2].length ?? 0;
}

/**
 * 收集 [from, to) 覆盖的每一行（空选区 = 光标所在行）的起始位置与文本。
 */
function collectLines(doc: string, from: number, to: number): { start: number; text: string }[] {
  const lines: { start: number; text: string }[] = [];
  const isCaretOnly = from === to;
  const lastPos = isCaretOnly ? from : to - 1; // 选区内的最后一个字符位置

  let start = doc.lastIndexOf('\n', from - 1) + 1;
  let nl = doc.indexOf('\n', start);
  let end = nl === -1 ? doc.length : nl;

  for (;;) {
    lines.push({ start, text: doc.slice(start, end) });
    // 选区最后一个字符落在当前行内（或为行尾换行符）时停止
    if (isCaretOnly || lastPos <= end) break;
    start = end + 1;
    if (start > doc.length) break;
    nl = doc.indexOf('\n', start);
    end = nl === -1 ? doc.length : nl;
  }
  return lines;
}

/**
 * 计算 `#` 前缀的结束偏移（含紧随的一个空格/制表符，行尾则不含）。
 *
 * @returns 需一并移除的定界符长度。
 */
function headingSepLength(text: string, match: RegExpMatchArray): number {
  const sepIdx = match[1].length + match[2].length;
  const ch = text[sepIdx];
  return ch === ' ' || ch === '\t' ? 1 : 0;
}

function applyHeading(doc: string, from: number, to: number, level: HeadingLevel): FormatResult {
  const target = '#'.repeat(level);
  const lines = collectLines(doc, from, to);
  // 所有行均已处于目标级别 → 本次为取消；否则统一设为目标级别
  // （已带其他级别标题的行替换为该级别，无标题的行直接添加）。
  const removeAll = lines.every((l) => headingLevelOf(l.text) === level);

  const changes: DocChange[] = [];
  for (const line of lines) {
    const m = line.text.match(HEADING_RE);
    if (removeAll && m) {
      // 移除 hashes 及紧随的一个空格/制表符，保留前导空白
      const hashStart = line.start + m[1].length;
      const removeLen = m[2].length + headingSepLength(line.text, m);
      changes.push({ from: hashStart, to: hashStart + removeLen, insert: '' });
    } else {
      // 计算替换区间：剥离既有任意级别标题前缀（若存在）后插入目标级别
      const insertAt = line.start + (m ? m[1].length : (line.text.match(/^\s*/)?.[0].length ?? 0));
      const oldLen = m ? m[2].length + headingSepLength(line.text, m) : 0;
      changes.push({ from: insertAt, to: insertAt + oldLen, insert: `${target} ` });
    }
  }

  return {
    changes,
    selection: { anchor: shiftPosition(from, changes), head: shiftPosition(to, changes) },
  };
}

/* ── 入口 ─────────────────────────────────────────────────────────── */

/**
 * 应用 Markdown 格式命令，返回文档变更与新的光标/选区位置。
 *
 * @param doc   完整文档文本。
 * @param from  选区起点（min(anchor, head)）。
 * @param to    选区终点（max(anchor, head)）；与 from 相等表示仅光标。
 * @param kind  格式类型。
 * @param level 标题级别（1-6）；仅 kind === 'heading' 时生效，默认 1。
 */
export function applyMarkdownFormat(
  doc: string,
  from: number,
  to: number,
  kind: MarkdownFormatKind,
  level: HeadingLevel = 1,
): FormatResult {
  if (kind === 'heading') {
    return applyHeading(doc, from, to, level);
  }
  return applyInline(doc, from, to, MARKERS[kind]);
}
