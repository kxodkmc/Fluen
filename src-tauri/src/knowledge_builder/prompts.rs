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
   下方"已有知识库条目"列出当前知识库全部条目（[S]=综述/[C]=概念/[E]=实体）。
   对每个 concept/entity 候选，**先对照快照判断是否已有相似条目**，
   再调用 `knowledge_query` 检索确认（避免快照与磁盘短暂不一致）。
   **慎用 `knowledge_query_batch`**：其返回包含条目全量内容，单次即可注入数万 token
   永久占用会话历史；仅在确有多组关键词需一次核对时使用，且全程不超过 1 次，
   其余去重确认一律用 `knowledge_query`。
   - 若返回结果中存在高度相似条目（title 语义匹配，或 score > 0.7）：
     在 `existing_id` 中填入该条目的 wiki_id，**不新建**
     若文献对该概念有新补充，在 `merge_supplement` 中说明
   - 若无相似条目：`existing_id` 留空，将新建

3. **输出格式**：调用 `submit_plan` 工具提交结构化计划，包含：
   - summary_points: 字符串数组（3-5 个要点）
   - concepts: 数组，每个元素含 title/brief/existing_id/merge_supplement
   - entities: 数组，同上
   - **必填约束**：concepts 和 entities 的每个元素都必须提供 `title` 与 `brief` 两个必填字段。
     缺任一字段（例如只写 title 没有 brief）会导致整个计划解析失败，你必须补全后重新提交

## 重要说明

- 你可以输出自由文本（思考过程），但最终必须调用 `submit_plan` 提交计划
- **不要在此阶段创建任何条目**（不调用 create_entry）
- 去重判断要严格：宁可合并到已有条目，也不要新建近似条目
  例如"机器学习"/"机器学习技术"/"ML"应合并到同一个 concept
- brief 字段要具体（用于第二阶段生成正文），不要仅复制标题
- **原文优先提取**：summary_points 与 brief 应尽量保留文献原文的关键表述（术语/界定/数据/原句），供执行阶段按"原文优先"组织条目；原文有明确出处时优先摘录原意，避免仅凭印象泛泛概括
- 提交前自查：concepts 与 entities 中每个元素均包含 title 和 brief，JSON 结构完整无缺漏

## 已有知识库条目（Index 快照）

{index_snapshot}

## 文献内容

{md_content}
"#;

/// 创建 summary 条目的 prompt。
///
/// V2.1：注入 L2 候选条目（若已有相似综述，应合并而非新建）。
/// 溯源（新库契约）：body 全文用 `<{ref_id}>…</{ref_id}>` 单源包裹。
pub const CREATE_SUMMARY_PROMPT: &str = r#"为以下文献创建综述页（summary）条目。

## 文献要点

{summary_points}

## 候选条目（L2 检索结果）

{candidates}

## 要求

- 若候选中存在高度相似条目（title 语义匹配），**不要新建**，调用 `knowledge_edit_entry` 合并补充内容
  - 合并必须**在一次 `knowledge_edit_entry` 调用中提交全部 ops**，禁止拆分为多次调用
  - 追加的每个内容片段必须自带 `<{ref_id}>…</{ref_id}>` 溯源包裹
- 否则调用 `knowledge_create_entry`，参数：type=summary、title、body、source
- source 字段必须为 `{ref_id}`（新知识库直接使用 refID，不再带 raw/ 前缀与扩展名）
- title 用文献标题（若已知）或"文献综述-{ref_id}"
- body 写 200-400 字综述，涵盖研究问题、方法、结论
- **溯源（强制）**：body 全文用 `<{ref_id}>` 开头、`</{ref_id}>` 结尾包裹（单来源）；
  合并路径下追加的内容片段同样必须用 `<{ref_id}>…</{ref_id}>` 包裹
