//! 上下文预算计算（纯计算，无状态）。
//!
//! 基于 LLM 配置的 `context_window` 计算会话可用的历史预算，
//! 判断是否启用跨论文会话复用（依赖模型前缀缓存）。
//!
//! ## 关键参数
//!
//! - `MIN_CONTEXT_FOR_CACHING = 128_000`：硬门槛，低于此值不启用会话复用
//! - `MAX_HISTORY_RATIO = 0.75`：历史占用比例上限
//! - 输出预留 16K（应对单次大输出）
//! - 系统开销 2K（system prompt + 工具定义）
//!
//! ## 真实 usage 跟踪
//!
//! `history_used` 不再累加估算的输入 tokens，而是取会话最后一次 LLM 调用的
//! `usage.input_tokens + usage.output_tokens`。LLM API 的 `usage.input_tokens`
//! 已包含完整历史（system + 所有历史轮次输入输出 + 当前轮输入），无需自行累加。

use crate::llm_config::model::{LlmConfig, SceneModelRef};

use super::error::KnowledgeBuilderError;

/// 启用会话复用的最小上下文窗口（128K）。
///
/// 低于此值时可用预算不足以容纳 2 篇论文（每篇实际 40-60K），
/// 缓存复用收益低于风险，回退到 V1 单篇独立模式。
pub const MIN_CONTEXT_FOR_CACHING: usize = 128_000;

/// 历史占用比例上限（75%）。
///
/// 1M 模型可用 750K，预留 250K 应对单次大输出。
pub const MAX_HISTORY_RATIO: f64 = 0.75;

/// 默认输出预留 tokens（应对单次 LLM 大输出）。
pub const DEFAULT_RESERVED_FOR_OUTPUT: usize = 16_000;

/// 默认系统开销 tokens（system prompt + 工具定义）。
pub const DEFAULT_SYSTEM_OVERHEAD: usize = 2_000;

/// 下一篇预估系数：`paper_md_tokens × 2.5` 覆盖输出、L2 候选注入、Planning 开销。
pub const NEXT_PAPER_ESTIMATE_FACTOR: f64 = 2.5;

/// 上下文预算配置。
///
/// 由 [`ContextBudget::from_config`] 从 LLM 配置解析，
/// 也可直接构造用于测试。
#[derive(Debug, Clone)]
pub struct ContextBudget {
    /// 模型上下文窗口大小（tokens）。
    pub model_context: usize,
    /// 输出预留 tokens（默认 16K）。
    pub reserved_for_output: usize,
    /// 系统开销 tokens（默认 2K）。
    pub system_overhead_tokens: usize,
}

impl ContextBudget {
    /// 从 LLM 配置与场景模型引用构造。
    ///
    /// 解析 `context_window`：若模型未声明则回退到默认 128K。
    pub fn from_config(
        llm: &LlmConfig,
        model_ref: &SceneModelRef,
    ) -> Result<Self, KnowledgeBuilderError> {
        let (provider, model_id) = super::llm_helper::resolve_kb_provider(llm, model_ref)?;
        let model = provider.find_model(&model_id).ok_or_else(|| {
            KnowledgeBuilderError::Config(format!(
                "模型 {} 在 provider {} 中不存在",
                model_id, provider.id
            ))
        })?;
        let model_context = model
            .context_window
            .map(|w| w as usize)
            .unwrap_or(MIN_CONTEXT_FOR_CACHING);
        Ok(Self {
            model_context,
            reserved_for_output: DEFAULT_RESERVED_FOR_OUTPUT,
            system_overhead_tokens: DEFAULT_SYSTEM_OVERHEAD,
        })
    }

    /// 直接构造（用于测试）。
    pub fn new(model_context: usize) -> Self {
        Self {
            model_context,
            reserved_for_output: DEFAULT_RESERVED_FOR_OUTPUT,
            system_overhead_tokens: DEFAULT_SYSTEM_OVERHEAD,
        }
    }

    /// 是否支持会话缓存复用（硬门槛：≥ 128K）。
    pub fn supports_caching(&self) -> bool {
        self.model_context >= MIN_CONTEXT_FOR_CACHING
    }

    /// 历史上下文可用预算：`model_context × 0.75 - 输出预留 - 系统开销`。
    ///
    /// 注：`history_used` 由 [`super::session::KnowledgeBuildSession`] 从真实 usage 跟踪，
    /// 已含 Index 快照与当前论文实际占用，不再扣除。
    pub fn history_budget(&self) -> usize {
        let fixed = self.reserved_for_output + self.system_overhead_tokens;
        ((self.model_context as f64) * MAX_HISTORY_RATIO) as usize - fixed
    }

