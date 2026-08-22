<script setup lang="ts">
/**
 * AnalysisResult — 统计分析结果渲染器。
 *
 * 根据 type prop 切换渲染逻辑，每种分析类型对应一组卡片/表格。
 * 所有结果类型镜像 socstat 的 Serialize 结构体。
 */
import type {
  ChiSquareTest,
  CorrelationPair,
  Crosstab,
  DescriptiveResult,
  FactorialAnova,
  FisherExactTest,
  IndependentTTest,
  KolmogorovSmirnovResult,
  KruskalWallisResult,
  LinearRegressionResult,
  LogisticRegressionResult,
  MannWhitneyUTest,
  OneWayAnova,
  PairedTTest,
  PcaResult,
  PostHocResult,
  PartialCorrelationResult,
  ReliabilityResult,
  ShapiroWilkResult,
  VifResult,
  WilcoxonSignedRankResult,
} from '../../../../../types/dataAnalysis';

const props = defineProps<{
  result: unknown;
  type: string;
}>();

function fmt(x: number | undefined | null): string {
  if (x == null || !Number.isFinite(x)) return '—';
  const abs = Math.abs(x);
  if (abs === 0) return '0';
  if (abs < 0.001 || abs >= 1e6) return x.toExponential(3);
  return x.toFixed(4);
}

function fmtP(p: number | undefined | null): string {
  if (p == null || !Number.isFinite(p)) return '—';
  if (p < 0.001) return '< 0.001';
  return p.toFixed(4);
}

const r = <T,>() => props.result as T;
</script>

