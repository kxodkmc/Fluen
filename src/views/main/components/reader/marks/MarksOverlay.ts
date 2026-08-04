/**
 * MarksOverlay —— iframe 内注入的高亮层脚本。
 *
 * 本模块不直接执行，而是通过 `buildMarksOverlayScript()` 返回一段 JS 字符串，
 * 内联到 ReferenceContent.vue 构建的 iframe 文档 `<script>` 中。
 *
 * ## 职责
 *
 * 1. 监听 `mouseup` → 序列化选区（块内字符偏移）→ `postMessage('selection:create')`
 * 2. 接收 `marks:render` → 用 Range + surroundContents 包裹 `<mark>` 高亮
 * 3. 接收 `marks:remove` → 找到 mark 元素，unwrap 还原
 * 4. 接收 `marks:scroll` → 滚动到指定高亮
 * 5. 监听 mark 元素 click → `postMessage('mark:click')`
 * 6. 跨块选区 → `postMessage('selection:create')` 且 `block_key` 为空（外层提示）
 *
 * ## 选区序列化
 *
 * - 找 `startContainer` / `endContainer` 最近的 `[data-block-key]` 祖先
 * - 若不同块 → 跨块信号（`block_key=''`）
 * - 若同块 → 用 TreeWalker 遍历块内文本节点，累计字符偏移
 *
 * ## 高亮渲染
 *
 * - 按 `data-block-key` 定位块元素
 * - 由偏移重建 Range（可能跨多个文本节点）
 * - 对范围内每个文本节点分别 surroundContents（保留 inline 格式如 strong/em）
 * - 反向处理避免 DOM 分裂影响后续偏移
 *
 * @module reader/marks/MarksOverlay
 */

/**
 * 构建 iframe 内注入的脚本字符串。
 *
 * 返回的脚本为自执行函数，不污染 iframe 全局。
 */
