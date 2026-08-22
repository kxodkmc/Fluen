//! Tauri 命令——数据分析。
//!
//! ## 命令一览
//!
//! | 命令 | 返回 | 说明 |
//! |------|------|------|
//! | `data_load_dataset` | `DatasetSchema` | 按扩展名加载数据文件并返回 schema |
//! | `data_descriptive` | `Descriptive` | 数值变量的描述统计 |
//! | `data_frequencies` | `FrequencyTable` | 变量的频数表 |
//! | `data_crosstab` | `Crosstab` | 交叉表（列联表） |
//! | `data_independent_t_test` | `IndependentTTest` | 独立样本T检验 |
//! | `data_paired_t_test` | `PairedTTest` | 配对样本T检验 |
//! | `data_one_way_anova` | `OneWayAnova` | 单因素方差分析 |
//! | `data_chi_square_test` | `ChiSquareTest` | 卡方独立性检验 |
//! | `data_fisher_exact_test` | `FisherExactTest` | Fisher精确检验 |
//! | `data_mann_whitney_u_test` | `MannWhitneyUTest` | Mann-Whitney U检验 |
//! | `data_wilcoxon_signed_rank_test` | `WilcoxonSignedRankResult` | Wilcoxon符号秩检验 |
//! | `data_kruskal_wallis_test` | `KruskalWallisResult` | Kruskal-Wallis检验 |
//! | `data_shapiro_wilk` | `ShapiroWilkResult` | Shapiro-Wilk正态性检验 |
//! | `data_ks_normality_test` | `KolmogorovSmirnovResult` | K-S正态性检验 |
//! | `data_correlation` | `Vec<CorrelationPair>` | 多变量相关分析 |
//! | `data_correlation_pair` | `CorrelationPair` | 双变量相关分析 |
//! | `data_partial_correlation` | `PartialCorrelationResult` | 偏相关分析 |
//! | `data_regression` | `LinearRegressionResult` | 线性回归（OLS） |
//! | `data_logistic_regression` | `LogisticRegressionResult` | 逻辑回归 |
//! | `data_vif` | `Vec<VifResult>` | 方差膨胀因子（多重共线性诊断） |
//! | `data_pca` | `PcaResult` | 主成分分析 |
//! | `data_reliability` | `ReliabilityResult` | 信度分析（Cronbach's α） |
//! | `data_post_hoc` | `PostHocResult` | 事后检验（Bonferroni/Tukey/Scheffé） |
//! | `data_factorial_anova` | `FactorialAnova` | 析因（多因素）方差分析 |
//!
//! 本模块只做路径解析与统计调用，**不复制统计逻辑**。所有统计由
//! [`socstat`] 提供，结果类型已派生 `Serialize`，可直接作为 JSON 载荷
//! 返回前端。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use socstat::prelude::*;

use super::error::DataAnalysisError;

// ---------------------------------------------------------------------------
// Schema & helpers
// ---------------------------------------------------------------------------

/// 数据文件的 schema 摘要（供前端展示变量列表）。
#[derive(Debug, Serialize)]
pub struct DatasetSchema {
    /// 文件绝对路径。
    pub path: String,
    /// 数据行数（变量数一致的列长度）。
    pub n_rows: usize,
    /// 变量数量。
    pub n_vars: usize,
    /// 变量元信息。
    pub variables: Vec<VariableInfo>,
}

/// 单个变量的元信息。
#[derive(Debug, Serialize)]
pub struct VariableInfo {
    pub name: String,
    pub label: String,
    pub data_type: String,
    pub measure: String,
    pub n_valid: usize,
    pub n_missing: usize,
}

/// 从路径加载数据集（内部辅助，统一错误转换）。
fn load(path: &str) -> Result<Dataset, DataAnalysisError> {
    let path_buf = PathBuf::from(path);
    if !path_buf.exists() {
        return Err(DataAnalysisError::NotFound(path.to_string()));
    }
    Ok(socstat::read().auto(&path_buf)?)
}

// ---------------------------------------------------------------------------
// 枚举参数的前端友好包装
// ---------------------------------------------------------------------------

/// 前端传入的相关方法（字符串映射到枚举，避免 serde rename 复杂性）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CorrelationMethodParam {
    Pearson,
    Spearman,
    Kendall,
}

