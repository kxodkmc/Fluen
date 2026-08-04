//! 两阶段流水线的 prompt 模板。
//!
//! - 阶段 1（Planning）：AI 阅读全文，query 去重，产出 `ExtractionPlan`
//! - 阶段 2（Execution）：按计划逐条创建 summary / concept / entity，最后建立 relations
//!
//! 允许 AI 输出自由文本（思考过程），但**以 tool_call 为执行凭证**。

use super::types::KnowledgeBuildOptions;

/// 长文献阈值（超过此长度先摘要预处理）。
pub const LONG_DOC_THRESHOLD: usize = 50_000;

/// 摘要预处理 prompt（长文献压缩）。
pub const SUMMARIZE_FOR_PLANNING_PROMPT: &str = r#"你是学术文献摘要助手。请将以下文献压缩为结构化摘要，要求：
- 保留核心研究问题、方法、关键概念、重要实体（人物/机构/项目）
- 丢弃细节论述与重复内容
- 输出 1500-3000 字的中文摘要
- 末尾列出文献中出现的 5-15 个核心概念和 3-10 个重要实体（仅列名称）

文献内容：
{md_content}
"#;

/// Planning 阶段 prompt 模板。
///
/// 占位符：
/// - `{max_concepts}`：概念数量上限
/// - `{max_entities}`：实体数量上限
/// - `{md_content}`：文献正文（或摘要）
pub const PLANNING_PROMPT: &str = r#"你是学术文献知识库的规划助手。阅读文献，产出结构化提取计划。

## 任务

1. 阅读文献全文，识别：
   - 核心研究问题与方法（用于 summary 的要点，3-5 条）
   - 关键学术概念（concept 候选，最多 {max_concepts} 个）
   - 重要人物/机构/项目（entity 候选，最多 {max_entities} 个）

2. **跨文献去重（强制）**：
   对每个 concept/entity 候选，必须先调用 `knowledge_query` 检索已有知识库。
   - 若返回结果中存在高度相似条目（title 语义匹配，或 score > 0.7）：
     在 `existing_id` 中填入该条目的 wiki_id，**不新建**
     若文献对该概念有新补充，在 `merge_supplement` 中说明
   - 若无相似条目：`existing_id` 留空，将新建

3. **输出格式**：调用 `submit_plan` 工具提交结构化计划，包含：
   - summary_points: 字符串数组（3-5 个要点）
   - concepts: 数组，每个元素含 title/brief/existing_id/merge_supplement
   - entities: 数组，同上

## 重要说明

- 你可以输出自由文本（思考过程），但最终必须调用 `submit_plan` 提交计划
- **不要在此阶段创建任何条目**（不调用 create_entry）
- 去重判断要严格：宁可合并到已有条目，也不要新建近似条目
  例如"机器学习"/"机器学习技术"/"ML"应合并到同一个 concept
- brief 字段要具体（用于第二阶段生成正文），不要仅复制标题

## 文献内容

{md_content}
"#;

/// 创建 summary 条目的 prompt。
pub const CREATE_SUMMARY_PROMPT: &str = r#"为以下文献创建综述页（summary）条目。

## 文献要点

{summary_points}

## 要求

- 调用 `knowledge_create_entry`，wiki_type=summary
- source 字段必须为 `raw/{ref_id}.pdf`
- title 用文献标题（若已知）或"文献综述-{ref_id}"
- content 写 200-400 字综述，涵盖研究问题、方法、结论
- 可输出自由文本，但必须以 create_entry 调用结束
"#;

/// 创建单个 concept 条目的 prompt。
pub const CREATE_CONCEPT_PROMPT: &str = r#"为以下概念创建知识库条目。

## 概念信息

- 标题：{title}
- 文献中的简述：{brief}

## 要求

- 调用 `knowledge_create_entry`，wiki_type=concept
- title 用概念全称（如"数智化技术"而非"数智化"）
- content 包含：概念定义 + 该文献中的具体应用/贡献
- 可输出自由文本，但必须以 create_entry 调用结束
"#;

/// 创建单个 entity 条目的 prompt。
pub const CREATE_ENTITY_PROMPT: &str = r#"为以下实体创建知识库条目。

