//! 文献 Markdown frontmatter 解析与序列化。
//!
//! 模式 2 / 3 下 AI 校正后输出含 YAML frontmatter 的标准 Markdown，
//! 本模块负责解析 frontmatter 字段并序列化写入 MD 文件头部。
//!
//! ## frontmatter 格式
//!
//! ```yaml
//! ---
//! title: 生成式 AI 赋能个性化学习的内在机理研究
//! authors:
//!   - 张三
//!   - 李四
//! journal: 中国教育学报
//! year: 2025
//! ---
//! # 正文标题
//! ...
//! ```
//!
//! 字段说明：
//! - `title`：文献标题（与索引条目 `title` 同步）
//! - `authors`：作者列表（写入索引 `authors` 字段供列表展示）
//! - `journal`：期刊名（可选，仅写入 MD）
//! - `year`：发表年份（可选，仅写入 MD）

use serde::{Deserialize, Serialize};

use super::error::ReferenceError;

/// 文献 frontmatter 元数据。
///
/// 由 AI 在校正阶段从正文识别，写入 MD 文件头部 YAML frontmatter，
/// 同时 `title` / `authors` 同步到 [`super::model::ReferenceEntry`] 索引。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceFrontmatter {
    /// 文献标题。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// 作者列表。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    /// 期刊名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub journal: Option<String>,
    /// 发表年份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<String>,
}

impl ReferenceFrontmatter {
    /// frontmatter 起始标记。
    const FM_START: &str = "---\n";
    /// frontmatter 结束标记。
    const FM_END: &str = "\n---\n";

    /// 从 Markdown 文本解析 frontmatter。
    ///
    /// 若文本不以 `---\n` 开头或无闭合 `---`，返回空 frontmatter（正文即全部文本）。
    ///
    /// 返回 `(frontmatter, body)`：`body` 为去除 frontmatter 后的正文。
    pub fn split_from_markdown(md: &str) -> (Self, &str) {
        if !md.starts_with(Self::FM_START) {
            return (Self::default(), md);
        }
        // 跳过起始 "---\n"，查找结束 "---"
        let after_start = &md[Self::FM_START.len()..];
        let Some(end_idx) = after_start.find("\n---") else {
            return (Self::default(), md);
        };
        let yaml_part = &after_start[..end_idx];
        // 跳过 "\n---" 及随后的换行
        let body_start = end_idx + "\n---".len();
        let body = if body_start < after_start.len() {
            // 跳过 "---" 后的一个换行（若有）
            let rest = &after_start[body_start..];
            if let Some(stripped) = rest.strip_prefix('\n') {
                stripped
            } else {
                rest
            }
        } else {
            ""
        };

        let frontmatter: Self = match serde_yaml::from_str(yaml_part) {
            Ok(fm) => fm,
            Err(e) => {
                tracing::warn!(error = %e, "frontmatter YAML 解析失败，使用空 frontmatter");
                Self::default()
            }
        };
        (frontmatter, body)
    }

    /// 将 frontmatter 序列化并拼接到正文头部。
    ///
    /// 若 frontmatter 所有字段均为空，直接返回 `body`（不添加空 frontmatter）。
    pub fn prepend_to_body(&self, body: &str) -> String {
        if self.title.is_none()
            && self.authors.is_empty()
            && self.journal.is_none()
            && self.year.is_none()
        {
            return body.to_string();
        }
        let yaml = match serde_yaml::to_string(self) {
            Ok(y) => y,
            Err(e) => {
                tracing::warn!(error = %e, "frontmatter 序列化失败，跳过");
                return body.to_string();
            }
        };
        // serde_yaml 输出末尾带换行，拼接为 "---\n{yaml}---\n{body}"
        format!("---\n{}---\n{}", yaml, body)
    }

