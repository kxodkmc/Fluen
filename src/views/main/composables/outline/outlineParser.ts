/**
 * 大纲解析纯函数层。
 *
 * 从 `main.md` 内容提取标题。解析规则：
 * - 识别 `#` ~ `######` 开头的行为 H1-H6 标题；
 * - 识别 `<!-- @sec_id:{id} -->` 标记，将后续标题关联到对应章节；
 * - 忽略代码围栏（``` / ~~~ 包裹）内的标题行；
 * - 每个标题附带行号（供编辑器跳转）、树深度（供缩进）与后代数量（供折叠）。
 *
 * 输出扁平的 [`FlatHeading`] 列表，不含 children；树形聚合能力（[`buildTree`] /
 * [`parseOutline`] / [`filterByLevel`]）作为纯函数保留，供测试与筛选场景使用。
 *
 * @example
 * ```ts
 * const flat = parseOutlineFlat(mainMd);
 * const headings = assignStableIds(prev, flat); // 见 headingIds
 * ```
 */

// ---------------------------------------------------------------------------
// 类型定义
// ---------------------------------------------------------------------------

/** 无 id 的扁平标题（解析的原始产物）。由 `assignStableIds` 补充 `id`。 */
export interface OutlineFlat {
  /** 标题层级（1-6）。 */
  level: number;
  /** 标题纯文本。 */
  text: string;
  /** 在文档中的行号（0-based）。 */
  line: number;
  /** 所属章节 ID（从最近的 `<!-- @sec_id: -->` 标记继承）。 */
  sectionId: string | null;
  /** 树深度（0-based，根为 0），用于渲染缩进。 */
  depth: number;
  /** 后代标题数量（含间接后代），>0 表示可折叠。 */
  childrenCount: number;
}

/** 带稳定 id 的扁平标题，为前端渲染/折叠的状态载体。 */
export interface FlatHeading extends OutlineFlat {
  /** 跨解析重保持稳定的节点 id（供 Vue key 与折叠集合索引）。 */
  id: string;
}

/** 树形大纲节点（保留给筛选/聚合纯函数）。 */
export interface OutlineNode {
  /** 标题层级（1-6）。 */
  level: number;
  /** 标题纯文本。 */
  text: string;
  /** 在 `main.md` 中的行号（0-based）。 */
  line: number;
  /** 所属章节 ID。 */
  sectionId: string | null;
  /** 子节点（比当前 level 更深的后续标题）。 */
  children: OutlineNode[];
}

// ---------------------------------------------------------------------------
// 正则
// ---------------------------------------------------------------------------

/** 标题行：1-6 个 `#` 后跟空白与文本。 */
const HEADING_RE = /^(#{1,6})\s+(.+)$/;

/** 章节标记：`<!-- @sec_id:xxx -->`。 */
const SEC_MARKER_RE = /<!--\s*@sec_id:(\S+)\s*-->/;

/** 代码围栏开关行。 */
const FENCE_RE = /^(`{3,}|~{3,})/;

// ---------------------------------------------------------------------------
// 解析
// ---------------------------------------------------------------------------

/**
 * 从 `main.md` 内容解析出扁平的标题列表（已按行序排列）。
 *
 * 两步扫描：先提取原始标题行，再计算每个节点的树深度与后代数量。
 */
export function parseOutlineFlat(content: string): OutlineFlat[] {
  const raw = scanRawHeadings(content);
  const n = raw.length;
  const depth = new Array<number>(n).fill(0);
  const childrenCount = new Array<number>(n).fill(0);

  // 树深度：栈中保存仍开放的祖先标题下标。
  const stack: number[] = [];
  for (let i = 0; i < n; i++) {
    while (stack.length > 0 && raw[stack[stack.length - 1]].level >= raw[i].level) {
      stack.pop();
    }
    depth[i] = stack.length;
    stack.push(i);
  }

  // 后代数量：向后扫描，直到遇到同层或更浅的标题即停止。
  for (let i = 0; i < n; i++) {
    let count = 0;
    for (let j = i + 1; j < n; j++) {
      if (raw[j].level <= raw[i].level) break;
      count++;
    }
    childrenCount[i] = count;
  }

  return raw.map((h, i) => ({ ...h, depth: depth[i], childrenCount: childrenCount[i] }));
}

/** 提取原始标题（无 depth / childrenCount）。 */
interface RawHeading {
  level: number;
  text: string;
  line: number;
  sectionId: string | null;
}

function scanRawHeadings(content: string): RawHeading[] {
  const lines = content.split('\n');
  const out: RawHeading[] = [];
  let currentSectionId: string | null = null;
  let inFence = false;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (FENCE_RE.test(line.trim())) {
      inFence = !inFence;
      continue;
    }
    if (inFence) continue;

    const marker = line.match(SEC_MARKER_RE);
    if (marker) {
      currentSectionId = marker[1];
      continue;
    }

    const heading = line.match(HEADING_RE);
    if (heading) {
      out.push({
        level: heading[1].length,
        text: heading[2].trim(),
        line: i,
        sectionId: currentSectionId,
      });
    }
  }
  return out;
}

/**
 * 将扁平的标题列表聚合为树形结构（[`OutlineNode`]）。
 *
 * 栈算法：遇到更深的标题挂到上一个更浅的标题下；遇到相同或更浅的标题弹出栈顶。
 */
export function buildTree(flatNodes: OutlineNode[]): OutlineNode[] {
  const roots: OutlineNode[] = [];
  const stack: OutlineNode[] = [];

  for (const node of flatNodes) {
    while (stack.length > 0 && stack[stack.length - 1].level >= node.level) {
      stack.pop();
    }
    if (stack.length === 0) {
      roots.push(node);
    } else {
      stack[stack.length - 1].children.push(node);
    }
    stack.push(node);
  }
  return roots;
}

/** 将扁平标题（含 depth / childrenCount）转为树形节点，丢弃渲染辅助字段。 */
function toTreeNode(flat: OutlineFlat): OutlineNode {
  return { level: flat.level, text: flat.text, line: flat.line, sectionId: flat.sectionId, children: [] };
}

/**
 * 一步完成解析 + 建树。
 *
 * 等价于 `buildTree(parseOutlineFlat(content))`。返回的树形节点不含 `depth` /
 * `childrenCount`（后者仅供扁平渲染使用）。
 */
export function parseOutline(content: string): OutlineNode[] {
  return buildTree(parseOutlineFlat(content).map(toTreeNode));
}

// ---------------------------------------------------------------------------
// 筛选（纯函数保留，供按层级筛选场景使用）
// ---------------------------------------------------------------------------

/**
 * 按层级范围筛选大纲树。
 *
 * 只保留 `minLevel` ~ `maxLevel` 层级的节点；被过滤掉的浅层节点的子节点向上
 * 提升到最近的祖先节点下；比最大层级更深的子树整体跳过。
 */
export function filterByLevel(
  nodes: OutlineNode[],
  minLevel = 1,
  maxLevel = 6,
): OutlineNode[] {
  const result: OutlineNode[] = [];

  function walk(node: OutlineNode, parent: OutlineNode | null): void {
    if (node.level < minLevel) {
      for (const child of node.children) walk(child, parent);
      return;
    }
    if (node.level > maxLevel) return;

    const filtered: OutlineNode = { ...node, children: [] };
    if (parent) parent.children.push(filtered);
    else result.push(filtered);

    for (const child of node.children) walk(child, filtered);
  }

  for (const node of nodes) walk(node, null);
  return result;
}