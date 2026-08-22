/**
 * 数据分析 composable。
 *
 * 封装对后端 `data_*` 命令的 invoke 调用，覆盖 socstat 全部分析能力：
 *   - 数据加载 / 描述统计 / 频数表 / 交叉表
 *   - 均值比较（T检验 / ANOVA / 非参数检验）
 *   - 列联表检验（卡方 / Fisher精确）
 *   - 正态性检验（Shapiro-Wilk / K-S）
 *   - 相关分析（Pearson / Spearman / Kendall / 偏相关）
 *   - 回归分析（线性 / 逻辑 / VIF）
 *   - 多变量分析（PCA / 信度）
 *   - 事后检验（Bonferroni / Tukey / Scheffé）
 *
 * @example
 * ```ts
 * const schema = await loadDataset('/path/to/data.csv');
 * const d = await descriptive(schema.path, 'income');
 * const t = await independentTTest(schema.path, 'income', 'gender');
 * const sw = await shapiroWilk(schema.path, 'income');
 * ```
 */

import { invoke } from '@tauri-apps/api/core';
import type {
  Alternative,
  CorrelationMethod,
  Crosstab,
  DatasetSchema,
  DescriptiveResult,
  FactorialAnova,
  FisherExactTest,
  FrequencyTable,
  IndependentTTest,
  KolmogorovSmirnovResult,
  KsTestTypeParam,
  KruskalWallisResult,
  LinearRegressionResult,
  LogisticRegressionResult,
  MannWhitneyUTest,
  OneWayAnova,
  PairedTTest,
  PcaMatrix,
  PcaResult,
  PartialCorrelationResult,
  PostHocMethod,
  PostHocResult,
  CorrelationPair,
  ReliabilityResult,
  ShapiroWilkResult,
  ChiSquareTest,
  SsType,
  VifResult,
  WilcoxonSignedRankResult,
} from '../types/dataAnalysis';

// ---------------------------------------------------------------------------
// 数据加载
// ---------------------------------------------------------------------------

/** 加载数据文件，返回 schema。 */
export async function loadDataset(path: string): Promise<DatasetSchema> {
  return invoke<DatasetSchema>('data_load_dataset', { path });
}

// ---------------------------------------------------------------------------
// 描述统计 & 频数 & 交叉表
// ---------------------------------------------------------------------------

/** 数值变量的描述统计。 */
export async function descriptive(
  path: string,
  varName: string,
): Promise<DescriptiveResult> {
  return invoke<DescriptiveResult>('data_descriptive', { path, var: varName });
}

/** 变量的频数表。 */
export async function frequencies(
  path: string,
  varName: string,
): Promise<FrequencyTable> {
  return invoke<FrequencyTable>('data_frequencies', { path, var: varName });
}

/** 交叉表（列联表）。 */
export async function crosstab(
  path: string,
  rowVar: string,
  colVar: string,
): Promise<Crosstab> {
  return invoke<Crosstab>('data_crosstab', {
    path,
    rowVar,
    colVar,
  });
}

// ---------------------------------------------------------------------------
// 均值比较（参数检验）
// ---------------------------------------------------------------------------

/** 独立样本T检验。 */
export async function independentTTest(
  path: string,
  depVar: string,
  groupVar: string,
): Promise<IndependentTTest> {
  return invoke<IndependentTTest>('data_independent_t_test', {
    path,
    depVar,
    groupVar,
  });
}

/** 配对样本T检验。 */
export async function pairedTTest(
  path: string,
  var1: string,
  var2: string,
): Promise<PairedTTest> {
  return invoke<PairedTTest>('data_paired_t_test', { path, var1, var2 });
}

/** 单因素方差分析。 */
export async function oneWayAnova(
  path: string,
  depVar: string,
  factorVar: string,
): Promise<OneWayAnova> {
  return invoke<OneWayAnova>('data_one_way_anova', {
    path,
    depVar,
    factorVar,
  });
}

// ---------------------------------------------------------------------------
// 均值比较（非参数检验）
// ---------------------------------------------------------------------------

/** Mann-Whitney U 检验。 */
export async function mannWhitneyUTest(
  path: string,
  depVar: string,
  groupVar: string,
): Promise<MannWhitneyUTest> {
  return invoke<MannWhitneyUTest>('data_mann_whitney_u_test', {
    path,
    depVar,
    groupVar,
  });
}

/** Wilcoxon 符号秩检验。 */
export async function wilcoxonSignedRankTest(
  path: string,
  var1: string,
  var2: string,
): Promise<WilcoxonSignedRankResult> {
  return invoke<WilcoxonSignedRankResult>('data_wilcoxon_signed_rank_test', {
    path,
    var1,
    var2,
  });
}

/** Kruskal-Wallis 检验。 */
export async function kruskalWallisTest(
  path: string,
  depVar: string,
  factorVar: string,
): Promise<KruskalWallisResult> {
  return invoke<KruskalWallisResult>('data_kruskal_wallis_test', {
    path,
    depVar,
    factorVar,
  });
}

