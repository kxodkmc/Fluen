<script setup lang="ts">
/**
 * MathPicker — Word 式公式插入面板。
 *
 * 参照 Word 公式工具条的分组逻辑（精简为学术写作高频项）：
 *   - 顶部：行内公式 / 块级公式两种语法形态入口；
 *   - 符号：常用数学符号网格，点击插入对应 LaTeX 命令；
 *   - 结构：分式/上下标/根式/积分/求和/极限等模板，点击插入带占位
 *     参数的 LaTeX 骨架。
 *
 * 插入目标由命令层（mathEditing）自动判定：焦点在表格单元格时插入
 * 单元格，否则插入文档光标处。本组件仅上交意图，保持与命令解耦。
 */
import { useI18n } from '../../../../../i18n';

const emit = defineEmits<{
  insertInline: [];
  insertBlock: [];
  insert: [latex: string];
}>();

const { t } = useI18n();

/** 常用符号：glyph 为面板显示，latex 为插入内容。 */
const SYMBOLS: { glyph: string; latex: string }[] = [
  { glyph: '±', latex: '\\pm' },
  { glyph: '×', latex: '\\times' },
  { glyph: '÷', latex: '\\div' },
  { glyph: '≤', latex: '\\leq' },
  { glyph: '≥', latex: '\\geq' },
  { glyph: '≠', latex: '\\neq' },
  { glyph: '≈', latex: '\\approx' },
  { glyph: '∞', latex: '\\infty' },
  { glyph: '∝', latex: '\\propto' },
  { glyph: '∂', latex: '\\partial' },
  { glyph: '∇', latex: '\\nabla' },
  { glyph: '°', latex: '^\\circ' },
  { glyph: '→', latex: '\\to' },
  { glyph: '∈', latex: '\\in' },
  { glyph: 'α', latex: '\\alpha' },
  { glyph: 'β', latex: '\\beta' },
  { glyph: 'γ', latex: '\\gamma' },
  { glyph: 'θ', latex: '\\theta' },
  { glyph: 'λ', latex: '\\lambda' },
  { glyph: 'μ', latex: '\\mu' },
  { glyph: 'σ', latex: '\\sigma' },
  { glyph: 'π', latex: '\\pi' },
  { glyph: 'Δ', latex: '\\Delta' },
  { glyph: 'Ω', latex: '\\Omega' },
];

/** 结构模板：插入带占位参数的 LaTeX 骨架（占位符由用户覆写）。 */
const STRUCTURES: { glyph: string; latex: string; key: string }[] = [
  { glyph: 'a/b', latex: '\\frac{a}{b}', key: 'fraction' },
  { glyph: 'x²', latex: 'x^{2}', key: 'superscript' },
  { glyph: 'xᵢ', latex: 'x_{i}', key: 'subscript' },
  { glyph: '√x', latex: '\\sqrt{x}', key: 'sqrt' },
  { glyph: '∫', latex: '\\int_{a}^{b}', key: 'integral' },
  { glyph: '∑', latex: '\\sum_{i=1}^{n}', key: 'sum' },
  { glyph: 'lim', latex: '\\lim_{x \\to 0}', key: 'limit' },
  { glyph: 'x̄', latex: '\\bar{x}', key: 'bar' },
];
</script>

<template>
  <div class="math-picker">
    <div class="math-picker__modes">
      <button class="math-picker__mode" type="button" @mousedown.prevent @click="emit('insertInline')">
        <span class="math-picker__mode-glyph">$x$</span>
        <span>{{ t('main.content.toolbar.inlineMath') }}</span>
      </button>
      <button class="math-picker__mode" type="button" @mousedown.prevent @click="emit('insertBlock')">
        <span class="math-picker__mode-glyph">$$x$$</span>
        <span>{{ t('main.content.toolbar.blockMath') }}</span>
      </button>
    </div>

    <div class="math-picker__section">
      <span class="math-picker__label">{{ t('main.content.toolbar.mathSymbols') }}</span>
      <div class="math-picker__grid">
        <button
          v-for="s in SYMBOLS"
          :key="s.latex"
          class="math-picker__cell"
          type="button"
          :title="s.latex"
          @mousedown.prevent
          @click="emit('insert', s.latex)"
        >{{ s.glyph }}</button>
      </div>
    </div>

    <div class="math-picker__section">
      <span class="math-picker__label">{{ t('main.content.toolbar.mathStructures') }}</span>
      <div class="math-picker__grid math-picker__grid--structures">
        <button
          v-for="s in STRUCTURES"
          :key="s.key"
          class="math-picker__cell math-picker__cell--wide"
          type="button"
          :title="s.latex"
          @mousedown.prevent
          @click="emit('insert', s.latex)"
        >{{ s.glyph }}</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.math-picker {
  width: 232px;
  padding: 10px;
}

/* ── 语法形态入口 ─────────────────────────────────────────────────── */
.math-picker__modes {
  display: flex;
  gap: 6px;
  padding-bottom: 10px;
  margin-bottom: 10px;
  border-bottom: 1px solid var(--fluen-hairline);
}

.math-picker__mode {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 3px;
  padding: 6px 4px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 6px;
  background: var(--fluen-surface-soft);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 11px;
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease;
}

.math-picker__mode:hover {
  background: var(--fluen-hover);
  border-color: var(--fluen-accent);
}

.math-picker__mode-glyph {
  font-size: 13px;
  font-weight: 600;
}

/* ── 分区 ─────────────────────────────────────────────────────────── */
.math-picker__section + .math-picker__section {
  margin-top: 10px;
}

.math-picker__label {
  display: block;
  margin-bottom: 6px;
  color: var(--fluen-slate);
  font-size: 12px;
  font-weight: 600;
}

.math-picker__grid {
  display: grid;
  grid-template-columns: repeat(8, 1fr);
  gap: 3px;
}

.math-picker__grid--structures {
  grid-template-columns: repeat(4, 1fr);
}

.math-picker__cell {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  height: 26px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 4px;
  background: var(--fluen-surface);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-serif, var(--fluen-font-sans));
  font-size: 14px;
  line-height: 1;
  cursor: pointer;
  transition: background 0.12s ease, border-color 0.12s ease;
}

.math-picker__cell--wide {
  font-size: 13px;
}

.math-picker__cell:hover {
  background: var(--fluen-hover);
  border-color: var(--fluen-accent);
}

.math-picker__cell:focus-visible {
  outline: 2px solid var(--fluen-accent);
  outline-offset: 1px;
}
</style>
