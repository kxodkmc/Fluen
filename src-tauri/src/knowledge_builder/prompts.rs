//! 两阶段流水线的 prompt 模板。
//!
//! - 阶段 1（Planning）：AI 阅读全文 + Index 快照，query 去重，产出 `ExtractionPlan`
//! - 阶段 2（Execution）：每条创建前 L2 混合检索查重，按计划逐条创建 summary / concept / entity
//!
//! V2.1 新增：
//! - Planning 阶段注入 Index 快照（全局去重视图）
//! - Execution 阶段每条注入 L2 候选条目（更新 vs 新建决策）
//!
//! 允许 AI 输出自由文本（思考过程），但**以 tool_call 为执行凭证**。

use super::index_snapshot::IndexSnapshot;
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
/// - `{index_snapshot}`：Index 快照（全局去重视图，V2.1 新增）
/// - `{md_content}`：文献正文（或摘要）
pub const PLANNING_PROMPT: &str = r#"你是学术文献知识库的规划助手。阅读文献，产出结构化提取计划。

## 任务

1. 阅读文献全文，识别：
   - 核心研究问题与方法（用于 summary 的要点，3-5 条）
   - 关键学术概念（concept 候选，最多 {max_concepts} 个）
   - 重要人物/机构/项目（entity 候选，最多 {max_entities} 个）

2. **跨文献去重（强制）**：
   下方"已有知识库条目"列出当前知识库全部条目（[S]=综述/[C]=概念/[E]=实体/#=标签）。
   对每个 concept/entity 候选，**先对照快照判断是否已有相似条目**，
   再调用 `knowledge_query` 检索确认（避免快照与磁盘短暂不一致）。
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

## 已有知识库条目（Index 快照）

{index_snapshot}

## 文献内容

{md_content}
"#;

/// 创建 summary 条目的 prompt。
///
/// V2.1：注入 L2 候选条目（若已有相似综述，应合并而非新建）。
pub const CREATE_SUMMARY_PROMPT: &str = r#"为以下文献创建综述页（summary）条目。

## 文献要点

{summary_points}

## 候选条目（L2 检索结果）

{candidates}

## 要求

- 若候选中存在高度相似条目（title 语义匹配），**不要新建**，调用 `knowledge_edit_entry` 合并补充内容
- 否则调用 `knowledge_create_entry`，wiki_type=summary
- source 字段必须为 `raw/{ref_id}.pdf`
- title 用文献标题（若已知）或"文献综述-{ref_id}"
- content 写 200-400 字综述，涵盖研究问题、方法、结论
- 可输出自由文本，但必须以 create_entry 或 edit_entry 调用结束
"#;

/// 创建单个 concept 条目的 prompt。
///
/// V2.1：注入 L2 候选条目（若已有相似概念，应合并而非新建）。
pub const CREATE_CONCEPT_PROMPT: &str = r#"为以下概念创建知识库条目。

## 概念信息

- 标题：{title}
- 文献中的简述：{brief}

## 候选条目（L2 检索结果）

{candidates}

## 要求

- 若候选中存在高度相似条目（title 语义匹配），**不要新建**，调用 `knowledge_edit_entry` 合并补充内容
- 否则调用 `knowledge_create_entry`，wiki_type=concept
- title 用概念全称（如"数智化技术"而非"数智化"）
- content 包含：概念定义 + 该文献中的具体应用/贡献
- **禁止在 content 中写入 `## 关联页面` 区**：关联关系由后续阶段统一建立，AI 不得自行写入
- 可输出自由文本，但必须以 create_entry 或 edit_entry 调用结束
"#;

/// 创建单个 entity 条目的 prompt。
///
/// V2.1：注入 L2 候选条目。
pub const CREATE_ENTITY_PROMPT: &str = r#"为以下实体创建知识库条目。

## 实体信息

- 标题：{title}
- 文献中的简述：{brief}

## 候选条目（L2 检索结果）

{candidates}

## 要求

- 若候选中存在高度相似条目（title 语义匹配），**不要新建**，调用 `knowledge_edit_entry` 合并补充内容
- 否则调用 `knowledge_create_entry`，wiki_type=entity
- title 用人物全名或机构全名
- content 包含：身份/性质 + 与该文献的关系/贡献
- **仅基于文献明确陈述的内容**，不得推断或幻觉
  若文献信息不足，在 content 中注明"待补充"
