use serde::{Deserialize, Serialize};

/// 知识库条目类型。
///
/// 对应 wiki.md 规范的三类条目：
/// - `Summary`：综述页，对应一篇文献的整体综述，`source` 字段指向 raw/ref-xxx.pdf
/// - `Concept`：概念页，学术概念/理论/方法的解释
/// - `Entity`：实体页，人物、机构、项目等
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WikiType {
    Summary,
    Concept,
    Entity,
}

impl WikiType {
    /// 转为目录名（concepts / entities / summaries）。
    pub fn dir(&self) -> &'static str {
        match self {
            WikiType::Summary => "summaries",
            WikiType::Concept => "concepts",
            WikiType::Entity => "entities",
        }
    }

    /// 从目录名解析。
    pub fn from_dir(dir: &str) -> Option<Self> {
        match dir {
            "summaries" => Some(WikiType::Summary),
            "concepts" => Some(WikiType::Concept),
            "entities" => Some(WikiType::Entity),
            _ => None,
        }
    }

    /// 转为字符串（用于 DB 存储与 JSON 序列化）。
    pub fn as_str(&self) -> &'static str {
        match self {
            WikiType::Summary => "summary",
            WikiType::Concept => "concept",
            WikiType::Entity => "entity",
        }
    }

    /// 从字符串解析。
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "summary" => Some(WikiType::Summary),
            "concept" => Some(WikiType::Concept),
            "entity" => Some(WikiType::Entity),
            _ => None,
        }
    }
}

impl std::fmt::Display for WikiType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 知识库条目（完整数据，含正文）。
///
/// **存储说明**：`authors` 仅存于 MD frontmatter，从 DB 读取时为空；
/// `content` 从 MD 文件读取，DB 不存储。如需完整数据，使用
/// [`crate::wiki::get_entry_full`]。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiEntry {
    /// 条目唯一 ID，格式 `wiki-xxxxxxxxxxxxxxxx`（16 位 UUID4）。
    pub id: String,
    /// 条目类型。
    pub wiki_type: WikiType,
    /// 条目标题。
    pub title: String,
    /// 文件相对路径（相对 references/），如 `wiki/concepts/wiki-xxx-标题.md`。
    pub file_path: String,
    /// 仅 summaries：源文献路径，如 `raw/ref-xxx.pdf`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// 仅 summaries：作者 wikiID 列表。
    ///
    /// 注意：DB 不存储 authors，从 DB 读取时此字段为空。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    /// 标签 ID 列表（tag-xxx）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// 关联 wikiID 列表（来自 `## 关联页面` 区）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<String>,
    /// Markdown 正文（不含 YAML frontmatter）。
    ///
    /// 注意：DB 不存储正文，从 DB 读取时此字段为空。
    #[serde(skip_serializing_if = "String::is_empty")]
    pub content: String,
    /// 创建时间（RFC3339）。
    pub created: String,
    /// 更新时间（RFC3339）。
    pub updated: String,
}

/// 知识库条目详情（含标签名、关联条目标题等前端展示辅助字段）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiEntryDetail {
    /// 原始条目数据。
    #[serde(flatten)]
    pub entry: WikiEntry,
    /// 标签名称列表，与 `entry.tags` 一一对应。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tag_titles: Vec<String>,
    /// 关联条目标题列表，与 `entry.relations` 一一对应。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relation_titles: Vec<String>,
}

/// 标签。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiTag {
    /// 标签 ID，格式 `tag-xxxxxxxxxxxxxxxx`。
    pub id: String,
    /// 标签名称。
    pub title: String,
}

/// 检索方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalMethod {
    Keyword,
    Semantic,
    Hybrid,
}

impl Default for RetrievalMethod {
    fn default() -> Self {
        RetrievalMethod::Hybrid
    }
}

/// 实际使用的检索方式（含 ID 直查与降级标记）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalMethodUsed {
    DirectIdLookup,
    Keyword,
    Semantic,
    Hybrid,
}

