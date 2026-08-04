/**
 * 大纲解析工具——从 `.temp.md` 内容中提取标题树。
 *
 * 解析规则：
 * - 识别 `#` ~ `######` 开头的行为 H1-H6 标题
 * - 识别 `<!-- @sec_id:{id} -->` 标记，将后续标题关联到对应章节
 * - 忽略代码块内的 `#` 行（``` 包裹）
 * - 每个节点记录行号，供编辑器跳转使用
 *
 * @example
 * ```ts
 * const nodes = parseOutline(tempMd);
 * const filtered = filterByLevel(nodes, 1, 3);
 * ```
 */

// ---------------------------------------------------------------------------
// 类型定义
// ---------------------------------------------------------------------------

/** 大纲节点。 */
export interface OutlineNode {
  /** 标题层级（1-6）。 */
  level: number;
  /** 标题纯文本。 */
  text: string;
  /** 在 `.temp.md` 中的行号（0-based）。 */
  line: number;
  /** 所属章节 ID（从最近的 `<!-- @sec_id: -->` 标记继承）。 */
  sectionId: string | null;
  /** 子节点（比当前 level 更深的后续标题）。 */
  children: OutlineNode[];
}

// ---------------------------------------------------------------------------
// 解析
// ---------------------------------------------------------------------------

/** 标题行正则：匹配 1-6 个 `#` 后跟空格和标题文本。 */
const HEADING_RE = /^(#{1,6})\s+(.+)$/;

/** 章节标记正则：`<!-- @sec_id:xxx -->`。 */
const SEC_MARKER_RE = /<!--\s*@sec_id:(\S+)\s*-->/;

/** 代码围栏正则。 */
const FENCE_RE = /^(`{3,}|~{3,})/;

/**
 * 从 `.temp.md` 内容中解析出扁平的标题列表。
 *
 * 返回的列表已按行序排列，每个节点不含子节点（`children` 为空）。
 * 调方可通过 [`buildTree`] 将其组装为树形结构。
 */
export function parseOutlineFlat(content: string): OutlineNode[] {
  const lines = content.split('\n');
  const nodes: OutlineNode[] = [];
  let currentSectionId: string | null = null;
  let inFence = false;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    // 代码围栏开关
    if (FENCE_RE.test(line.trim())) {
      inFence = !inFence;
      continue;
    }
    if (inFence) continue;

    // 章节标记
    const markerMatch = line.match(SEC_MARKER_RE);
    if (markerMatch) {
      currentSectionId = markerMatch[1];
      continue;
    }

    // 标题
    const headingMatch = line.match(HEADING_RE);
    if (headingMatch) {
      const level = headingMatch[1].length;
      const text = headingMatch[2].trim();
      nodes.push({
        level,
        text,
        line: i,
        sectionId: currentSectionId,
        children: [],
      });
    }
  }

  return nodes;
}

/**
 * 将扁平的标题列表组装为树形结构。
 *
 * 使用栈算法：遇到更深的标题时挂到上一个更浅的标题下，
 * 遇到相同或更浅的标题时弹出栈顶。
 */
export function buildTree(flatNodes: OutlineNode[]): OutlineNode[] {
  const roots: OutlineNode[] = [];
  const stack: OutlineNode[] = [];

  for (const node of flatNodes) {
    // 弹出栈顶直到找到比当前节点更浅的层级
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

/**
 * 一步完成解析 + 建树。
 *
 * 等价于 `buildTree(parseOutlineFlat(content))`。
 */
export function parseOutline(content: string): OutlineNode[] {
  return buildTree(parseOutlineFlat(content));
}

// ---------------------------------------------------------------------------
// 筛选
// ---------------------------------------------------------------------------

/**
 * 按层级范围筛选大纲节点（保留树形结构）。
 *
 * 只保留 `minLevel` ~ `maxLevel` 层级的节点；
 * 被过滤掉的层级的子节点会向上提升到最近的祖先节点下。
 *
 * @param nodes 完整大纲树
 * @param minLevel 最小层级（含），默认 1
 * @param maxLevel 最大层级（含），默认 6
 */
export function filterByLevel(
  nodes: OutlineNode[],
  minLevel = 1,
  maxLevel = 6,
): OutlineNode[] {
  const result: OutlineNode[] = [];

  function walk(node: OutlineNode, parent: OutlineNode | null): void {
    if (node.level < minLevel) {
      // 当前层级太浅，子节点向上提升到 parent
      for (const child of node.children) {
        walk(child, parent);
      }
      return;
    }

    if (node.level > maxLevel) {
      // 当前层级太深，直接跳过（子节点也更深，一并跳过）
      return;
    }

    // 在范围内：克隆节点（浅拷贝），递归处理子节点
    const filtered: OutlineNode = {
      ...node,
      children: [],
    };

    if (parent) {
      parent.children.push(filtered);
    } else {
      result.push(filtered);
    }

    for (const child of node.children) {
      walk(child, filtered);
    }
  }

  for (const node of nodes) {
    walk(node, null);
  }

  return result;
}