- **原文优先（忠实科学）**：综述文字以文献原文为基准——凡原文已明确的概念界定、方法、数据与结论，**优先沿用原文表述/关键句子/术语**，必要处裁剪、衔接、润色以保证通顺；确需概括时再用自己的话小结；**不得**编造或歪曲原文没有的内容，不得为"像综述"而改写原意
- 可输出自由文本，但**必须以一次 `knowledge_create_entry` 或 `knowledge_edit_entry` 工具调用结束**：
  - 候选中无相似条目 → 调用 `knowledge_create_entry`（type=summary）
  - 候选中存在相似条目 → 调用 `knowledge_edit_entry`（id=候选条目 wikiID，ops 数组用 insert_after/search_replace）
  - 二选一，必须调用其一；回复若没有这两个工具调用，本轮将被判定失败
"#;

/// 创建单个 concept 条目的 prompt。
///
/// V2.1：注入 L2 候选条目（若已有相似概念，应合并而非新建）。
/// 溯源（新库契约）：body 用 `<{ref_id}>` 标注来源（允许未溯源片段，lint 仅警告）。
pub const CREATE_CONCEPT_PROMPT: &str = r#"为以下概念创建知识库条目。

## 概念信息

- 标题：{title}
- 文献中的简述：{brief}

## 候选条目（L2 检索结果）

{candidates}

## 要求

- 若候选中存在高度相似条目（title 语义匹配），**不要新建**，调用 `knowledge_edit_entry` 合并补充内容
  - 合并必须**在一次 `knowledge_edit_entry` 调用中提交全部 ops**，禁止拆分为多次调用
- 否则调用 `knowledge_create_entry`，参数：type=concept、title、body
- title 用概念全称（如"数智化技术"而非"数智化"）
- body 包含：概念定义 + 该文献中的具体应用/贡献
- **溯源**：来自本文献的内容用 `<{ref_id}>…</{ref_id}>` 包裹标注来源
- **原文优先**：概念界定与应用尽量沿用文献原文的表述（优先原句/术语），可润色衔接，不得偏离或编造
- **禁止在 body 中写入 `## 关联页面` 区**：关联关系由后续阶段统一建立，AI 不得自行写入
- 可输出自由文本，但**必须以一次 `knowledge_create_entry` 或 `knowledge_edit_entry` 工具调用结束**（二选一，不可漏调）
"#;

/// 创建单个 entity 条目的 prompt。
///
/// V2.1：注入 L2 候选条目。
/// 溯源（新库契约）：body 用 `<{ref_id}>` 标注来源（允许未溯源片段，lint 仅警告）。
pub const CREATE_ENTITY_PROMPT: &str = r#"为以下实体创建知识库条目。

## 实体信息

- 标题：{title}
- 文献中的简述：{brief}

## 候选条目（L2 检索结果）

{candidates}

## 要求

- 若候选中存在高度相似条目（title 语义匹配），**不要新建**，调用 `knowledge_edit_entry` 合并补充内容
  - 合并必须**在一次 `knowledge_edit_entry` 调用中提交全部 ops**，禁止拆分为多次调用
- 否则调用 `knowledge_create_entry`，参数：type=entity、title、body
- title 用人物全名或机构全名
- body 包含：身份/性质 + 与该文献的关系/贡献
- **溯源**：来自本文献的内容用 `<{ref_id}>…</{ref_id}>` 包裹标注来源
- **原文优先、忠实原意**：身份与贡献尽量沿用文献原文表述（优先原句/术语）；仅基于文献明确陈述的内容，不得推断或幻觉
  若文献信息不足，在 body 中注明"待补充"
- **禁止在 body 中写入 `## 关联页面` 区**：关联关系由后续阶段统一建立，AI 不得自行写入
- 可输出自由文本，但**必须以一次 `knowledge_create_entry` 或 `knowledge_edit_entry` 工具调用结束**（二选一，不可漏调）
"#;

