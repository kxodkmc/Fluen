/**
 * 数据分析模块 — 前端类型定义。
 *
 * 与后端 `data_analysis::commands` 返回结构对应。
 * 所有结构体镜像 socstat 的 Rust 结构体（已派生 Serialize）。
 */

// ---------------------------------------------------------------------------
// 数据加载
// ---------------------------------------------------------------------------

/** 数据文件 schema 摘要（`data_load_dataset` 返回）。 */
export interface DatasetSchema {
  path: string;
  n_rows: number;
  n_vars: number;
  variables: VariableInfo[];
}

/** 单个变量的元信息。 */
export interface VariableInfo {
  name: string;
  label: string;
  data_type: 'Numeric' | 'Text';
  measure: 'Nominal' | 'Ordinal' | 'Scale';
  n_valid: number;
  n_missing: number;
}

// ---------------------------------------------------------------------------
// 描述统计 & 频数 & 交叉表
// ---------------------------------------------------------------------------

/** 描述统计（`data_descriptive` 返回）。 */
export interface DescriptiveResult {
  n: number;
  mean: number;
  std_dev: number;
  variance: number;
  min: number;
  max: number;
  range: number;
  sum: number;
  median: number;
  skewness: number;
  kurtosis: number;
  q1: number;
  q3: number;
  sem: number;
  ci_95: [number, number];
}

/** 频数表行。 */
export interface FrequencyRow {
  value: string;
  count: number;
  percent: number;
  valid_percent: number;
  cumulative: number;
}

/** 频数表。 */
export interface FrequencyTable {
  rows: FrequencyRow[];
  n_valid: number;
  n_missing: number;
  total: number;
}

/** 交叉表（列联表）。 */
export interface Crosstab {
  row_labels: string[];
  col_labels: string[];
  counts: number[][];
  expected: number[][];
  row_pcts: number[][];
  col_pcts: number[][];
  total_pcts: number[][];
  n: number;
  row_totals: number[];
  col_totals: number[];
}

// ---------------------------------------------------------------------------
// 均值比较（参数检验）
// ---------------------------------------------------------------------------

/** 组统计摘要。 */
export interface GroupSummary {
  label: string;
  n: number;
  mean: number;
  std_dev: number;
  variance: number;
  min: number;
  max: number;
  std_error: number;
}

/** Levene 方差齐性检验。 */
export interface LeveneResult {
  f_statistic: number;
  df1: number;
  df2: number;
  p_value: number;
}

/** 单个 t 检验模型（合并方差或 Welch）。 */
export interface TTestModel {
  t_statistic: number;
  df: number;
  p_value: number;
  mean_difference: number;
  std_error: number;
  ci_95: [number, number];
}

/** 独立样本T检验。 */
export interface IndependentTTest {
  group_stats: GroupSummary[];
  levene_test: LeveneResult;
  equal_variances: TTestModel;
  unequal_variances: TTestModel;
}

/** 配对样本T检验。 */
export interface PairedTTest {
  n: number;
  mean_difference: number;
  std_dev_difference: number;
  std_error: number;
  t_statistic: number;
  df: number;
  p_value: number;
  ci_95: [number, number];
  correlation: number;
}

/** ANOVA 效应行。 */
export interface Effect {
  ss: number;
  df: number;
  ms: number;
  f: number | null;
  p_value: number | null;
}

/** 单因素方差分析。 */
export interface OneWayAnova {
  group_stats: GroupSummary[];
  between_groups: Effect;
  within_groups: Effect;
  total: Effect;
  f_statistic: number;
  p_value: number;
  eta_squared: number;
}

// ---------------------------------------------------------------------------
// 均值比较（非参数检验）
// ---------------------------------------------------------------------------

/** 秩摘要。 */
export interface RankSummary {
  label: string;
  n: number;
  rank_sum: number;
  mean_rank: number;
}

/** Mann-Whitney U 检验。 */
export interface MannWhitneyUTest {
  group_stats: RankSummary[];
  u_statistic: number;
  z_score: number;
  p_value: number;
  n1: number;
  n2: number;
  has_ties: boolean;
}

/** Wilcoxon 符号秩检验。 */
export interface WilcoxonSignedRankResult {
  n: number;
  w_positive: number;
  w_negative: number;
  z_score: number;
  p_value: number;
  has_ties: boolean;
  has_zeros: boolean;
}

/** Kruskal-Wallis 检验。 */
export interface KruskalWallisResult {
  group_stats: RankSummary[];
  h_statistic: number;
  df: number;
  p_value: number;
  has_ties: boolean;
  n: number;
  warning: string | null;
}

// ---------------------------------------------------------------------------
// 列联表检验
// ---------------------------------------------------------------------------

/** 卡方独立性检验。 */
export interface ChiSquareTest {
  row_labels: string[];
  col_labels: string[];
  observed: number[][];
  expected: number[][];
  row_totals: number[];
  col_totals: number[];
  n: number;
  chi_square: number;
  df: number;
  p_value: number;
}

/** Fisher 精确检验的备择假设。 */
export type Alternative = 'twosided' | 'less' | 'greater';

/** Fisher 精确检验。 */
export interface FisherExactTest {
  table: [[number, number], [number, number]];
  odds_ratio: number;
  p_value_two_sided: number;
  p_value_less: number;
  p_value_greater: number;
  n: number;
}

// ---------------------------------------------------------------------------
// 正态性检验
// ---------------------------------------------------------------------------

