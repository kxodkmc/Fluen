//! 论文审核角色的系统提示词 —— **占位实现**。
//!
//! 论文审核能力尚未开放（产品规划中）。本模块仅提供一个**明确的占位提示词**，
//! 使子智能体可被注册与枚举、前端可展示，但在启用后如实告知用户功能开发中，
//! 不做任何实际审核，避免误导。功能落地时直接替换 `system()` 实现即可。

use super::environment;

/// 占位引导 —— 明确告知审核能力尚未开放。
const REVIEW_ROLE: &str = "你是 Fluen 学术创作平台的**论文审核助手**。当前**论文审核功能仍在开发中，暂不可用**。

收到审核类任务时：如实告知该能力尚未开放，并简要说明预期功能范围（如下方 Roadmap），引导使用者关注已可用的撰写与思辨助手。**不要**模拟审核结论、不要虚假反馈。";

/// 预期能力范围 —— 作为占位说明，便于使用者了解规划。
const REVIEW_ROADMAP: &str = "## 预期能力范围（规划中，未实现）

- 结构完整性审查：章节齐全、逻辑连贯、论证闭环。
- 论证强度审查：前提-证据-推理链路是否成立，有无逻辑跳跃。
- 规范合规审查：fluen-markup v1.1 标签与 ID 前缀、引用真实性、参考文献者对应。

> 以上仅供功能预览。实现前此角色不产出审核意见。";

/// 组装论文审核角色的占位系统提示词。
pub fn system() -> String {
    [REVIEW_ROLE, REVIEW_ROADMAP, &environment()].join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_prompt_is_explicit_placeholder() {
        let s = system();
        assert!(s.contains("论文审核助手"));
        assert!(s.contains("开发中"));
        assert!(s.contains("暂不可用"));
        assert!(s.contains("运行环境"));
    }
}