export function buildMarksOverlayScript(): string {
  // 内部脚本使用字符串拼接（非模板字面量），避免外层 ${} 插值冲突
  return `(function(){
"use strict";

// ── 状态 ──
var MARK_ATTR = 'data-mark-id';
var BLOCK_ATTR = 'data-block-key';
var MARK_CLASS = 'fluen-mark';
// markId -> { colorClass, elements: [markEl...] }
var applied = new Map();
var ready = false;

// ── 工具：找最近 [data-block-key] 祖先 ──
function closestBlock(node) {
  if (!node) return null;
  var el = node.nodeType === 1 ? node : node.parentNode;
  while (el && el.nodeType === 1) {
    if (el.hasAttribute && el.hasAttribute(BLOCK_ATTR)) return el;
    el = el.parentNode;
  }
  return null;
}

// ── 工具：块内文本节点列表（DOM 顺序）──
function blockTextNodes(blockEl) {
  var walker = document.createTreeWalker(blockEl, NodeFilter.SHOW_TEXT, null);
  var out = [];
  while (walker.nextNode()) out.push(walker.currentNode);
  return out;
}

// ── 工具：计算选区在块内的字符偏移 + 块全文 ──
function computeOffsets(blockEl, range) {
  var nodes = blockTextNodes(blockEl);
  var blockText = '';
  var startOff = -1, endOff = -1;
  var startNode = range.startContainer, endNode = range.endContainer;
  for (var i = 0; i < nodes.length; i++) {
    var n = nodes[i];
    var len = n.nodeValue.length;
    if (n === startNode) startOff = blockText.length + range.startOffset;
    if (n === endNode) endOff = blockText.length + range.endOffset;
    blockText += n.nodeValue;
  }
  return { blockText: blockText, startOffset: startOff, endOffset: endOff };
}

// ── 工具：由偏移重建 Range ──
function rangeFromOffsets(blockEl, startOffset, endOffset) {
  var nodes = blockTextNodes(blockEl);
  var acc = 0;
  var startNode = null, startLocal = 0, endNode = null, endLocal = 0;
  for (var i = 0; i < nodes.length; i++) {
    var n = nodes[i];
    var len = n.nodeValue.length;
    if (!startNode && acc + len >= startOffset) {
      startNode = n;
      startLocal = startOffset - acc;
    }
    if (!endNode && acc + len >= endOffset) {
      endNode = n;
      endLocal = endOffset - acc;
    }
    if (startNode && endNode) break;
    acc += len;
  }
  if (!startNode || !endNode) return null;
  if (startLocal > startNode.nodeValue.length) startLocal = startNode.nodeValue.length;
  if (endLocal > endNode.nodeValue.length) endLocal = endNode.nodeValue.length;
  var r = document.createRange();
  r.setStart(startNode, startLocal);
  r.setEnd(endNode, endLocal);
  return r;
}

// ── 工具：包裹 Range 为多个 <mark>（保留 inline 格式）──
function wrapRange(range, markId, colorClass) {
  var root = range.commonAncestorContainer;
  if (root.nodeType === 3) root = root.parentNode;
  var walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, null);
  var parts = [];
  while (walker.nextNode()) {
    var node = walker.currentNode;
    if (!range.intersectsNode(node)) continue;
    var s = (node === range.startContainer) ? range.startOffset : 0;
    var e = (node === range.endContainer) ? range.endOffset : node.nodeValue.length;
    if (s < e) parts.push({ node: node, s: s, e: e });
  }
  var els = [];
  // 反向处理：避免 DOM 分裂影响后续节点的偏移
  for (var i = parts.length - 1; i >= 0; i--) {
    var p = parts[i];
    var mark = document.createElement('mark');
    mark.className = MARK_CLASS + ' ' + colorClass;
    mark.setAttribute(MARK_ATTR, markId);
    var r = document.createRange();
    r.setStart(p.node, p.s);
    r.setEnd(p.node, p.e);
    r.surroundContents(mark);
    els.push(mark);
  }
  return els;
}

// ── 工具：unwrap 单个 mark 元素 ──
function unwrapMark(el) {
  var parent = el.parentNode;
  if (!parent) return;
  while (el.firstChild) parent.insertBefore(el.firstChild, el);
  parent.removeChild(el);
  parent.normalize();
}

// ── 工具：清除全部已应用高亮 ──
function clearAll() {
  applied.forEach(function (entry) {
    for (var i = 0; i < entry.elements.length; i++) {
      var el = entry.elements[i];
      if (el.parentNode) unwrapMark(el);
    }
  });
  applied.clear();
}

// ── 工具：解析 block_key 字符串 "{type}:{line}:{occ}" ──
function parseBlockKey(str) {
  var idx1 = str.indexOf(':');
  var idx2 = str.lastIndexOf(':');
  if (idx1 < 0 || idx2 <= idx1) return null;
  return {
    block_type: str.substring(0, idx1),
    source_line: parseInt(str.substring(idx1 + 1, idx2), 10),
    occurrence: parseInt(str.substring(idx2 + 1), 10),
  };
}

// ── 渲染单条标记 ──
function applyMark(m) {
  var blockEl = document.querySelector('[' + BLOCK_ATTR + '="' + m.block_key + '"]');
  if (!blockEl) return; // 块不存在（orphaned），跳过
  var range = rangeFromOffsets(blockEl, m.start_offset, m.end_offset);
  if (!range) return;
  // 校验：块内对应文本是否与快照一致（不一致则跳过，外层标记为 degraded）
  var frag = range.toString();
  if (frag !== m.text) return;
  var colorClass = MARK_CLASS + '--' + m.color;
  var els = wrapRange(range, m.id, colorClass);
  applied.set(m.id, { colorClass: colorClass, elements: els });
}

// ── 移除单条高亮 ──
function removeMark(markId) {
  var entry = applied.get(markId);
  if (!entry) return;
  for (var i = 0; i < entry.elements.length; i++) {
    var el = entry.elements[i];
    if (el.parentNode) unwrapMark(el);
  }
  applied.delete(markId);
}

// ── 滚动到高亮 ──
function scrollToMark(markId) {
  var entry = applied.get(markId);
  if (!entry || entry.elements.length === 0) return;
  var el = entry.elements[0];
  el.scrollIntoView({ behavior: 'smooth', block: 'center' });
  // 闪烁高亮
  el.style.transition = 'box-shadow 0.3s';
  el.style.boxShadow = '0 0 0 3px rgba(59,130,246,0.5)';
  setTimeout(function () { el.style.boxShadow = ''; }, 1200);
}

// ── 选区序列化 ──
function serializeSelection() {
  var sel = window.getSelection();
  if (!sel || sel.isCollapsed || sel.rangeCount === 0) return;
  var range = sel.getRangeAt(0);
  var startBlock = closestBlock(range.startContainer);
  var endBlock = closestBlock(range.endContainer);
  var selectedText = sel.toString();
  if (!selectedText) return;

  // 跨块：发信号（block_key 为空），外层提示用户
  if (!startBlock || !endBlock || startBlock !== endBlock) {
    parent.postMessage({
      type: 'selection:create',
      anchor: {
        block_key: '',
        block_type: 'other',
        source_line: 0,
        occurrence: 0,
        start_offset: 0,
        end_offset: 0,
        block_text: '',
        selected_text: selectedText,
      },
      text: selectedText,
    }, '*');
    return;
  }

  var blockKey = startBlock.getAttribute(BLOCK_ATTR);
  var parsed = parseBlockKey(blockKey);
  if (!parsed) return;
  var off = computeOffsets(startBlock, range);
  if (off.startOffset < 0 || off.endOffset < 0) return;

  // 选区在 iframe 视口内的 bounding rect（外层据此定位颜色选择浮层）
  var rect = range.getBoundingClientRect();
  parent.postMessage({
    type: 'selection:create',
    anchor: {
      block_key: blockKey,
      block_type: parsed.block_type,
      source_line: parsed.source_line,
      occurrence: parsed.occurrence,
      start_offset: off.startOffset,
      end_offset: off.endOffset,
      block_text: off.blockText,
      selected_text: selectedText,
    },
    text: selectedText,
    rect: {
      left: rect.left,
      top: rect.top,
      right: rect.right,
      bottom: rect.bottom,
      width: rect.width,
      height: rect.height,
    },
  }, '*');
}

// ── 点击高亮 ──
function onClick(e) {
  var target = e.target;
  var markEl = target && target.closest ? target.closest('mark.' + MARK_CLASS) : null;
  if (!markEl) return;
  var id = markEl.getAttribute(MARK_ATTR);
  if (!id) return;
  parent.postMessage({ type: 'mark:click', markId: id }, '*');
}

// ── 消息处理 ──
function onMessage(e) {
  var data = e.data;
  if (!data || typeof data !== 'object') return;
  if (data.type === 'marks:render') {
    clearAll();
    var marks = data.marks || [];
    for (var i = 0; i < marks.length; i++) applyMark(marks[i]);
  } else if (data.type === 'marks:clear') {
    clearAll();
  } else if (data.type === 'marks:remove') {
    removeMark(data.markId);
  } else if (data.type === 'marks:scroll') {
    scrollToMark(data.markId);
  }
}

// ── 初始化 ──
function init() {
  if (ready) return;
  ready = true;
  document.addEventListener('mouseup', function () {
    // 延迟一帧，让浏览器先更新 selection
    setTimeout(serializeSelection, 0);
  });
  document.addEventListener('click', onClick, true);
  window.addEventListener('message', onMessage);
  parent.postMessage({ type: 'marks:ready' }, '*');
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', init);
} else {
  init();
}
})();`;
}
