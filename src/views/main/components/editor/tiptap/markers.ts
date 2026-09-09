/**
 * TipTap 视图章节标记剥离/回插（纯函数层）。
 *
 * `main.md` 的章节块由 `<!-- @sec_id:{id} -->` 标记行 + H1 构成，后端保存时
 * 按标记拆分 `sec-{id}.md`。TipTap 没有评论节点，HTML 注释经 marked 解析后
 * 会被丢弃或变为可见字面文本，因此进入编辑器前先剥离标记，序列化后按
 * H1 位置回插。
 *
 * 绑定规则镜像后端 `split_main_md`（src-tauri/src/project/section.rs）：
 * 标记行进入 pending；遇到 H1 且 pending 非空时，**最后一个** pending 标记
 * 绑定到该 H1（堆叠的多余标记因所属块为空而被丢弃）；遇到其他非空行时
 * pending 全部失效。H1 判定与后端一致：`line.trimStart().startsWith('# ')`。
 *
 * 回插按 H1 位置配对：正常文档（每个标记紧邻其 H1）中第 i 个绑定标记
 * 恰好属于第 i 个 H1。数量不匹配（新增/删除章节）时返回 null，调用方
 * 降级为原文保存，由后端按标题自愈（找回旧 ID 或分配新 ID）。
 */

/** 匹配整行章节标记（与 codemirror/markerDecoration.ts 保持一致）。 */
const SEC_MARKER_RE = /^\s*<!--\s*@sec_id:(\S+)\s*-->\s*$/;

/** H1 判定（与后端 `is_h1_line` 保持一致）。 */
function isH1Line(line: string): boolean {
  return line.trimStart().startsWith('# ');
}

/** 标记剥离结果：正文（不含标记行）与按文档顺序排列的绑定标记行。 */
export interface SecMarkerState {
  /** 剥离标记行后的正文。 */
  body: string;
  /** 原始完整标记行（按绑定到的 H1 顺序）。 */
  markers: string[];
}

/**
 * 剥离 MD 文本中的章节标记行。
 *
 * @param md 完整 main.md 文本。
 */
export function extractSecMarkers(md: string): SecMarkerState {
  const lines = md.split('\n');
  const bodyLines: string[] = [];
  const markers: string[] = [];
  let pending: string[] = [];

  for (const line of lines) {
    if (SEC_MARKER_RE.test(line)) {
      pending.push(line);
      continue;
    }
    if (!line.trim()) {
      // 空行：标记的空块允许跨空行（镜像后端行为），pending 存活
      bodyLines.push(line);
      continue;
    }
    if (isH1Line(line)) {
      // 最后一个 pending 绑定到该 H1；更早的堆叠标记所属块为空，丢弃
      if (pending.length > 0) {
        markers.push(pending[pending.length - 1]);
        pending = [];
      }
    } else if (pending.length > 0) {
      // 标记后跟了非 H1 内容：全部失效（镜像后端 resolve_section_ids）
      pending = [];
    }
    bodyLines.push(line);
  }
  // EOF 处未绑定的 pending（后端视为独立空块）不参与回插，直接丢弃

  return { body: bodyLines.join('\n'), markers };
}

/**
 * 将标记按 H1 位置回插到序列化后的 MD 文本。
 *
 * @param md      TipTap 序列化得到的正文（不含标记）。
 * @param markers `extractSecMarkers` 收集的绑定标记行。
 * @returns 回插后的完整文本；H1 数量与标记数量不一致时返回 null。
 */
export function reinsertSecMarkers(md: string, markers: string[]): string | null {
  const lines = md.split('\n');
  const h1Indexes: number[] = [];
  for (let i = 0; i < lines.length; i++) {
    if (isH1Line(lines[i])) h1Indexes.push(i);
  }
  if (h1Indexes.length !== markers.length) return null;

  const out = [...lines];
  // 从后向前插入，避免索引漂移
  for (let i = markers.length - 1; i >= 0; i--) {
    out.splice(h1Indexes[i], 0, markers[i]);
  }
  return out.join('\n');
}