<template>
  <div class="ar">
    <!-- 描述统计 -->
    <template v-if="type === 'descriptive'">
      <div class="ar-grid">
        <div class="ar-stat" v-for="f in ['n','mean','std_dev','variance','min','max','range','sum','median','skewness','kurtosis','q1','q3','sem']" :key="f">
          <span>{{ f }}</span><b>{{ fmt((r<DescriptiveResult>())[f as keyof DescriptiveResult] as number) }}</b>
        </div>
      </div>
    </template>

    <!-- 频数表 -->
    <template v-else-if="type === 'frequencies'">
      <table class="ar-table">
        <thead><tr><th>Value</th><th>n</th><th>%</th></tr></thead>
        <tbody>
          <tr v-for="row in (r as any).rows" :key="row.value">
            <td>{{ row.value }}</td><td>{{ row.count }}</td><td>{{ row.valid_percent.toFixed(1) }}</td>
          </tr>
        </tbody>
      </table>
    </template>

    <!-- 交叉表 -->
    <template v-else-if="type === 'crosstab'">
      <table class="ar-table">
        <thead><tr><th></th><th v-for="c in (r<Crosstab>()).col_labels" :key="c">{{ c }}</th></tr></thead>
        <tbody>
          <tr v-for="(row, i) in (r<Crosstab>()).counts" :key="i">
            <td class="ar-table__label">{{ (r<Crosstab>()).row_labels[i] }}</td>
            <td v-for="(c, j) in row" :key="j">{{ c }}</td>
          </tr>
        </tbody>
      </table>
    </template>

    <!-- 独立样本T检验 -->
    <template v-else-if="type === 'independentTTest'">
      <div class="ar-section">
        <p class="ar-section__title">Group Statistics</p>
        <table class="ar-table">
          <thead><tr><th>Group</th><th>n</th><th>Mean</th><th>SD</th></tr></thead>
          <tbody>
            <tr v-for="g in (r<IndependentTTest>()).group_stats" :key="g.label">
              <td>{{ g.label }}</td><td>{{ fmt(g.n) }}</td><td>{{ fmt(g.mean) }}</td><td>{{ fmt(g.std_dev) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <div class="ar-section">
        <p class="ar-section__title">Levene's Test</p>
        <div class="ar-grid">
          <div class="ar-stat"><span>F</span><b>{{ fmt((r<IndependentTTest>()).levene_test.f_statistic) }}</b></div>
          <div class="ar-stat"><span>p</span><b>{{ fmtP((r<IndependentTTest>()).levene_test.p_value) }}</b></div>
        </div>
      </div>
      <div class="ar-section">
        <p class="ar-section__title">Equal Variances (Pooled)</p>
        <div class="ar-grid">
          <div class="ar-stat"><span>t</span><b>{{ fmt((r<IndependentTTest>()).equal_variances.t_statistic) }}</b></div>
          <div class="ar-stat"><span>df</span><b>{{ fmt((r<IndependentTTest>()).equal_variances.df) }}</b></div>
          <div class="ar-stat"><span>p</span><b>{{ fmtP((r<IndependentTTest>()).equal_variances.p_value) }}</b></div>
          <div class="ar-stat"><span>MD</span><b>{{ fmt((r<IndependentTTest>()).equal_variances.mean_difference) }}</b></div>
        </div>
      </div>
    </template>

    <!-- 配对T检验 -->
    <template v-else-if="type === 'pairedTTest'">
      <div class="ar-grid">
        <div class="ar-stat"><span>n</span><b>{{ fmt((r<PairedTTest>()).n) }}</b></div>
        <div class="ar-stat"><span>MD</span><b>{{ fmt((r<PairedTTest>()).mean_difference) }}</b></div>
        <div class="ar-stat"><span>SD_diff</span><b>{{ fmt((r<PairedTTest>()).std_dev_difference) }}</b></div>
        <div class="ar-stat"><span>t</span><b>{{ fmt((r<PairedTTest>()).t_statistic) }}</b></div>
        <div class="ar-stat"><span>df</span><b>{{ fmt((r<PairedTTest>()).df) }}</b></div>
        <div class="ar-stat"><span>p</span><b>{{ fmtP((r<PairedTTest>()).p_value) }}</b></div>
        <div class="ar-stat"><span>r</span><b>{{ fmt((r<PairedTTest>()).correlation) }}</b></div>
      </div>
    </template>

    <!-- ANOVA -->
    <template v-else-if="type === 'oneWayAnova'">
      <div class="ar-grid">
        <div class="ar-stat"><span>F</span><b>{{ fmt((r<OneWayAnova>()).f_statistic) }}</b></div>
        <div class="ar-stat"><span>p</span><b>{{ fmtP((r<OneWayAnova>()).p_value) }}</b></div>
        <div class="ar-stat"><span>η²</span><b>{{ fmt((r<OneWayAnova>()).eta_squared) }}</b></div>
      </div>
      <table class="ar-table">
        <thead><tr><th>Source</th><th>SS</th><th>df</th><th>MS</th><th>F</th><th>p</th></tr></thead>
        <tbody>
          <tr><td>Between</td><td>{{ fmt((r<OneWayAnova>()).between_groups.ss) }}</td><td>{{ fmt((r<OneWayAnova>()).between_groups.df) }}</td><td>{{ fmt((r<OneWayAnova>()).between_groups.ms) }}</td><td>{{ fmt((r<OneWayAnova>()).between_groups.f) }}</td><td>{{ fmtP((r<OneWayAnova>()).between_groups.p_value) }}</td></tr>
          <tr><td>Within</td><td>{{ fmt((r<OneWayAnova>()).within_groups.ss) }}</td><td>{{ fmt((r<OneWayAnova>()).within_groups.df) }}</td><td>{{ fmt((r<OneWayAnova>()).within_groups.ms) }}</td><td>—</td><td>—</td></tr>
        </tbody>
      </table>
    </template>

    <!-- Mann-Whitney U -->
    <template v-else-if="type === 'mannWhitney'">
      <div class="ar-grid">
        <div class="ar-stat"><span>U</span><b>{{ fmt((r<MannWhitneyUTest>()).u_statistic) }}</b></div>
        <div class="ar-stat"><span>Z</span><b>{{ fmt((r<MannWhitneyUTest>()).z_score) }}</b></div>
        <div class="ar-stat"><span>p</span><b>{{ fmtP((r<MannWhitneyUTest>()).p_value) }}</b></div>
      </div>
    </template>

    <!-- Wilcoxon -->
    <template v-else-if="type === 'wilcoxon'">
      <div class="ar-grid">
        <div class="ar-stat"><span>W+</span><b>{{ fmt((r<WilcoxonSignedRankResult>()).w_positive) }}</b></div>
        <div class="ar-stat"><span>W−</span><b>{{ fmt((r<WilcoxonSignedRankResult>()).w_negative) }}</b></div>
        <div class="ar-stat"><span>Z</span><b>{{ fmt((r<WilcoxonSignedRankResult>()).z_score) }}</b></div>
        <div class="ar-stat"><span>p</span><b>{{ fmtP((r<WilcoxonSignedRankResult>()).p_value) }}</b></div>
      </div>
    </template>

    <!-- Kruskal-Wallis -->
    <template v-else-if="type === 'kruskalWallis'">
      <div class="ar-grid">
        <div class="ar-stat"><span>H</span><b>{{ fmt((r<KruskalWallisResult>()).h_statistic) }}</b></div>
        <div class="ar-stat"><span>df</span><b>{{ fmt((r<KruskalWallisResult>()).df) }}</b></div>
        <div class="ar-stat"><span>p</span><b>{{ fmtP((r<KruskalWallisResult>()).p_value) }}</b></div>
      </div>
    </template>

    <!-- 卡方检验 -->
    <template v-else-if="type === 'chiSquare'">
      <div class="ar-grid">
        <div class="ar-stat"><span>χ²</span><b>{{ fmt((r<ChiSquareTest>()).chi_square) }}</b></div>
        <div class="ar-stat"><span>df</span><b>{{ fmt((r<ChiSquareTest>()).df) }}</b></div>
        <div class="ar-stat"><span>p</span><b>{{ fmtP((r<ChiSquareTest>()).p_value) }}</b></div>
        <div class="ar-stat"><span>n</span><b>{{ fmt((r<ChiSquareTest>()).n) }}</b></div>
      </div>
    </template>

    <!-- Fisher精确检验 -->
    <template v-else-if="type === 'fisherExact'">
      <div class="ar-grid">
        <div class="ar-stat"><span>OR</span><b>{{ fmt((r<FisherExactTest>()).odds_ratio) }}</b></div>
        <div class="ar-stat"><span>p (2-sided)</span><b>{{ fmtP((r<FisherExactTest>()).p_value_two_sided) }}</b></div>
        <div class="ar-stat"><span>p (less)</span><b>{{ fmtP((r<FisherExactTest>()).p_value_less) }}</b></div>
        <div class="ar-stat"><span>p (greater)</span><b>{{ fmtP((r<FisherExactTest>()).p_value_greater) }}</b></div>
      </div>
    </template>

    <!-- Shapiro-Wilk -->
    <template v-else-if="type === 'shapiroWilk'">
      <div class="ar-grid">
        <div class="ar-stat"><span>W</span><b>{{ fmt((r<ShapiroWilkResult>()).w_statistic) }}</b></div>
        <div class="ar-stat"><span>p</span><b>{{ fmtP((r<ShapiroWilkResult>()).p_value) }}</b></div>
      </div>
    </template>

    <!-- K-S 正态性检验 -->
    <template v-else-if="type === 'ksNormality'">
      <div class="ar-grid">
        <div class="ar-stat"><span>D</span><b>{{ fmt((r<KolmogorovSmirnovResult>()).d_statistic) }}</b></div>
        <div class="ar-stat"><span>p</span><b>{{ fmtP((r<KolmogorovSmirnovResult>()).p_value) }}</b></div>
      </div>
    </template>

    <!-- 相关分析（多变量） -->
    <template v-else-if="type === 'correlation' || type === 'correlationPair'">
      <table class="ar-table" v-if="Array.isArray(props.result)">
        <thead><tr><th>Pair</th><th>n</th><th>r</th><th>p</th></tr></thead>
        <tbody>
          <tr v-for="p in (r<CorrelationPair[]>())" :key="`${p.var1}-${p.var2}`">
            <td>{{ p.var1 }} ~ {{ p.var2 }}</td>
            <td>{{ fmt(p.n) }}</td>
            <td>{{ fmt(p.pearson?.coefficient ?? p.spearman?.coefficient ?? p.kendall?.coefficient) }}</td>
            <td>{{ fmtP(p.pearson?.p_value ?? p.spearman?.p_value ?? p.kendall?.p_value) }}</td>
          </tr>
        </tbody>
      </table>
      <div v-else class="ar-grid">
        <div class="ar-stat"><span>n</span><b>{{ fmt((r<CorrelationPair>()).n) }}</b></div>
        <div class="ar-stat"><span>r</span><b>{{ fmt((r<CorrelationPair>()).pearson?.coefficient ?? (r<CorrelationPair>()).spearman?.coefficient ?? (r<CorrelationPair>()).kendall?.coefficient) }}</b></div>
        <div class="ar-stat"><span>p</span><b>{{ fmtP((r<CorrelationPair>()).pearson?.p_value ?? (r<CorrelationPair>()).spearman?.p_value ?? (r<CorrelationPair>()).kendall?.p_value) }}</b></div>
      </div>
    </template>

    <!-- 偏相关 -->
    <template v-else-if="type === 'partialCorrelation'">
      <div class="ar-grid">
        <div class="ar-stat"><span>r</span><b>{{ fmt((r<PartialCorrelationResult>()).coefficient) }}</b></div>
        <div class="ar-stat"><span>df</span><b>{{ fmt((r<PartialCorrelationResult>()).df) }}</b></div>
        <div class="ar-stat"><span>p</span><b>{{ fmtP((r<PartialCorrelationResult>()).p_value) }}</b></div>
      </div>
    </template>

    <!-- 线性回归 -->
    <template v-else-if="type === 'regression'">
      <div class="ar-grid">
        <div class="ar-stat"><span>R²</span><b>{{ fmt((r<LinearRegressionResult>()).r_squared) }}</b></div>
        <div class="ar-stat"><span>Adj R²</span><b>{{ fmt((r<LinearRegressionResult>()).adj_r_squared) }}</b></div>
        <div class="ar-stat"><span>F</span><b>{{ fmt((r<LinearRegressionResult>()).f_statistic) }}</b></div>
        <div class="ar-stat"><span>p</span><b>{{ fmtP((r<LinearRegressionResult>()).f_p_value) }}</b></div>
      </div>
      <table class="ar-table">
        <thead><tr><th>Predictor</th><th>β</th><th>SE</th><th>t</th><th>p</th></tr></thead>
        <tbody>
          <tr v-for="c in (r<LinearRegressionResult>()).coefficients" :key="c.name">
            <td>{{ c.name }}</td><td>{{ fmt(c.estimate) }}</td><td>{{ fmt(c.std_error) }}</td><td>{{ fmt(c.t_statistic) }}</td><td>{{ fmtP(c.p_value) }}</td>
          </tr>
        </tbody>
      </table>
    </template>

    <!-- 逻辑回归 -->
    <template v-else-if="type === 'logisticRegression'">
      <div class="ar-grid">
        <div class="ar-stat"><span>AIC</span><b>{{ fmt((r<LogisticRegressionResult>()).aic) }}</b></div>
        <div class="ar-stat"><span>McFadden R²</span><b>{{ fmt((r<LogisticRegressionResult>()).mcfadden_r2) }}</b></div>
        <div class="ar-stat"><span>Cox-Snell R²</span><b>{{ fmt((r<LogisticRegressionResult>()).cox_snell_r2) }}</b></div>
      </div>
      <table class="ar-table">
        <thead><tr><th>Predictor</th><th>β</th><th>OR</th><th>z</th><th>p</th></tr></thead>
        <tbody>
          <tr v-for="c in (r<LogisticRegressionResult>()).coefficients" :key="c.name">
            <td>{{ c.name }}</td><td>{{ fmt(c.estimate) }}</td><td>{{ fmt(c.odds_ratio) }}</td><td>{{ fmt(c.z_statistic) }}</td><td>{{ fmtP(c.p_value) }}</td>
          </tr>
        </tbody>
      </table>
    </template>

    <!-- VIF -->
    <template v-else-if="type === 'vif'">
      <table class="ar-table">
        <thead><tr><th>Variable</th><th>VIF</th><th>Tolerance</th><th>R²</th></tr></thead>
        <tbody>
          <tr v-for="v in (r<VifResult[]>())" :key="v.variable">
            <td>{{ v.variable }}</td><td>{{ fmt(v.vif) }}</td><td>{{ fmt(v.tolerance) }}</td><td>{{ fmt(v.r_squared) }}</td>
          </tr>
        </tbody>
      </table>
    </template>

    <!-- PCA -->
    <template v-else-if="type === 'pca'">
      <table class="ar-table">
        <thead><tr><th>PC</th><th>λ</th><th>% Var</th><th>Cumulative</th></tr></thead>
        <tbody>
          <tr v-for="(c, i) in (r<PcaResult>()).components" :key="i">
            <td>PC{{ i + 1 }}</td><td>{{ fmt(c.eigenvalue) }}</td><td>{{ (c.explained_variance_ratio * 100).toFixed(2) }}%</td><td>{{ (c.cumulative_variance_ratio * 100).toFixed(2) }}%</td>
          </tr>
        </tbody>
      </table>
    </template>

    <!-- 信度分析 -->
    <template v-else-if="type === 'reliability'">
      <div class="ar-grid">
        <div class="ar-stat"><span>α</span><b>{{ fmt((r<ReliabilityResult>()).alpha) }}</b></div>
        <div class="ar-stat"><span>Std α</span><b>{{ fmt((r<ReliabilityResult>()).standardized_alpha) }}</b></div>
        <div class="ar-stat"><span>n</span><b>{{ fmt((r<ReliabilityResult>()).n) }}</b></div>
      </div>
      <table class="ar-table">
        <thead><tr><th>Item</th><th>r_it</th><th>α if deleted</th></tr></thead>
        <tbody>
          <tr v-for="it in (r<ReliabilityResult>()).item_statistics" :key="it.item">
            <td>{{ it.item }}</td><td>{{ fmt(it.corrected_item_total_correlation) }}</td><td>{{ fmt(it.alpha_if_deleted) }}</td>
          </tr>
        </tbody>
      </table>
    </template>

    <!-- 事后检验 -->
    <template v-else-if="type === 'postHoc'">
      <table class="ar-table">
        <thead><tr><th>Group1</th><th>Group2</th><th>MD</th><th>SE</th><th>Stat</th><th>p (adj)</th></tr></thead>
        <tbody>
          <tr v-for="c in (r<PostHocResult>()).comparisons" :key="`${c.group1}-${c.group2}`">
            <td>{{ c.group1 }}</td><td>{{ c.group2 }}</td><td>{{ fmt(c.mean_difference) }}</td><td>{{ fmt(c.std_error) }}</td><td>{{ fmt(c.statistic) }}</td><td>{{ fmtP(c.p_value) }}</td>
          </tr>
        </tbody>
      </table>
    </template>

    <!-- 析因方差分析 -->
    <template v-else-if="type === 'factorialAnova'">
      <div class="ar-grid">
        <div class="ar-stat"><span>R²</span><b>{{ fmt((r<FactorialAnova>()).r_squared) }}</b></div>
        <div class="ar-stat"><span>Adj R²</span><b>{{ fmt((r<FactorialAnova>()).adj_r_squared) }}</b></div>
        <div class="ar-stat"><span>n</span><b>{{ fmt((r<FactorialAnova>()).n) }}</b></div>
        <div class="ar-stat"><span>SS type</span><b>{{ (r<FactorialAnova>()).ss_type === 'type_i' ? 'Type I' : 'Type II' }}</b></div>
      </div>
      <table class="ar-table">
        <thead><tr><th>Source</th><th>SS</th><th>df</th><th>MS</th><th>F</th><th>p</th><th>η²</th><th>partial η²</th></tr></thead>
        <tbody>
          <tr v-for="e in (r<FactorialAnova>()).effects" :key="e.source">
            <td>{{ e.source }}</td>
            <td>{{ fmt(e.ss) }}</td>
            <td>{{ fmt(e.df) }}</td>
            <td>{{ fmt(e.ms) }}</td>
            <td>{{ fmt(e.f) }}</td>
            <td>{{ fmtP(e.p_value) }}</td>
            <td>{{ fmt(e.eta_squared) }}</td>
            <td>{{ fmt(e.partial_eta_squared) }}</td>
          </tr>
        </tbody>
      </table>
    </template>

    <!-- 未知 -->
    <template v-else>
      <pre class="ar-raw">{{ JSON.stringify(props.result, null, 2) }}</pre>
    </template>
  </div>
</template>

<style scoped>
.ar {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.ar-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 6px;
}
.ar-stat {
  display: flex;
  justify-content: space-between;
  padding: 6px 8px;
  background: var(--fluen-surface);
  border: 1px solid var(--fluen-hairline);
  border-radius: 6px;
  font-size: 12px;
}
.ar-stat span {
  color: var(--fluen-stone);
}
.ar-stat b {
  color: var(--fluen-ink);
  font-variant-numeric: tabular-nums;
}
.ar-section__title {
  font-size: 12px;
  font-weight: 600;
  color: var(--fluen-stone);
  margin: 0 0 4px 0;
}
.ar-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 11px;
}
.ar-table th,
.ar-table td {
  padding: 4px 6px;
  text-align: left;
  border-bottom: 1px solid var(--fluen-hairline);
  color: var(--fluen-ink);
}
.ar-table th {
  font-weight: 600;
  color: var(--fluen-stone);
}
.ar-table__label {
  font-weight: 500;
}
.ar-raw {
  font-size: 11px;
  white-space: pre-wrap;
  word-break: break-all;
  color: var(--fluen-stone);
  max-height: 200px;
  overflow-y: auto;
}
</style>
