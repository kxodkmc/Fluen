//! 知识库 MCP / 工具层配置。
//!
//! 提供精细化的工具启停、输出截断、前缀定制等能力，
//! 适配不同集成场景（MCP server、confluent agent_runtime 工具注入等）。

use serde::{Deserialize, Serialize};

/// 默认工具名前缀。
const DEFAULT_TOOL_PREFIX: &str = "knowledge_";

/// 默认返回正文最大字符数。
const DEFAULT_MAX_CONTENT_LENGTH: usize = 4096;

/// 默认 top_k。
const DEFAULT_TOP_K: usize = 10;

/// 默认最近条目数量。
const DEFAULT_RECENT_LIMIT: usize = 20;

/// MCP server 与工具层共享的配置。
///
/// 所有字段均有合理默认值，可通过 [`KnowledgeConfig::builder`] 链式构造。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeConfig {
    /// 工具名前缀（如 `knowledge_`），便于在多工具注册表中命名隔离。
    #[serde(default = "default_tool_prefix")]
    pub tool_prefix: String,

    /// 返回正文的截断上限（字符数）。超出时尾部追加 `…(truncated)`。
    /// 设为 `0` 表示不截断。
    #[serde(default = "default_max_content_length")]
    pub max_content_length: usize,

    /// 默认 top_k（单条查询返回上限）。
    #[serde(default = "default_top_k")]
    pub default_top_k: usize,

    /// 默认 recent 条目数量。
    #[serde(default = "default_recent_limit")]
    pub default_recent_limit: usize,

    /// 启用的工具名列表（不含前缀，如 `["query", "create_entry"]`）。
    /// 空列表表示启用全部工具。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enabled_tools: Vec<String>,

    /// 是否自动计算 embedding（需注入 embedding provider 时生效）。
    /// 为 `false` 时，semantic/hybrid 检索自动降级为 keyword。
    #[serde(default)]
    pub enable_embeddings: bool,
}

fn default_tool_prefix() -> String {
    DEFAULT_TOOL_PREFIX.to_string()
}
fn default_max_content_length() -> usize {
    DEFAULT_MAX_CONTENT_LENGTH
}
fn default_top_k() -> usize {
    DEFAULT_TOP_K
}
fn default_recent_limit() -> usize {
    DEFAULT_RECENT_LIMIT
}

impl Default for KnowledgeConfig {
    fn default() -> Self {
        Self {
            tool_prefix: default_tool_prefix(),
            max_content_length: default_max_content_length(),
            default_top_k: default_top_k(),
            default_recent_limit: default_recent_limit(),
            enabled_tools: Vec::new(),
            enable_embeddings: false,
        }
    }
}

impl KnowledgeConfig {
    /// 构造默认配置。
    pub fn new() -> Self {
        Self::default()
    }

    /// 链式构造器。
    pub fn builder() -> KnowledgeConfigBuilder {
        KnowledgeConfigBuilder::default()
    }

    /// 判断指定工具（不含前缀）是否启用。
    pub fn is_tool_enabled(&self, name: &str) -> bool {
        self.enabled_tools.is_empty() || self.enabled_tools.iter().any(|t| t == name)
    }

    /// 构造带前缀的完整工具名。
    pub fn tool_name(&self, short: &str) -> String {
        format!("{}{}", self.tool_prefix, short)
    }

    /// 截断正文，超出上限时追加截断标记。
    pub fn truncate_content(&self, content: &str) -> String {
        if self.max_content_length == 0 || content.chars().count() <= self.max_content_length {
            content.to_string()
        } else {
            // 按字符截断，避免截断 UTF-8 多字节字符中间
            let truncated: String = content.chars().take(self.max_content_length).collect();
            format!("{truncated}…(truncated)")
        }
    }
}

/// 链式构造器。
#[derive(Debug, Default)]
pub struct KnowledgeConfigBuilder {
    config: KnowledgeConfig,
}

impl KnowledgeConfigBuilder {
    pub fn tool_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.config.tool_prefix = prefix.into();
        self
    }

    pub fn max_content_length(mut self, len: usize) -> Self {
        self.config.max_content_length = len;
        self
    }

    pub fn default_top_k(mut self, k: usize) -> Self {
        self.config.default_top_k = k;
        self
    }

    pub fn default_recent_limit(mut self, limit: usize) -> Self {
        self.config.default_recent_limit = limit;
        self
    }

    pub fn enabled_tools(mut self, tools: Vec<String>) -> Self {
        self.config.enabled_tools = tools;
        self
    }

    pub fn enable_embeddings(mut self, enable: bool) -> Self {
        self.config.enable_embeddings = enable;
        self
    }

    pub fn build(self) -> KnowledgeConfig {
        self.config
    }
}

/// 所有可用的工具短名（不含前缀）。
pub const ALL_TOOL_SHORT_NAMES: &[&str] = &[
    "query",
    "query_batch",
    "create_entry",
    "edit_entry",
    "meta",
    "get_entry",
    "list_entries",
    "delete_entry",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = KnowledgeConfig::default();
        assert_eq!(cfg.tool_prefix, "knowledge_");
        assert_eq!(cfg.max_content_length, 4096);
        assert_eq!(cfg.default_top_k, 10);
        assert!(cfg.enabled_tools.is_empty());
        assert!(cfg.is_tool_enabled("query"));
    }

    #[test]
    fn test_enabled_tools_filter() {
        let cfg = KnowledgeConfig::builder()
            .enabled_tools(vec!["query".into(), "meta".into()])
            .build();
        assert!(cfg.is_tool_enabled("query"));
        assert!(cfg.is_tool_enabled("meta"));
        assert!(!cfg.is_tool_enabled("create_entry"));
    }

    #[test]
    fn test_tool_name() {
        let cfg = KnowledgeConfig::default();
        assert_eq!(cfg.tool_name("query"), "knowledge_query");
    }

    #[test]
    fn test_truncate_content() {
        let cfg = KnowledgeConfig::builder().max_content_length(5).build();
        let result = cfg.truncate_content("hello world");
        assert!(result.contains("hello"));
        assert!(result.contains("truncated"));

        // 不截断
        let cfg2 = KnowledgeConfig::builder().max_content_length(0).build();
        assert_eq!(cfg2.truncate_content("hello world"), "hello world");
    }

    #[test]
    fn test_truncate_utf8() {
        let cfg = KnowledgeConfig::builder().max_content_length(3).build();
        let result = cfg.truncate_content("你好世界你好世界");
        // 应按字符截断，不会产生无效 UTF-8
        assert!(result.starts_with("你好世"));
    }
}
