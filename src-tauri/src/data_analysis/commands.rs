//! Tauri 命令——数据分析。
//!
//! ## 命令一览
//!
//! | 命令 | 返回 | 说明 |
//! |------|------|------|
//! | `data_load_dataset` | `DatasetSchema` | 按扩展名加载数据文件并返回 schema |
//! | `data_preview_rows` | `DatasetPreview` | 返回前 N 行显示值用于内容栏数据表格 |
//! | `data_list_datasets` | `Vec<DatasetEntry>` | 扫描项目 data 目录下已导入的 CSV 数据表 |
//! | `data_import_dataset` | `DatasetEntry` | 复制 CSV 到项目 data 目录对应类型子目录 |
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

/// 数据表条目（项目 `data/` 目录下的一份可分析数据）。
#[derive(Debug, Serialize)]
pub struct DatasetEntry {
    /// 文件绝对路径。
    pub path: String,
    /// 文件名（不含扩展名）。
    pub name: String,
    /// 数据类型：`questionnaire` / `experiment`。
    pub kind: String,
    /// 数据行数。
    pub n_rows: usize,
    /// 变量数量。
    pub n_vars: usize,
}

/// 数据在项目内的归档子目录。
fn kind_dir(kind: &str) -> Result<&'static str, DataAnalysisError> {
    match kind {
        "questionnaire" => Ok("questionnaires"),
        "experiment" => Ok("experiments"),
        other => Err(DataAnalysisError::InvalidInput(format!(
            "未知的数据类型: {other}"
        ))),
    }
}

/// 扫描项目 `data/questionnaires` 与 `data/experiments` 下的 CSV 数据表。
///
/// 无法解析的文件会被跳过，避免单个损坏文件阻塞整个列表。
#[tauri::command]
pub fn data_list_datasets(project_path: String) -> Result<Vec<DatasetEntry>, DataAnalysisError> {
    let data_root = PathBuf::from(&project_path).join("data");
    let mut out = Vec::new();

    for (sub, kind) in [("questionnaires", "questionnaire"), ("experiments", "experiment")] {
        let Ok(entries) = std::fs::read_dir(data_root.join(sub)) else {
            continue;
        };
        let mut files: Vec<std::fs::DirEntry> = entries.flatten().collect();
        files.sort_by_key(|e| e.file_name());
        for file in files {
            let path = file.path();
            let is_csv = path
                .extension()
                .map(|x| x.eq_ignore_ascii_case("csv"))
                .unwrap_or(false);
            if !is_csv {
                continue;
            }
            let Ok(ds) = socstat::read().auto(&path) else {
                continue;
            };
            out.push(DatasetEntry {
                path: path.to_string_lossy().to_string(),
                name: path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                kind: kind.to_string(),
                n_rows: ds.n_rows(),
                n_vars: ds.n_vars(),
            });
        }
    }

    Ok(out)
}

/// 导入 CSV 数据：复制到项目 `data/` 对应类型的子目录并返回条目。
///
/// 与目标目录内已有文件重名时自动追加 `-2`、`-3` 等后缀。
#[tauri::command]
pub fn data_import_dataset(
    project_path: String,
    source_path: String,
    kind: String,
) -> Result<DatasetEntry, DataAnalysisError> {
    let src = PathBuf::from(&source_path);
    if !src.exists() {
        return Err(DataAnalysisError::NotFound(source_path));
    }
    let is_csv = src
        .extension()
        .map(|x| x.eq_ignore_ascii_case("csv"))
        .unwrap_or(false);
    if !is_csv {
        return Err(DataAnalysisError::InvalidInput(
            "目前仅支持导入 CSV 文件".to_string(),
        ));
    }

    let dir = PathBuf::from(&project_path)
        .join("data")
        .join(kind_dir(&kind)?);
    std::fs::create_dir_all(&dir)?;

    let stem = src
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let mut dest = dir.join(format!("{stem}.csv"));
    let mut suffix = 2;
    while dest.exists() {
        dest = dir.join(format!("{stem}-{suffix}.csv"));
        suffix += 1;
    }

    std::fs::copy(&src, &dest)?;
    let ds = socstat::read().auto(&dest)?;

    Ok(DatasetEntry {
        path: dest.to_string_lossy().to_string(),
        name: dest
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        kind,
        n_rows: ds.n_rows(),
        n_vars: ds.n_vars(),
    })
}

/// 数据预览（前 N 行的显示值，供内容栏数据表格展示）。
#[derive(Debug, Serialize)]
pub struct DatasetPreview {
    /// 列名（与单元格顺序一致）。
    pub columns: Vec<String>,
    /// 行数据（格式化为显示字符串，缺失值为空串）。
    pub rows: Vec<Vec<String>>,
    /// 数据总行数。
    pub total_rows: usize,
    /// 是否仅返回了部分行。
    pub truncated: bool,
}

/// 返回数据集前 `limit` 行（默认 100，上限 1000）用于预览。
#[tauri::command]
pub fn data_preview_rows(
    path: String,
    limit: Option<usize>,
) -> Result<DatasetPreview, DataAnalysisError> {
    let ds = load(&path)?;
    let total = ds.n_rows();
    let n = limit.unwrap_or(100).min(1000).min(total);

    let columns: Vec<String> = ds.var_names().map(str::to_string).collect();
    let mut rows = Vec::with_capacity(n);
    for i in 0..n {
        let mut row = Vec::with_capacity(columns.len());
        for idx in 0..ds.n_vars() {
            let cell = ds
                .column(idx)
                .ok()
                .and_then(|c| c.get_value(i))
                .map(|v| v.display())
                .unwrap_or_default();
            row.push(cell);
        }
        rows.push(row);
    }

    Ok(DatasetPreview {
        columns,
        rows,
        total_rows: total,
        truncated: n < total,
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

/// Fisher 精确检验（socstat 一次返回双侧/小于/大于三个 p 值）。
#[tauri::command]
pub fn data_fisher_exact_test(
    path: String,
    var1: String,
    var2: String,
) -> Result<FisherExactTest, DataAnalysisError> {
    let ds = load(&path)?;
    Ok(ds.fisher_exact_test(&var1, &var2)?)
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