    /// 从 frontmatter 提取标题（用于索引同步）。
    pub fn extract_title(&self) -> Option<&str> {
        self.title.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty())
    }

    /// 作者列表非空时返回克隆（用于索引同步）。
    pub fn authors_for_index(&self) -> Option<Vec<String>> {
        if self.authors.is_empty() {
            None
        } else {
            Some(self.authors.clone())
        }
    }
}

/// 从 AI 输出的 Markdown 中提取 frontmatter 与正文。
///
/// AI 可能输出无 frontmatter 的纯正文（模式失败回退），此时返回空 frontmatter
/// 与完整正文。返回 `(frontmatter, body)`。
pub fn parse_ai_output(md: &str) -> Result<(ReferenceFrontmatter, &str), ReferenceError> {
    Ok(ReferenceFrontmatter::split_from_markdown(md))
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_with_full_frontmatter() {
        let md = "---\ntitle: 测试标题\nauthors:\n  - 张三\n  - 李四\njournal: 测试期刊\nyear: '2025'\n---\n# 正文\n内容";
        let (fm, body) = ReferenceFrontmatter::split_from_markdown(md);
        assert_eq!(fm.title.as_deref(), Some("测试标题"));
        assert_eq!(fm.authors, vec!["张三", "李四"]);
        assert_eq!(fm.journal.as_deref(), Some("测试期刊"));
        assert_eq!(fm.year.as_deref(), Some("2025"));
        assert_eq!(body, "# 正文\n内容");
    }

    #[test]
    fn split_without_frontmatter() {
        let md = "# 标题\n正文内容";
        let (fm, body) = ReferenceFrontmatter::split_from_markdown(md);
        assert_eq!(fm, ReferenceFrontmatter::default());
        assert_eq!(body, md);
    }

    #[test]
    fn split_with_unclosed_frontmatter() {
        let md = "---\ntitle: 未闭合\n# 正文";
        let (fm, body) = ReferenceFrontmatter::split_from_markdown(md);
        assert_eq!(fm, ReferenceFrontmatter::default());
        assert_eq!(body, md);
    }

    #[test]
    fn prepend_empty_returns_body() {
        let fm = ReferenceFrontmatter::default();
        let body = "# 标题\n正文";
        assert_eq!(fm.prepend_to_body(body), body);
    }

    #[test]
    fn prepend_with_fields() {
        let fm = ReferenceFrontmatter {
            title: Some("测试".into()),
            authors: vec!["张三".into()],
            journal: None,
            year: Some("2025".into()),
        };
        let result = fm.prepend_to_body("# 正文");
        assert!(result.starts_with("---\n"));
        assert!(result.contains("title: 测试"));
        assert!(result.contains("authors:"));
        assert!(result.contains("- 张三"));
        assert!(result.contains("year: '2025'"));
        assert!(result.contains("# 正文"));
    }

    #[test]
    fn roundtrip_preserves_data() {
        let fm = ReferenceFrontmatter {
            title: Some("循环测试".into()),
            authors: vec!["作者A".into(), "作者B".into()],
            journal: Some("期刊X".into()),
            year: Some("2024".into()),
        };
        let md = fm.prepend_to_body("# 正文内容");
        let (parsed, body) = ReferenceFrontmatter::split_from_markdown(&md);
        assert_eq!(parsed, fm);
        assert_eq!(body, "# 正文内容");
    }

    #[test]
    fn extract_title_filters_empty() {
        let fm = ReferenceFrontmatter {
            title: Some("  ".into()),
            ..Default::default()
        };
        assert_eq!(fm.extract_title(), None);

        let fm2 = ReferenceFrontmatter {
            title: Some("有效标题".into()),
            ..Default::default()
        };
        assert_eq!(fm2.extract_title(), Some("有效标题"));
    }

    #[test]
    fn authors_for_index() {
        let fm = ReferenceFrontmatter::default();
        assert_eq!(fm.authors_for_index(), None);

        let fm2 = ReferenceFrontmatter {
            authors: vec!["张三".into()],
            ..Default::default()
        };
        assert_eq!(fm2.authors_for_index(), Some(vec!["张三".to_string()]));
    }
}
