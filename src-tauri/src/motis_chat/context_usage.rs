//! 上下文用量统计——Motis 每轮请求上下文的分类估算。
//!
//! 与引擎截断同源，复用 referee [`TokenEstimator`]（~1.5 字符/token 的
//! 保守启发式）对**本轮实际发送**的各部分分别估算 token 占用，产出
//! [`ContextUsageReport`] 经 `motis:context-usage` 事件推送前端，
//! 驱动「上下文容量」面板的分类拆解展示。
//!
//! ## 分类口径
//!
//! | key | 内容 |
//! |-----|------|
//! | `messages` | 回放的历史消息 + 本轮用户输入 |
//! | `system_prompt` | 基础系统提示词（身份 / 约束 / 任务 / 环境等段落） |
//! | `sub_agents` | 子智能体清单段 |
//! | `board` | 项目成果板段 |
//! | `tools` | 工具声明（name + description + parameters JSON Schema） |
//! | `output_reserved` | 模型最大输出预留（`max_output_tokens`，非估算） |
//!
//! 注意：这是**发送前的本地估算**；API 返回的真实 `prompt_tokens`
//! 经 `motis:finish` 事件单独送达，前端并列展示（vendor 不上报时缺省）。

use serde::Serialize;

use referee_ai::budget::TokenEstimator;
use referee_ai::provider::ToolDeclaration;

use super::prompt::{PromptSection, PromptSectionKind};
use crate::chat_bridge::HistoryMessage;

/// 分类 key——前端据此映射 i18n 文案。
pub mod category {
    /// 消息（历史 + 本轮输入）。
    pub const MESSAGES: &str = "messages";
    /// 基础系统提示词。
    pub const SYSTEM_PROMPT: &str = "system_prompt";
    /// 子智能体清单段。
    pub const SUB_AGENTS: &str = "sub_agents";
    /// 项目成果板段。
    pub const BOARD: &str = "board";
    /// 工具声明。
    pub const TOOLS: &str = "tools";
    /// 输出预留。
    pub const OUTPUT_RESERVED: &str = "output_reserved";
}

/// 单个分类的 token 占用。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextCategory {
    /// 分类 key（见 [`category`]）。
    pub key: &'static str,
    /// 估算 token 数（`output_reserved` 为配置值）。
    pub tokens: usize,
}

/// 一轮请求的上下文用量报告（`motis:context-usage` 事件 payload）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsageReport {
    /// 各分类占用（固定顺序，为 0 的类别由前端隐藏）。
    pub categories: Vec<ContextCategory>,
    /// 输入侧估算总量（不含 `output_reserved`）。
    pub estimated_prompt_tokens: usize,
    /// 当前模型上下文窗口。
    pub context_window: usize,
    /// 当前模型最大输出预留。
    pub max_output_tokens: usize,
}

/// 估算一段文本的 token 占用（[`TokenEstimator`] 的 usize 适配）。
fn est(text: &str) -> usize {
    TokenEstimator::estimate(text) as usize
}