- **禁止在 content 中写入 `## 关联页面` 区**：关联关系由后续阶段统一建立，AI 不得自行写入
- 可输出自由文本，但必须以 create_entry 或 edit_entry 调用结束
"#;

/// 为 concept/entity 建立关联关系的 prompt（独立阶段）。
///
/// 传入完整的「wikiID → 标题」对照表，让 AI 基于标题判断关联关系，
/// 但通过 `edit_entry` 工具用 wikiID 写入，确保链接格式合规。
pub const ESTABLISH_ENTRY_RELATIONS_PROMPT: &str = r#"为知识库条目建立关联关系。

## 条目清单

以下是本次构建的所有条目（wikiID → 类型 → 标题）：

{entries_table}

## 任务

1. 阅读各条目的标题，判断哪些条目之间存在语义关联（理论关系、应用关系、因果关系等）
2. 对每个需要建立关联的条目，调用 `knowledge_edit_entry`：
   - `wiki_id`：该条目的 wikiID
   - `add_relations`：关联对象的 wikiID 列表
3. 关联是双向的：若 A 应关联 B，则对 A 和 B 分别调用 edit_entry 互加对方

## 严格规则

- `add_relations` 中的每个元素**必须是 wikiID**（形如 `wiki-xxxxxxxxxxxxxxxx`）
- **严禁使用标题、描述或其他非 wikiID 文本**作为 relations
- 仅建立有真实语义关联的关系，不要随意关联所有条目
- 可输出自由文本（思考过程），但最终必须通过 `knowledge_edit_entry` 工具调用完成
"#;

/// 渲染 Planning prompt（V2.1：注入 Index 快照）。
pub fn render_planning(
    md_content: &str,
    options: &KnowledgeBuildOptions,
    snapshot: &IndexSnapshot,
) -> String {
    let max_concepts = options
        .max_concepts
        .unwrap_or(KnowledgeBuildOptions::DEFAULT_MAX);
    let max_entities = options
        .max_entities
        .unwrap_or(KnowledgeBuildOptions::DEFAULT_MAX);
    let snapshot_str = if snapshot.total_entries() == 0 && snapshot.tags.is_empty() {
        "（知识库为空，无需去重）".to_string()
    } else {
        snapshot.render()
    };
    PLANNING_PROMPT
        .replace("{max_concepts}", &max_concepts.to_string())
        .replace("{max_entities}", &max_entities.to_string())
        .replace("{index_snapshot}", &snapshot_str)
        .replace("{md_content}", md_content)
}

/// 渲染摘要预处理 prompt。
pub fn render_summarize(md_content: &str) -> String {
    SUMMARIZE_FOR_PLANNING_PROMPT.replace("{md_content}", md_content)
}

/// 渲染创建 summary 的 prompt（V2.1：注入 L2 候选）。
pub fn render_create_summary(
    ref_id: &str,
    summary_points: &[String],
    candidates: &[L2Candidate],
) -> String {
    let points = summary_points
        .iter()
        .map(|p| format!("- {p}"))
        .collect::<Vec<_>>()
        .join("\n");
    CREATE_SUMMARY_PROMPT
        .replace("{ref_id}", ref_id)
        .replace("{summary_points}", &points)
        .replace("{candidates}", &render_candidates(candidates))
}

/// 渲染创建 concept 的 prompt（V2.1：注入 L2 候选）。
pub fn render_create_concept(title: &str, brief: &str, candidates: &[L2Candidate]) -> String {
    CREATE_CONCEPT_PROMPT
        .replace("{title}", title)
        .replace("{brief}", brief)
        .replace("{candidates}", &render_candidates(candidates))
}

/// 渲染创建 entity 的 prompt（V2.1：注入 L2 候选）。
pub fn render_create_entity(title: &str, brief: &str, candidates: &[L2Candidate]) -> String {
    CREATE_ENTITY_PROMPT
        .replace("{title}", title)
        .replace("{brief}", brief)
        .replace("{candidates}", &render_candidates(candidates))
}

/// 渲染 concept/entity 关联建立的 prompt。
///
/// `entries` 为 (wikiID, 类型, 标题) 列表，渲染为对照表传入 prompt。
pub fn render_establish_entry_relations(entries: &[(String, &str, String)]) -> String {
    let table = entries
        .iter()
        .map(|(id, ty, title)| format!("- {}（{}）：{}", id, ty, title))
        .collect::<Vec<_>>()
        .join("\n");
    ESTABLISH_ENTRY_RELATIONS_PROMPT.replace("{entries_table}", &table)
}