impl From<CorrelationMethodParam> for CorrelationMethod {
    fn from(v: CorrelationMethodParam) -> Self {
        match v {
            CorrelationMethodParam::Pearson => CorrelationMethod::Pearson,
            CorrelationMethodParam::Spearman => CorrelationMethod::Spearman,
            CorrelationMethodParam::Kendall => CorrelationMethod::Kendall,
        }
    }
}

/// Fisher 检验的备择假设。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlternativeParam {
    TwoSided,
    Less,
    Greater,
}

impl From<AlternativeParam> for Alternative {
    fn from(v: AlternativeParam) -> Self {
        match v {
            AlternativeParam::TwoSided => Alternative::TwoSided,
            AlternativeParam::Less => Alternative::Less,
            AlternativeParam::Greater => Alternative::Greater,
        }
    }
}

/// 事后检验方法。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PostHocMethodParam {
    Bonferroni,
    Tukey,
    Scheffe,
}

impl From<PostHocMethodParam> for PostHocMethod {
    fn from(v: PostHocMethodParam) -> Self {
        match v {
            PostHocMethodParam::Bonferroni => PostHocMethod::Bonferroni,
            PostHocMethodParam::Tukey => PostHocMethod::Tukey,
            PostHocMethodParam::Scheffe => PostHocMethod::Scheffe,
        }
    }
}

/// 析因 ANOVA 的离均差平方和类型（Type I 顺序 / Type II 边际）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SsTypeParam {
    TypeI,
    TypeII,
}

impl From<SsTypeParam> for SsType {
    fn from(v: SsTypeParam) -> Self {
        match v {
            SsTypeParam::TypeI => SsType::TypeI,
            SsTypeParam::TypeII => SsType::TypeII,
        }
    }
}

/// PCA 分析矩阵类型。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PcaMatrixParam {
    Correlation,
    Covariance,
}

impl From<PcaMatrixParam> for PcaMatrix {
    fn from(v: PcaMatrixParam) -> Self {
        match v {
            PcaMatrixParam::Correlation => PcaMatrix::Correlation,
            PcaMatrixParam::Covariance => PcaMatrix::Covariance,
        }
    }
}

/// K-S 正态性检验类型。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum KsTestTypeParam {
    /// 完全指定 N(mean, std_dev)。
    OneSample { mean: f64, std_dev: f64 },
    /// Lilliefors 变体（参数由样本估计）。
    Lilliefors,
}

impl From<KsTestTypeParam> for KsTestType {
    fn from(v: KsTestTypeParam) -> Self {
        match v {
            KsTestTypeParam::OneSample { mean, std_dev } => KsTestType::OneSample { mean, std_dev },
            KsTestTypeParam::Lilliefors => KsTestType::Lilliefors,
        }
    }
}

// ---------------------------------------------------------------------------
// 命令：数据加载
// ---------------------------------------------------------------------------

/// 加载数据文件（按扩展名自动识别 CSV / JSON / .sav）。
///
/// 数据文件位于项目 `data/` 目录下（experiments / questionnaires）。
#[tauri::command]
pub fn data_load_dataset(path: String) -> Result<DatasetSchema, DataAnalysisError> {
    let ds = load(&path)?;
    let n_rows = ds.n_rows();

    let variables = ds
        .variables()
        .iter()
        .map(|v| {
            let n_valid = ds.n_valid(&v.name).unwrap_or(0);
            VariableInfo {
                name: v.name.clone(),
                label: v.label.clone().unwrap_or_default(),
                data_type: format!("{:?}", v.data_type),
                measure: format!("{:?}", v.measure),
                n_valid,
                n_missing: n_rows.saturating_sub(n_valid),
            }
        })
        .collect();

    Ok(DatasetSchema {
        path,
        n_rows,
        n_vars: ds.n_vars(),
        variables,
    })
}

// ---------------------------------------------------------------------------
// 命令：描述统计 & 频数 & 交叉表
// ---------------------------------------------------------------------------

/// 数值变量的描述统计。
#[tauri::command]
pub fn data_descriptive(path: String, var: String) -> Result<Descriptive, DataAnalysisError> {
    let ds = load(&path)?;
    Ok(ds.descriptive(&var)?)
}

/// 变量的频数表。
#[tauri::command]
pub fn data_frequencies(path: String, var: String) -> Result<FrequencyTable, DataAnalysisError> {
    let ds = load(&path)?;
    Ok(ds.frequencies(&var)?)
}

/// 交叉表（列联表）。
#[tauri::command]
pub fn data_crosstab(
    path: String,
    row_var: String,
    col_var: String,
) -> Result<Crosstab, DataAnalysisError> {
    let ds = load(&path)?;
    Ok(ds.crosstab(&row_var, &col_var)?)
}