/// 依据本轮实际发送内容构建上下文用量报告。
///
/// - `sections`：[`super::prompt::build_system_prompt_parts`] 的带类别分段
/// - `tool_declarations`：主会话工具注册表导出（工具不可用时为空）
/// - `history` / `message`：回放历史与本轮用户输入
/// - `context_window` / `max_output_tokens`：模型规格（`llm_chat` 解析）
pub fn build_report(
    sections: &[PromptSection],
    tool_declarations: &[ToolDeclaration],
    history: &[HistoryMessage],
    message: &str,
    context_window: usize,
    max_output_tokens: usize,
) -> ContextUsageReport {
    let sum_kind = |kind: PromptSectionKind| -> usize {
        sections
            .iter()
            .filter(|s| s.kind == kind)
            .map(|s| est(&s.text))
            .sum()
    };

    let messages: usize = history.iter().map(|h| est(&h.content)).sum::<usize>() + est(message);

    let tools: usize = tool_declarations
        .iter()
        .map(|d| {
            let params = serde_json::to_string(&d.parameters).unwrap_or_default();
            est(&d.name) + est(&d.description) + est(&params)
        })
        .sum();

    let system_prompt = sum_kind(PromptSectionKind::Base);
    let sub_agents = sum_kind(PromptSectionKind::SubAgents);
    let board = sum_kind(PromptSectionKind::Board);

    let categories = vec![
        ContextCategory {
            key: category::MESSAGES,
            tokens: messages,
        },
        ContextCategory {
            key: category::SYSTEM_PROMPT,
            tokens: system_prompt,
        },
        ContextCategory {
            key: category::SUB_AGENTS,
            tokens: sub_agents,
        },
        ContextCategory {
            key: category::BOARD,
            tokens: board,
        },
        ContextCategory {
            key: category::TOOLS,
            tokens: tools,
        },
        ContextCategory {
            key: category::OUTPUT_RESERVED,
            tokens: max_output_tokens,
        },
    ];

    let estimated_prompt_tokens = categories
        .iter()
        .filter(|c| c.key != category::OUTPUT_RESERVED)
        .map(|c| c.tokens)
        .sum();

    ContextUsageReport {
        categories,
        estimated_prompt_tokens,
        context_window,
        max_output_tokens,
    }
}

// ===========================================================================
// 测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motis_chat::prompt::PromptSectionKind;
    use referee_ai::provider::ToolDeclaration;

    fn section(kind: PromptSectionKind, text: &str) -> PromptSection {
        PromptSection {
            kind,
            text: text.into(),
        }
    }

    fn tool(name: &str, description: &str) -> ToolDeclaration {
        ToolDeclaration {
            name: name.into(),
            description: description.into(),
            parameters: serde_json::json!({"type": "object"}),
        }
    }

    #[test]
    fn report_sums_prompt_categories_excluding_output() {
        let sections = vec![
            section(PromptSectionKind::Base, "身份与约束文案"),
            section(PromptSectionKind::SubAgents, "子智能体清单"),
        ];
        let decls = vec![tool("paper_outline", "读取论文大纲")];
        let history = vec![
            HistoryMessage {
                role: "user".into(),
                content: "第一句话".into(),
            },
            HistoryMessage {
                role: "assistant".into(),
                content: "回复内容".into(),
            },
        ];
        let report = build_report(&sections, &decls, &history, "本轮输入", 128 * 1024, 16 * 1024);

        let get = |key: &str| {
            report
                .categories
                .iter()
                .find(|c| c.key == key)
                .map(|c| c.tokens)
                .unwrap()
        };
        assert!(get(category::MESSAGES) > 0);
        assert!(get(category::SYSTEM_PROMPT) > 0);
        assert!(get(category::SUB_AGENTS) > 0);
        assert_eq!(get(category::BOARD), 0);
        assert!(get(category::TOOLS) > 0);
        assert_eq!(get(category::OUTPUT_RESERVED), 16 * 1024);

        let prompt_sum = get(category::MESSAGES)
            + get(category::SYSTEM_PROMPT)
            + get(category::SUB_AGENTS)
            + get(category::BOARD)
            + get(category::TOOLS);
        assert_eq!(report.estimated_prompt_tokens, prompt_sum);
        assert_eq!(report.context_window, 128 * 1024);
    }

    #[test]
    fn empty_inputs_only_reserve_output_and_base() {
        let report = build_report(&[], &[], &[], "", 128 * 1024, 16 * 1024);
        assert_eq!(report.categories.len(), 6);
        // 空串也按 TokenEstimator 计 1 token/段（消息与工具均为空 → 0 段）
        assert_eq!(
            report
                .categories
                .iter()
                .find(|c| c.key == category::MESSAGES)
                .unwrap()
                .tokens,
            1
        );
        assert_eq!(
            report
                .categories
                .iter()
                .find(|c| c.key == category::TOOLS)
                .unwrap()
                .tokens,
            0
        );
    }
}