## 实体信息

- 标题：{title}
- 文献中的简述：{brief}

## 要求

- 调用 `knowledge_create_entry`，wiki_type=entity
- title 用人物全名或机构全名
- content 包含：身份/性质 + 与该文献的关系/贡献
- **仅基于文献明确陈述的内容**，不得推断或幻觉
  若文献信息不足，在 content 中注明"待补充"
- 可输出自由文本，但必须以 create_entry 调用结束
"#;

/// 建立 relations 的 prompt（独立阶段，杜绝孤儿条目）。
pub const ESTABLISH_RELATIONS_PROMPT: &str = r#"为综述页建立关联关系。

## 输入

- summary 条目 ID：{summary_id}
- 需关联的条目 ID 列表：{related_ids}

## 要求

- 调用 `knowledge_edit_entry`，对 summary 条目添加 relations
- relations 包含所有 related_ids
- 这一步是必须的，不得跳过
"#;

/// 渲染 Planning prompt。
pub fn render_planning(md_content: &str, options: &KnowledgeBuildOptions) -> String {
    let max_concepts = options
        .max_concepts
        .unwrap_or(KnowledgeBuildOptions::DEFAULT_MAX);
    let max_entities = options
        .max_entities
        .unwrap_or(KnowledgeBuildOptions::DEFAULT_MAX);
    PLANNING_PROMPT
        .replace("{max_concepts}", &max_concepts.to_string())
        .replace("{max_entities}", &max_entities.to_string())
        .replace("{md_content}", md_content)
}

/// 渲染摘要预处理 prompt。
pub fn render_summarize(md_content: &str) -> String {
    SUMMARIZE_FOR_PLANNING_PROMPT.replace("{md_content}", md_content)
}

/// 渲染创建 summary 的 prompt。
pub fn render_create_summary(ref_id: &str, summary_points: &[String]) -> String {
    let points = summary_points
        .iter()
        .map(|p| format!("- {p}"))
        .collect::<Vec<_>>()
        .join("\n");
    CREATE_SUMMARY_PROMPT
        .replace("{ref_id}", ref_id)
        .replace("{summary_points}", &points)
}

/// 渲染创建 concept 的 prompt。
pub fn render_create_concept(title: &str, brief: &str) -> String {
    CREATE_CONCEPT_PROMPT
        .replace("{title}", title)
        .replace("{brief}", brief)
}

/// 渲染创建 entity 的 prompt。
pub fn render_create_entity(title: &str, brief: &str) -> String {
    CREATE_ENTITY_PROMPT
        .replace("{title}", title)
        .replace("{brief}", brief)
}

/// 渲染建立 relations 的 prompt。
pub fn render_establish_relations(summary_id: &str, related_ids: &[String]) -> String {
    let ids = related_ids.join(", ");
    ESTABLISH_RELATIONS_PROMPT
        .replace("{summary_id}", summary_id)
        .replace("{related_ids}", &ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_planning_substitutes_placeholders() {
        let opts = KnowledgeBuildOptions::default();
        let rendered = render_planning("文献正文", &opts);
        assert!(rendered.contains("最多 15 个"));
        assert!(rendered.contains("文献正文"));
        assert!(!rendered.contains("{max_concepts}"));
        assert!(!rendered.contains("{md_content}"));
    }

    #[test]
    fn render_create_summary_lists_points() {
        let rendered = render_create_summary(
            "ref-abc",
            &["要点1".into(), "要点2".into()],
        );
        assert!(rendered.contains("ref-abc"));
        assert!(rendered.contains("- 要点1"));
        assert!(rendered.contains("- 要点2"));
    }

    #[test]
    fn render_create_concept_substitutes() {
        let rendered = render_create_concept("机器学习", "一种数据驱动方法");
        assert!(rendered.contains("机器学习"));
        assert!(rendered.contains("数据驱动方法"));
    }

    #[test]
    fn render_establish_relations_joins_ids() {
        let rendered = render_establish_relations(
            "wiki-sum1",
            &["wiki-c1".into(), "wiki-e1".into()],
        );
        assert!(rendered.contains("wiki-sum1"));
        assert!(rendered.contains("wiki-c1, wiki-e1"));
    }
}