// ---------------------------------------------------------------------------
// 命令：均值比较（参数 & 非参数）
// ---------------------------------------------------------------------------

/// 独立样本T检验。
#[tauri::command]
pub fn data_independent_t_test(
    path: String,
    dep_var: String,
    group_var: String,
) -> Result<IndependentTTest, DataAnalysisError> {
    let ds = load(&path)?;
    Ok(ds.independent_t_test(&dep_var, &group_var)?)
}

/// 配对样本T检验。
#[tauri::command]
pub fn data_paired_t_test(
    path: String,
    var1: String,
    var2: String,
) -> Result<PairedTTest, DataAnalysisError> {
    let ds = load(&path)?;
    Ok(ds.paired_t_test(&var1, &var2)?)
}

/// 单因素方差分析。
#[tauri::command]
pub fn data_one_way_anova(
    path: String,
    dep_var: String,
    factor_var: String,
) -> Result<OneWayAnova, DataAnalysisError> {
    let ds = load(&path)?;
    Ok(ds.one_way_anova(&dep_var, &factor_var)?)
}

/// Mann-Whitney U 检验。
#[tauri::command]
pub fn data_mann_whitney_u_test(
    path: String,
    dep_var: String,
    group_var: String,
) -> Result<MannWhitneyUTest, DataAnalysisError> {
    let ds = load(&path)?;
    Ok(ds.mann_whitney_u_test(&dep_var, &group_var)?)
}

/// Wilcoxon 符号秩检验。
#[tauri::command]
pub fn data_wilcoxon_signed_rank_test(
    path: String,
    var1: String,
    var2: String,
) -> Result<WilcoxonSignedRankResult, DataAnalysisError> {
    let ds = load(&path)?;
    Ok(ds.wilcoxon_signed_rank_test(&var1, &var2)?)
}

/// Kruskal-Wallis 检验。
#[tauri::command]
pub fn data_kruskal_wallis_test(
    path: String,
    dep_var: String,
    factor_var: String,
) -> Result<KruskalWallisResult, DataAnalysisError> {
    let ds = load(&path)?;
    Ok(ds.kruskal_wallis_test(&dep_var, &factor_var)?)
}

// ---------------------------------------------------------------------------
// 命令：列联表检验
// ---------------------------------------------------------------------------

/// 卡方独立性检验。
#[tauri::command]
pub fn data_chi_square_test(
    path: String,
    var1: String,
    var2: String,
) -> Result<ChiSquareTest, DataAnalysisError> {
    let ds = load(&path)?;
    Ok(ds.chi_square_test(&var1, &var2)?)
}

/// Fisher 精确检验。
#[tauri::command]
pub fn data_fisher_exact_test(
    path: String,
    var1: String,
    var2: String,
    alternative: AlternativeParam,
) -> Result<FisherExactTest, DataAnalysisError> {
    let ds = load(&path)?;
    Ok(ds.fisher_exact_test(&var1, &var2, alternative.into())?)
}

// ---------------------------------------------------------------------------
// 命令：正态性检验
// ---------------------------------------------------------------------------

/// Shapiro-Wilk 正态性检验。
#[tauri::command]
pub fn data_shapiro_wilk(path: String, var: String) -> Result<ShapiroWilkResult, DataAnalysisError> {
    let ds = load(&path)?;
    Ok(ds.shapiro_wilk(&var)?)
}

/// Kolmogorov-Smirnov 正态性检验。
#[tauri::command]
pub fn data_ks_normality_test(
    path: String,
    var: String,
    test_type: KsTestTypeParam,
) -> Result<KolmogorovSmirnovResult, DataAnalysisError> {
    let ds = load(&path)?;
    Ok(ds.ks_normality_test(&var, test_type.into())?)
}

// ---------------------------------------------------------------------------
// 命令：相关分析
// ---------------------------------------------------------------------------

/// 多变量两两相关分析。
#[tauri::command]
pub fn data_correlation(
    path: String,
    vars: Vec<String>,
    method: CorrelationMethodParam,
) -> Result<Vec<CorrelationPair>, DataAnalysisError> {
    let ds = load(&path)?;
    let vars_ref: Vec<&str> = vars.iter().map(|s| s.as_str()).collect();
    Ok(ds.correlation(&vars_ref, method.into())?)
}