/// 为 concept/entity 建立关联关系的 prompt（独立阶段）。
///
/// 传入完整的「wikiID → 标题」对照表，让 AI 基于标题判断语义关联，
/// 通过 `submit_relations` 工具结构化提交（新库 MCP 无 add_relations，
/// 由 Rust 端经 merge 确定性落库并自动补双向）。
pub const ESTABLISH_ENTRY_RELATIONS_PROMPT: &str = r#"为知识库条目建立关联关系。

## 条目清单

以下是本次构建的所有条目（wikiID → 类型 → 标题）：

{entries_table}

## 任务

1. 阅读各条目的标题与类型，判断哪些条目之间存在语义关联（理论关系、应用关系、因果关系等）
2. 调用 `submit_relations` 一次性提交全部关联，每个元素含：
   - `from`：起始条目 wikiID
   - `to`：目标条目 wikiID
   - `predicate`：可选谓词（默认 related；也可用更精确的词，如"应用了""基于""属于"，仅字母/数字/下划线）
3. **无需双向提交**：系统会自动补反向关联，只需提交每个方向一次

## 严格规则

- `from` / `to` **必须是 wikiID**（形如 `wiki-xxxxxxxxxxxxxxxx`），严禁使用标题或描述文本
- 仅建立有真实语义关联的关系，不要随意关联所有条目
- 可输出自由文本（思考过程），但最终必须通过 `submit_relations` 工具调用完成
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
    let snapshot_str = if snapshot.total_entries() == 0 {
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

/// 渲染创建 concept 的 prompt（V2.1：注入 L2 候选；含溯源 refID）。
pub fn render_create_concept(
    ref_id: &str,
    title: &str,
    brief: &str,
    candidates: &[L2Candidate],
) -> String {
    CREATE_CONCEPT_PROMPT
        .replace("{ref_id}", ref_id)
        .replace("{title}", title)
        .replace("{brief}", brief)
        .replace("{candidates}", &render_candidates(candidates))
}

/// 渲染创建 entity 的 prompt（V2.1：注入 L2 候选；含溯源 refID）。
pub fn render_create_entity(
    ref_id: &str,
    title: &str,
    brief: &str,
    candidates: &[L2Candidate],
) -> String {
    CREATE_ENTITY_PROMPT
        .replace("{ref_id}", ref_id)
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
        let rendered = render_create_concept("ref-abc", "机器学习", "一种数据驱动方法", &candidates);
        assert!(rendered.contains("机器学习"));
        assert!(rendered.contains("数据驱动方法"));
        assert!(rendered.contains("wiki-xyz"));
        assert!(rendered.contains("ref-abc"));
    }

    #[test]
    fn render_create_entity_with_empty_candidates() {
        let rendered = render_create_entity("ref-abc", "张三", "教授", &[]);
        assert!(rendered.contains("张三"));
        assert!(rendered.contains("（无候选）"));
        assert!(rendered.contains("ref-abc"));
    }

    #[test]
    fn create_prompts_require_provenance_and_new_tool_args() {
        let summary = render_create_summary("ref-abc", &["要点".to_string()], &[]);
        assert!(!summary.contains("{ref_id}"), "占位符必须被替换");
        assert!(summary.contains("<ref-abc>"));
        assert!(summary.contains("source"));
        let concept = render_create_concept("ref-abc", "概念", "简述", &[]);
        assert!(concept.contains("<ref-abc>"));
        // MCP 契约：参数为 type/title/body（非 wiki_type/content）
        assert!(concept.contains("type=concept"));
        assert!(concept.contains("body"));
        assert!(!concept.contains("wiki_type"));
        assert!(!concept.contains("content 写"));
    }

    #[test]
    fn establish_relations_prompt_uses_submit_relations() {
        let entries = vec![("wiki-sum1".into(), "summary", "文献综述".into())];
        let rendered = render_establish_entry_relations(&entries);
        assert!(rendered.contains("submit_relations"));
        assert!(!rendered.contains("add_relations"));
        assert!(rendered.contains("无需双向提交"));
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