// ---------------------------------------------------------------------------
// 列联表检验
// ---------------------------------------------------------------------------

/** 卡方独立性检验。 */
export async function chiSquareTest(
  path: string,
  var1: string,
  var2: string,
): Promise<ChiSquareTest> {
  return invoke<ChiSquareTest>('data_chi_square_test', { path, var1, var2 });
}

/** Fisher 精确检验。 */
export async function fisherExactTest(
  path: string,
  var1: string,
  var2: string,
  alternative: Alternative = 'twosided',
): Promise<FisherExactTest> {
  return invoke<FisherExactTest>('data_fisher_exact_test', {
    path,
    var1,
    var2,
    alternative,
  });
}

// ---------------------------------------------------------------------------
// 正态性检验
// ---------------------------------------------------------------------------

/** Shapiro-Wilk 正态性检验。 */
export async function shapiroWilk(
  path: string,
  varName: string,
): Promise<ShapiroWilkResult> {
  return invoke<ShapiroWilkResult>('data_shapiro_wilk', {
    path,
    var: varName,
  });
}

/** Kolmogorov-Smirnov 正态性检验。 */
export async function ksNormalityTest(
  path: string,
  varName: string,
  testType: KsTestTypeParam,
): Promise<KolmogorovSmirnovResult> {
  return invoke<KolmogorovSmirnovResult>('data_ks_normality_test', {
    path,
    var: varName,
    testType,
  });
}

// ---------------------------------------------------------------------------
// 相关分析
// ---------------------------------------------------------------------------

/** 多变量两两相关分析。 */
export async function correlation(
  path: string,
  vars: string[],
  method: CorrelationMethod = 'pearson',
): Promise<CorrelationPair[]> {
  return invoke<CorrelationPair[]>('data_correlation', {
    path,
    vars,
    method,
  });
}

/** 双变量相关分析。 */
export async function correlationPair(
  path: string,
  var1: string,
  var2: string,
  method: CorrelationMethod = 'pearson',
): Promise<CorrelationPair> {
  return invoke<CorrelationPair>('data_correlation_pair', {
    path,
    var1,
    var2,
    method,
  });
}

/** 偏相关分析（控制变量后的净相关）。 */
export async function partialCorrelation(
  path: string,
  var1: string,
  var2: string,
  controlVars: string[],
  method: CorrelationMethod = 'pearson',
): Promise<PartialCorrelationResult> {
  return invoke<PartialCorrelationResult>('data_partial_correlation', {
    path,
    var1,
    var2,
    controlVars,
    method,
  });
}

// ---------------------------------------------------------------------------
// 回归分析
// ---------------------------------------------------------------------------

/** 线性回归（OLS）。 */
export async function regression(
  path: string,
  depVar: string,
  indepVars: string[],
): Promise<LinearRegressionResult> {
  return invoke<LinearRegressionResult>('data_regression', {
    path,
    depVar,
    indepVars,
  });
}

/** 逻辑回归（二分类）。 */
export async function logisticRegression(
  path: string,
  depVar: string,
  indepVars: string[],
): Promise<LogisticRegressionResult> {
  return invoke<LogisticRegressionResult>('data_logistic_regression', {
    path,
    depVar,
    indepVars,
  });
}

/** 方差膨胀因子（多重共线性诊断）。 */
export async function vif(
  path: string,
  indepVars: string[],
): Promise<VifResult[]> {
  return invoke<VifResult[]>('data_vif', { path, indepVars });
}

// ---------------------------------------------------------------------------
// 多变量分析
// ---------------------------------------------------------------------------

/** 主成分分析。 */
export async function pca(
  path: string,
  vars: string[],
  matrix: PcaMatrix = 'correlation',
): Promise<PcaResult> {
  return invoke<PcaResult>('data_pca', { path, vars, matrix });
}

/** 信度分析（Cronbach's α）。 */
export async function reliability(
  path: string,
  vars: string[],
): Promise<ReliabilityResult> {
  return invoke<ReliabilityResult>('data_reliability', { path, vars });
}

// ---------------------------------------------------------------------------
// 事后检验
// ---------------------------------------------------------------------------

/** 事后检验（Bonferroni / Tukey HSD / Scheffé）。 */
export async function postHoc(
  path: string,
  depVar: string,
  factorVar: string,
  method: PostHocMethod = 'bonferroni',
): Promise<PostHocResult> {
  return invoke<PostHocResult>('data_post_hoc', {
    path,
    depVar,
    factorVar,
    method,
  });
}

// ---------------------------------------------------------------------------
// 析因方差分析
// ---------------------------------------------------------------------------

/** 析因（多因素）方差分析。 */
export async function factorialAnova(
  path: string,
  depVar: string,
  factors: string[],
  ssType: SsType = 'type_ii',
): Promise<FactorialAnova> {
  return invoke<FactorialAnova>('data_factorial_anova', {
    path,
    depVar,
    factors,
    ssType,
  });
}
