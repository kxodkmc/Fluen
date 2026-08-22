/**
 * 跨解析稳定的节点 id 纯函数层。
 *
 * 目标：给每次重新解析的扁平标题列表分配稳定的 id，供 Vue `:key` 与折叠集合
 * 索引使用。只要标题的 `sectionId|level|text` 未变，就复用上一轮列表的 id，
 * 从而避免"标题上方增删一行"导致其后的所有标题 key 变化、被销毁重建。
 *
 * 重命名会使该行获得新 id（仅一行 remount，可接受）；折叠状态按 id 存储，
 * 因此重命名后该行的折叠状态随之丢失（已知取舍）。
 */

import type { FlatHeading, OutlineFlat } from './outlineParser';

/** id 序列号，保证会话内新建 id 唯一。 */
let _seq = 0;

function newId(): string {
  _seq += 1;
  return `h${_seq.toString(36)}`;
}

/** 内容键：`sectionId|level|text`。 */
function contentKey(h: Pick<OutlineFlat, 'sectionId' | 'level' | 'text'>): string {
  return `${h.sectionId ?? ''}|${h.level}|${h.text}`;
}

/**
 * 为 `next` 逐项分配 id：与 `prev` 相同内容键且未被占用的项复用其 id，其余新建。
 *
 * @param prev 上一轮已带 id 的列表。
 * @param next 本轮解析出的无 id 列表（长度可能变化）。
 * @returns 与 `next` 等长、携带稳定 id 的 [`FlatHeading`] 列表。
 */
export function assignStableIds(prev: FlatHeading[], next: OutlineFlat[]): FlatHeading[] {
  // 内容键 → prev 中候选下标（优先取最靠前的未用项，支持重名队列匹配）。
  const byKey = new Map<string, number[]>();
  prev.forEach((p, i) => {
    const key = contentKey(p);
    const list = byKey.get(key);
    if (list) list.push(i);
    else byKey.set(key, [i]);
  });

  const used = new Set<number>();
  return next.map((n) => {
    const candidates = byKey.get(contentKey(n));
    let id: string | null = null;
    if (candidates) {
      for (const i of candidates) {
        if (!used.has(i)) {
          used.add(i);
          id = prev[i].id;
          break;
        }
      }
    }
    return id === null ? { ...n, id: newId() } : { ...n, id };
  });
}