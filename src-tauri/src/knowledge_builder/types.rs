//! 知识库构建相关的核心数据类型。
//!
//! 这些类型在 `task_queue` 与 `knowledge_builder::pipeline` 之间共享，
//! 用于描述构建选项、阶段状态机、断点续传信息与 AI 提取计划。

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// 构建选项
// ---------------------------------------------------------------------------

/// 知识库构建选项（用户可在 UI 中调整）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeBuildOptions {
    /// 是否创建综述页（summary）。
    #[serde(default = "default_true")]
    pub create_summary: bool,
    /// 是否创建概念页（concept）。
    #[serde(default = "default_true")]
    pub create_concepts: bool,
    /// 是否创建实体页（entity）。
    #[serde(default = "default_true")]
    pub create_entities: bool,
    /// 是否自动建立 summary ↔ concept/entity relations。
    #[serde(default = "default_true")]
    pub auto_relations: bool,
    /// 概念提取数量上限（避免 AI 失控）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concepts: Option<usize>,
    /// 实体提取数量上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_entities: Option<usize>,
}

impl Default for KnowledgeBuildOptions {
    fn default() -> Self {
        Self {
            create_summary: true,
            create_concepts: true,
            create_entities: true,
            auto_relations: true,
            max_concepts: Some(15),
            max_entities: Some(15),
        }
    }
}

impl KnowledgeBuildOptions {
    /// 默认数量上限。
    pub const DEFAULT_MAX: usize = 15;
}

fn default_true() -> bool {
    true
}

// ---------------------------------------------------------------------------
// 阶段状态机
// ---------------------------------------------------------------------------

/// 知识库构建阶段（状态机）。
///
/// 严格顺序：
/// `Planning` → `CreatingSummary` → `CreatingConcepts`
///            → `CreatingEntities` → `EstablishingRelations` → `Done`
///
/// 中断恢复时根据 `stage` 判断从哪一步继续，避免跳过 relations
/// 产生孤儿条目。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BuildStage {
    /// 规划阶段：AI 阅读全文，产出结构化提取计划。
    #[default]
    Planning,
    /// 创建 summary 条目。
    CreatingSummary,
    /// 创建 concept 条目（逐条）。
    CreatingConcepts,
    /// 创建 entity 条目（逐条）。
    CreatingEntities,
    /// 为 summary 建立 relations（独立阶段，杜绝孤儿）。
    EstablishingRelations,
    /// 全部完成。
    Done,
}

impl BuildStage {
    /// 转为字符串（用于事件 payload）。
    pub fn as_str(&self) -> &'static str {
        match self {
            BuildStage::Planning => "planning",
            BuildStage::CreatingSummary => "creating_summary",
            BuildStage::CreatingConcepts => "creating_concepts",
            BuildStage::CreatingEntities => "creating_entities",
            BuildStage::EstablishingRelations => "establishing_relations",
            BuildStage::Done => "done",
        }
    }
}

// ---------------------------------------------------------------------------
// 断点续传
// ---------------------------------------------------------------------------

/// 知识库构建任务的 checkpoint（阶段化状态机）。
///
/// 持久化到 `task-queue.json` 中 `TaskRecord.checkpoint` 字段。
/// 中断恢复时从对应阶段继续执行，已创建的条目 ID 不会重复创建。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KnowledgeBuildCheckpoint {
    /// 当前阶段（恢复入口）。
    #[serde(default)]
    pub stage: BuildStage,
    /// 规划阶段产出的提取计划（Planning 完成后填充）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<ExtractionPlan>,
    /// summary 条目 ID（CreatingSummary 完成后填充）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_id: Option<String>,
    /// 已创建的 concept 条目 ID 列表（按 plan 顺序）。
    #[serde(default)]
    pub concept_ids: Vec<String>,
    /// 已创建的 entity 条目 ID 列表。
    #[serde(default)]
    pub entity_ids: Vec<String>,
    /// relations 是否已建立（EstablishingRelations 完成后置 true）。
    #[serde(default)]
    pub relations_established: bool,
    /// AI 对话已消耗的 token 数（成本追踪）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_used: Option<u32>,
}

