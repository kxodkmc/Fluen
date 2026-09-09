/**
 * useChatQuotes — Motis 对话引用文段状态（模块级单例）。
 *
 * 管理从论文编辑器划选后「添加到对话」的引用文段：
 *   - 编辑器划选工具栏写入（addQuote）
 *   - Motis 输入区展示引用卡片并支持移除
 *   - 发送消息时由 useMotisChat 组装进上下文并清空
 *
 * 单例设计使编辑器侧与对话面板侧无需 provide/inject 即共享同一份数据。
 */

import { ref, readonly, computed } from 'vue';

/** 单条引用文段。 */
export interface ChatQuote {
  /** 唯一 id（用于列表 key 与移除）。 */
  id: string;
  /** 引用的原文（已去除首尾空白）。 */
  text: string;
}

const _quotes = ref<ChatQuote[]>([]);

let _seq = 0;

/** 是否存在待发送的引用。 */
const hasQuotes = computed(() => _quotes.value.length > 0);

/**
 * 添加一条引用文段（去重：完全相同的文本不重复添加）。
 *
 * @returns 是否实际添加（重复时返回 false）。
 */
function addQuote(text: string): boolean {
  const trimmed = text.trim();
  if (!trimmed) return false;
  if (_quotes.value.some((q) => q.text === trimmed)) return false;
  _seq += 1;
  _quotes.value.push({ id: `quote-${Date.now()}-${_seq}`, text: trimmed });
  return true;
}

/** 移除指定引用。 */
function removeQuote(id: string): void {
  _quotes.value = _quotes.value.filter((q) => q.id !== id);
}

/** 清空全部引用（发送成功后调用）。 */
function clearQuotes(): void {
  _quotes.value = [];
}

export function useChatQuotes() {
  return {
    quotes: readonly(_quotes),
    hasQuotes,
    addQuote,
    removeQuote,
    clearQuotes,
  };
}