// ---------------------------------------------------------------------------
// L2 候选渲染
// ---------------------------------------------------------------------------

/// L2 检索候选条目（紧凑表示，含 wiki_id 供 AI 合并调用）。
#[derive(Debug, Clone)]
pub struct L2Candidate {
    pub wiki_id: String,
    pub title: String,
    pub score: f64,
}

/// 渲染 L2 候选列表为紧凑文本。
///
/// 格式：`- wiki-id（score: 0.85）：标题`
/// 空列表输出"（无候选）"。
pub fn render_candidates(candidates: &[L2Candidate]) -> String {
    if candidates.is_empty() {
        return "（无候选）".to_string();
    }
    candidates
        .iter()
        .map(|c| format!("- {}（score: {:.2}）：{}", c.wiki_id, c.score, c.title))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_planning_substitutes_placeholders() {
        let opts = KnowledgeBuildOptions::default();
        let snap = IndexSnapshot {
            concepts: vec!["机器学习".into()],
            ..Default::default()
        };
        let rendered = render_planning("文献正文", &opts, &snap);
        assert!(rendered.contains("最多 15 个"));
        assert!(rendered.contains("文献正文"));
        assert!(rendered.contains("- [C] 机器学习"));
        assert!(!rendered.contains("{max_concepts}"));
        assert!(!rendered.contains("{md_content}"));
        assert!(!rendered.contains("{index_snapshot}"));
    }

    #[test]
    fn render_planning_empty_snapshot() {
        let opts = KnowledgeBuildOptions::default();
        let snap = IndexSnapshot::default();
        let rendered = render_planning("文献正文", &opts, &snap);
        assert!(rendered.contains("（知识库为空，无需去重）"));
    }

    #[test]
    fn render_create_summary_lists_points_and_candidates() {
        let candidates = vec![L2Candidate {
            wiki_id: "wiki-abc".into(),
            title: "已有综述".into(),
            score: 0.85,
        }];
        let rendered = render_create_summary("ref-abc", &["要点1".into(), "要点2".into()], &candidates);
        assert!(rendered.contains("ref-abc"));
        assert!(rendered.contains("- 要点1"));
        assert!(rendered.contains("- 要点2"));
        assert!(rendered.contains("wiki-abc"));
        assert!(rendered.contains("已有综述"));
    }

    #[test]
    fn render_create_concept_substitutes_with_candidates() {
        let candidates = vec![L2Candidate {
            wiki_id: "wiki-xyz".into(),
            title: "机器学习".into(),
            score: 0.92,
        }];
        let rendered = render_create_concept("机器学习", "一种数据驱动方法", &candidates);
        assert!(rendered.contains("机器学习"));
        assert!(rendered.contains("数据驱动方法"));
        assert!(rendered.contains("wiki-xyz"));
    }

    #[test]
    fn render_create_entity_with_empty_candidates() {
        let rendered = render_create_entity("张三", "教授", &[]);
        assert!(rendered.contains("张三"));
        assert!(rendered.contains("（无候选）"));
    }

    #[test]
    fn render_establish_entry_relations_builds_table() {
        let entries = vec![
            ("wiki-sum1".into(), "summary", "文献综述".into()),
            ("wiki-c1".into(), "concept", "父母粗暴养育".into()),
            ("wiki-e1".into(), "entity", "张三".into()),
        ];
        let rendered = render_establish_entry_relations(&entries);
        assert!(rendered.contains("wiki-sum1（summary）：文献综述"));
        assert!(rendered.contains("wiki-c1（concept）：父母粗暴养育"));
        assert!(rendered.contains("wiki-e1（entity）：张三"));
    }

    #[test]
    fn render_candidates_empty_returns_placeholder() {
        assert_eq!(render_candidates(&[]), "（无候选）");
    }

    #[test]
    fn render_candidates_formats_with_score() {
        let candidates = vec![
            L2Candidate {
                wiki_id: "wiki-abc".into(),
                title: "机器学习".into(),
                score: 0.85,
            },
            L2Candidate {
                wiki_id: "wiki-def".into(),
                title: "深度学习".into(),
                score: 0.72,
            },
        ];
        let rendered = render_candidates(&candidates);
        assert!(rendered.contains("wiki-abc（score: 0.85）：机器学习"));
        assert!(rendered.contains("wiki-def（score: 0.72）：深度学习"));
    }
}