// ---------------------------------------------------------------------------
// AI 提取计划
// ---------------------------------------------------------------------------

/// 规划阶段产出的提取计划（AI 结构化输出）。
///
/// 在 Planning 阶段，AI 阅读全文后输出此计划。计划持久化到 checkpoint，
/// 执行阶段按计划逐条创建，中断后无需重新规划。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtractionPlan {
    /// summary 的要点（3-5 个，用于第二阶段创建 summary 条目）。
    #[serde(default)]
    pub summary_points: Vec<String>,
    /// concept 候选列表（已去重）。
    #[serde(default)]
    pub concepts: Vec<PlannedEntry>,
    /// entity 候选列表（已去重）。
    #[serde(default)]
    pub entities: Vec<PlannedEntry>,
}

/// 计划中的单个条目（concept 或 entity）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedEntry {
    /// 条目标题（concept 全称 / entity 全名）。
    pub title: String,
    /// 该条目在文献中的简述（用于 AI 第二阶段生成正文）。
    pub brief: String,
    /// 去重决策：
    /// - `Some(wiki_id)`：知识库已存在相似条目，**不新建**，仅建立 relation
    /// - `None`：新建条目
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub existing_id: Option<String>,
    /// 若 `existing_id` 为 Some，AI 应合并的补充内容（追加到已存在条目）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_supplement: Option<String>,
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_default_all_enabled() {
        let opts = KnowledgeBuildOptions::default();
        assert!(opts.create_summary);
        assert!(opts.create_concepts);
        assert!(opts.create_entities);
        assert!(opts.auto_relations);
        assert_eq!(opts.max_concepts, Some(15));
        assert_eq!(opts.max_entities, Some(15));
    }

    #[test]
    fn build_stage_default_is_planning() {
        assert_eq!(BuildStage::default(), BuildStage::Planning);
    }

    #[test]
    fn build_stage_as_str() {
        assert_eq!(BuildStage::Planning.as_str(), "planning");
        assert_eq!(BuildStage::Done.as_str(), "done");
    }

    #[test]
    fn checkpoint_default() {
        let ck = KnowledgeBuildCheckpoint::default();
        assert_eq!(ck.stage, BuildStage::Planning);
        assert!(ck.plan.is_none());
        assert!(ck.summary_id.is_none());
        assert!(ck.concept_ids.is_empty());
        assert!(!ck.relations_established);
    }

    #[test]
    fn checkpoint_roundtrip() {
        let ck = KnowledgeBuildCheckpoint {
            stage: BuildStage::CreatingConcepts,
            plan: Some(ExtractionPlan {
                summary_points: vec!["要点1".into()],
                concepts: vec![PlannedEntry {
                    title: "机器学习".into(),
                    brief: "一种数据驱动的方法".into(),
                    existing_id: Some("wiki-abc".into()),
                    merge_supplement: None,
                }],
                entities: vec![],
            }),
            summary_id: Some("wiki-sum1".into()),
            concept_ids: vec!["wiki-c1".into()],
            entity_ids: vec![],
            relations_established: false,
            tokens_used: Some(1500),
        };
        let json = serde_json::to_string(&ck).unwrap();
        let parsed: KnowledgeBuildCheckpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.stage, BuildStage::CreatingConcepts);
        assert_eq!(parsed.summary_id.as_deref(), Some("wiki-sum1"));
        assert_eq!(parsed.concept_ids, vec!["wiki-c1".to_string()]);
        let plan = parsed.plan.unwrap();
        assert_eq!(plan.concepts.len(), 1);
        assert_eq!(plan.concepts[0].existing_id.as_deref(), Some("wiki-abc"));
    }

    #[test]
    fn deserialize_old_checkpoint_missing_fields() {
        // 旧版本 checkpoint 仅有 stage 字段
        let json = r#"{"stage":"planning"}"#;
        let ck: KnowledgeBuildCheckpoint = serde_json::from_str(json).unwrap();
        assert_eq!(ck.stage, BuildStage::Planning);
        assert!(ck.plan.is_none());
        assert!(ck.concept_ids.is_empty());
    }
}