    /// 下一篇预估占用：`paper_md_tokens × 2.5`。
    ///
    /// 2.5 倍系数覆盖输出、L2 候选注入、Planning 等开销。
    pub fn estimate_next_paper(paper_md_tokens: usize) -> usize {
        ((paper_md_tokens as f64) * NEXT_PAPER_ESTIMATE_FACTOR) as usize
    }

    /// 判断是否还能追加下一篇论文。
    ///
    /// 条件：支持缓存 && `history_used + 下一篇预估 ≤ history_budget`。
    pub fn can_append_paper(&self, history_used: usize, next_paper_md_tokens: usize) -> bool {
        if !self.supports_caching() {
            return false;
        }
        let next = Self::estimate_next_paper(next_paper_md_tokens);
        history_used + next <= self.history_budget()
    }
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self::new(MIN_CONTEXT_FOR_CACHING)
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_caching_threshold() {
        assert!(!ContextBudget::new(64_000).supports_caching());
        assert!(!ContextBudget::new(127_999).supports_caching());
        assert!(ContextBudget::new(128_000).supports_caching());
        assert!(ContextBudget::new(256_000).supports_caching());
    }

    #[test]
    fn history_budget_subtracts_fixed_overhead() {
        let budget = ContextBudget::new(128_000);
        // 128000 * 0.75 = 96000; 96000 - 16000 - 2000 = 78000
        assert_eq!(budget.history_budget(), 78_000);
    }

    #[test]
    fn history_budget_for_1m_context() {
        let budget = ContextBudget::new(1_000_000);
        // 1000000 * 0.75 = 750000; 750000 - 16000 - 2000 = 732000
        assert_eq!(budget.history_budget(), 732_000);
    }

    #[test]
    fn estimate_next_paper_uses_2_5_factor() {
        assert_eq!(ContextBudget::estimate_next_paper(10_000), 25_000);
        assert_eq!(ContextBudget::estimate_next_paper(20_000), 50_000);
    }

    #[test]
    fn can_append_paper_below_budget() {
        let budget = ContextBudget::new(256_000);
        // history_budget = 256000 * 0.75 - 18000 = 174000
        // history_used=50000, next_paper=20000 → next_estimated=50000
        // 50000 + 50000 = 100000 ≤ 174000 ✅
        assert!(budget.can_append_paper(50_000, 20_000));
    }

    #[test]
    fn can_append_paper_above_budget() {
        let budget = ContextBudget::new(256_000);
        // history_used=130000, next_paper=20000 → next_estimated=50000
        // 130000 + 50000 = 180000 > 174000 ❌
        assert!(!budget.can_append_paper(130_000, 20_000));
    }

    #[test]
    fn can_append_paper_rejects_small_context() {
        let budget = ContextBudget::new(64_000);
        // 不支持缓存，无论 history 多少都返回 false
        assert!(!budget.can_append_paper(0, 1_000));
    }

    #[test]
    fn default_uses_min_context() {
        let budget = ContextBudget::default();
        assert_eq!(budget.model_context, MIN_CONTEXT_FOR_CACHING);
        assert!(budget.supports_caching());
    }

    /// 验证示例表格（V2.1 文档第 102-109 行）。
    #[test]
    fn doc_example_table() {
        // 64K → 0 篇
        let b64 = ContextBudget::new(64_000);
        assert!(!b64.supports_caching());

        // 128K → history_budget=78000，单篇 50K 占用 → 可加 1 篇
        let b128 = ContextBudget::new(128_000);
        assert_eq!(b128.history_budget(), 78_000);
        assert!(b128.can_append_paper(0, 20_000)); // 第一篇 0 历史
        assert!(!b128.can_append_paper(50_000, 20_000)); // 已用 50K，无法再加

        // 1M → 732000，单篇 50K → 14 篇
        let b1m = ContextBudget::new(1_000_000);
        assert_eq!(b1m.history_budget(), 732_000);
        // 13 篇已用 13*50K=650K，再加 50K = 700K ≤ 732K ✅
        assert!(b1m.can_append_paper(650_000, 20_000));
        // 14 篇已用 700K，再加 50K = 750K > 732K ❌
        assert!(!b1m.can_append_paper(700_000, 20_000));
    }
}