/// 双变量相关分析。
#[tauri::command]
pub fn data_correlation_pair(
    path: String,
    var1: String,
    var2: String,
    method: CorrelationMethodParam,
) -> Result<CorrelationPair, DataAnalysisError> {
    let ds = load(&path)?;
    Ok(ds.correlation_pair(&var1, &var2, method.into())?)
}

/// 偏相关分析（控制变量后的净相关）。
#[tauri::command]
pub fn data_partial_correlation(
    path: String,
    var1: String,
    var2: String,
    control_vars: Vec<String>,
    method: CorrelationMethodParam,
) -> Result<PartialCorrelationResult, DataAnalysisError> {
    let ds = load(&path)?;
    let controls_ref: Vec<&str> = control_vars.iter().map(|s| s.as_str()).collect();
    Ok(ds.partial_correlation(&var1, &var2, &controls_ref, method.into())?)
}

// ---------------------------------------------------------------------------
// 命令：回归分析
// ---------------------------------------------------------------------------

/// 线性回归（OLS）。
#[tauri::command]
pub fn data_regression(
    path: String,
    dep_var: String,
    indep_vars: Vec<String>,
) -> Result<LinearRegressionResult, DataAnalysisError> {
    let ds = load(&path)?;
    let indep_ref: Vec<&str> = indep_vars.iter().map(|s| s.as_str()).collect();
    Ok(ds.regression(&dep_var, &indep_ref)?)
}

/// 逻辑回归（二分类）。
#[tauri::command]
pub fn data_logistic_regression(
    path: String,
    dep_var: String,
    indep_vars: Vec<String>,
) -> Result<LogisticRegressionResult, DataAnalysisError> {
    let ds = load(&path)?;
    let indep_ref: Vec<&str> = indep_vars.iter().map(|s| s.as_str()).collect();
    Ok(ds.logistic_regression(&dep_var, &indep_ref)?)
}

/// 方差膨胀因子（多重共线性诊断）。
#[tauri::command]
pub fn data_vif(
    path: String,
    indep_vars: Vec<String>,
) -> Result<Vec<VifResult>, DataAnalysisError> {
    let ds = load(&path)?;
    let indep_ref: Vec<&str> = indep_vars.iter().map(|s| s.as_str()).collect();
    Ok(ds.vif(&indep_ref)?)
}

// ---------------------------------------------------------------------------
// 命令：多变量分析
// ---------------------------------------------------------------------------

/// 主成分分析。
#[tauri::command]
pub fn data_pca(
    path: String,
    vars: Vec<String>,
    matrix: PcaMatrixParam,
) -> Result<PcaResult, DataAnalysisError> {
    let ds = load(&path)?;
    let vars_ref: Vec<&str> = vars.iter().map(|s| s.as_str()).collect();
    Ok(ds.pca(&vars_ref, matrix.into())?)
}

/// 信度分析（Cronbach's α）。
#[tauri::command]
pub fn data_reliability(
    path: String,
    vars: Vec<String>,
) -> Result<ReliabilityResult, DataAnalysisError> {
    let ds = load(&path)?;
    let vars_ref: Vec<&str> = vars.iter().map(|s| s.as_str()).collect();
    Ok(ds.reliability(&vars_ref)?)
}

// ---------------------------------------------------------------------------
// 命令：事后检验
// ---------------------------------------------------------------------------

/// 事后检验（Bonferroni / Tukey HSD / Scheffé）。
#[tauri::command]
pub fn data_post_hoc(
    path: String,
    dep_var: String,
    factor_var: String,
    method: PostHocMethodParam,
) -> Result<PostHocResult, DataAnalysisError> {
    let ds = load(&path)?;
    Ok(ds.post_hoc(&dep_var, &factor_var, method.into())?)
}

// ---------------------------------------------------------------------------
// 命令：析因方差分析
// ---------------------------------------------------------------------------

/// 析因（多因素）方差分析。
///
/// 对 `dep_var` 在两个及以上 `factors` 上进行析因 ANOVA，支持 Type I
/// （顺序）与 Type II（边际）离均差平方和，报告主效应与两两交互效应。
#[tauri::command]
pub fn data_factorial_anova(
    path: String,
    dep_var: String,
    factors: Vec<String>,
    ss_type: SsTypeParam,
) -> Result<FactorialAnova, DataAnalysisError> {
    let ds = load(&path)?;
    let factors_ref: Vec<&str> = factors.iter().map(|s| s.as_str()).collect();
    Ok(ds.factorial_anova(&dep_var, &factors_ref, ss_type.into())?)
}