impl RetrievalMethodUsed {
    pub fn as_str(&self) -> &'static str {
        match self {
            RetrievalMethodUsed::DirectIdLookup => "direct_id_lookup",
            RetrievalMethodUsed::Keyword => "keyword",
            RetrievalMethodUsed::Semantic => "semantic",
            RetrievalMethodUsed::Hybrid => "hybrid",
        }
    }
}

/// 单条检索结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryMatch {
    pub wiki_id: String,
    #[serde(rename = "type")]
    pub wiki_type: WikiType,
    pub title: String,
    pub file_path: String,
    /// 匹配度得分。ID 直查时固定为 1.0。
    pub score: f64,
    /// 当 include_content 为 true 时返回正文。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// 单条查询结果（工具一）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub success: bool,
    pub retrieval_method_used: RetrievalMethodUsed,
    pub results: Vec<QueryMatch>,
}

/// 批量查询中的单个子查询结果（工具二）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchQueryItem {
    pub query: String,
    pub retrieval_method_used: RetrievalMethodUsed,
    pub matches: Vec<QueryMatch>,
}

/// 批量查询结果（工具二）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchQueryResult {
    pub success: bool,
    pub results: Vec<BatchQueryItem>,
}

/// 新建条目结果（工具三）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEntryResult {
    pub success: bool,
    pub wiki_id: String,
    pub file_path: String,
    pub tags_generated: Vec<TagMapping>,
    /// 用于生成 embedding 的文本（标题 + 正文）。
    /// 调用方可据此异步计算 embedding，再调用 `wiki::store_embedding` 写入 DB。
    #[serde(skip)]
    pub embedding_text: String,
    /// true 表示检测到重复条目，已将 tags/relations 合并到既有条目而非新建。
    #[serde(default)]
    pub merged: bool,
}

/// 标签名称到 ID 的映射。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagMapping {
    pub name: String,
    pub tag_id: String,
}

/// 修改条目中单个 edit 的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditItemResult {
    #[serde(rename = "type")]
    pub edit_type: String,
    pub success: bool,
}

/// 修改条目结果（工具四）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditEntryResult {
    pub success: bool,
    pub wiki_id: String,
    pub file_path: String,
    pub updated_time: String,
    pub edit_results: Vec<EditItemResult>,
    /// 用于重新生成 embedding 的文本（标题 + 新正文）。
    /// 调用方可据此异步计算 embedding，再调用 `wiki::store_embedding` 写入 DB。
    #[serde(skip)]
    pub embedding_text: String,
}

/// 元信息查询类型（工具五）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetaQueryType {
    Overview,
    Tags,
    Recent,
}

/// 元信息结果（工具五）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaResult {
    pub success: bool,
    pub data: MetaData,
}

/// 元信息数据体（按 query_type 返回不同字段）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetaData {
    pub total_entries: u32,
    pub total_tags: u32,
    pub embedding_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<WikiTag>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_entries: Option<Vec<RecentEntry>>,
}

/// 近期更新条目（仅含 id 和 title）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentEntry {
    pub id: String,
    pub title: String,
}

/// 单个 edit 操作的描述（工具四输入）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EditOp {
    /// 精确查找并替换。
    SearchReplace {
        search: String,
        replace: String,
    },
    /// 在指定锚点后追加内容。
    InsertAfter {
        anchor: String,
        content: String,
    },
}

/// Embedding 提供者 trait（供上层实现并注入）。
///
/// fluen-knowledge 不直接依赖 fluen-embedding，通过此 trait 解耦。
/// 上层（server/agent）将 `EmbeddingRouter` 包装为实现此 trait 的结构传入。
#[async_trait::async_trait]
pub trait KnowledgeEmbedding: Send + Sync {
    /// 将一批文本转为向量。
    async fn embed(&self, texts: Vec<String>) -> anyhow::Result<Vec<Vec<f32>>>;
}