/** K-S 检验类型参数。 */
export type KsTestTypeParam =
  | { kind: 'onesample'; mean: number; std_dev: number }
  | { kind: 'lilliefors' };

/** Shapiro-Wilk 正态性检验。 */
export interface ShapiroWilkResult {
  n: number;
  w_statistic: number;
  p_value: number;
}

/** Kolmogorov-Smirnov 正态性检验。 */
export interface KolmogorovSmirnovResult {
  n: number;
  d_statistic: number;
  p_value: number;
  test: { OneSample?: { mean: number; std_dev: number }; Lilliefors?: null };
  p_is_approx: boolean;
}

// ---------------------------------------------------------------------------
// 相关分析
// ---------------------------------------------------------------------------

/** 相关方法。 */
export type CorrelationMethod = 'pearson' | 'spearman' | 'kendall';

/** 单个相关系数结果。 */
export interface CorrelationResult {
  coefficient: number;
  p_value: number;
}

/** 变量对相关结果。 */
export interface CorrelationPair {
  var1: string;
  var2: string;
  n: number;
  pearson: CorrelationResult | null;
  spearman: CorrelationResult | null;
  kendall: CorrelationResult | null;
}

/** 偏相关分析结果。 */
export interface PartialCorrelationResult {
  var1: string;
  var2: string;
  controlling_for: string[];
  coefficient: number;
  p_value: number;
  n: number;
  df: number;
  method: CorrelationMethod;
}

// ---------------------------------------------------------------------------
// 回归分析
// ---------------------------------------------------------------------------

/** 回归系数。 */
export interface Coefficient {
  name: string;
  estimate: number;
  std_error: number;
  t_statistic: number;
  p_value: number;
  ci_95: [number, number];
}

/** 线性回归结果。 */
export interface LinearRegressionResult {
  model_formula: string;
  n: number;
  r_squared: number;
  adj_r_squared: number;
  f_statistic: number;
  f_p_value: number;
  residuals_std_error: number;
  degrees_of_freedom: [number, number];
  coefficients: Coefficient[];
}

/** 逻辑回归系数。 */
export interface LogisticCoefficient {
  name: string;
  estimate: number;
  std_error: number;
  z_statistic: number;
  p_value: number;
  ci_95: [number, number];
  odds_ratio: number;
  odds_ratio_ci_95: [number, number];
}

/** 逻辑回归结果。 */
export interface LogisticRegressionResult {
  model_formula: string;
  dep_var: string;
  n: number;
  coefficients: LogisticCoefficient[];
  log_likelihood: number;
  null_log_likelihood: number;
  aic: number;
  null_deviance: number;
  residual_deviance: number;
  degrees_of_freedom: [number, number];
  mcfadden_r2: number;
  cox_snell_r2: number;
  iterations: number;
  converged: boolean;
}

/** 方差膨胀因子。 */
export interface VifResult {
  variable: string;
  vif: number;
  tolerance: number;
  r_squared: number;
}

// ---------------------------------------------------------------------------
// 多变量分析
// ---------------------------------------------------------------------------

/** PCA 分析矩阵。 */
export type PcaMatrix = 'correlation' | 'covariance';

/** 主成分。 */
export interface PcaComponent {
  eigenvalue: number;
  explained_variance_ratio: number;
  cumulative_variance_ratio: number;
  eigenvector: number[];
  loadings: number[];
}

/** 主成分分析结果。 */
export interface PcaResult {
  variables: string[];
  n: number;
  matrix: PcaMatrix;
  components: PcaComponent[];
  means: number[];
  stds: number[];
  total_variance: number;
}

/** 量表项目诊断。 */
export interface ItemStatistic {
  item: string;
  mean: number;
  std_dev: number;
  corrected_item_total_correlation: number;
  alpha_if_deleted: number;
}

/** 信度分析结果。 */
export interface ReliabilityResult {
  items: string[];
  n: number;
  n_cases: number;
  alpha: number;
  standardized_alpha: number;
  scale_mean: number;
  scale_variance: number;
  item_statistics: ItemStatistic[];
}

// ---------------------------------------------------------------------------
// 事后检验
// ---------------------------------------------------------------------------

/** 事后检验方法。 */
export type PostHocMethod = 'bonferroni' | 'tukey' | 'scheffe';

/** 单个事后检验比较。 */
export interface PostHocComparison {
  group1: string;
  group2: string;
  mean_difference: number;
  std_error: number;
  statistic: number;
  p_value: number;
  ci_95: [number, number];
}

/** 事后检验结果。 */
export interface PostHocResult {
  method: PostHocMethod;
  comparisons: PostHocComparison[];
  n_groups: number;
  ms_within: number;
  df_within: number;
}

// ---------------------------------------------------------------------------
// 析因方差分析
// ---------------------------------------------------------------------------

/** 析因 ANOVA 的离均差平方和类型（Type I 顺序 / Type II 边际）。 */
export type SsType = 'type_i' | 'type_ii';

/** 析因 ANOVA 表中单个效应（主效应 / 交互 / 误差）。 */
export interface AnovaEffect {
  source: string;
  ss: number;
  df: number;
  ms: number;
  f: number | null;
  p_value: number | null;
  eta_squared: number | null;
  partial_eta_squared: number | null;
}

/** 析因（多因素）方差分析结果。 */
export interface FactorialAnova {
  factors: string[];
  dependent_var: string;
  ss_type: SsType;
  effects: AnovaEffect[];
  r_squared: number;
  adj_r_squared: number;
  n: number;
}
