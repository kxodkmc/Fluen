//! 参考文献模块的数据模型。
//!
//! 所有结构体仅承载数据，序列化为 JSON 存储于 `references-index.json`。
//!
//! ## 扩展指南
//!
//! 新增文献格式（如 Word）时：
//! 1. 在 [`ReferenceFormat`] 中添加变体
//! 2. 在 [`super::importer`] 的 preflight 中支持新格式的校验
//! 3. 如需格式转换，新增 `converter` 模块

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// 文献格式与状态
// ---------------------------------------------------------------------------

/// 文献文件格式。
///
/// 仅支持可直接 OCR 的格式。Word / CAJ 等需转换的格式预留扩展。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceFormat {
    /// PDF 文档。
    Pdf,
    /// 图片（jpg / png / bmp / tiff / webp）。
    Image,
}

impl ReferenceFormat {
    /// 根据文件扩展名推断格式。
    ///
    /// 不支持的扩展名返回 `None`。
    pub fn from_extension(filename: &str) -> Option<Self> {
        let ext = filename.rsplit('.').next().map(|e| e.to_lowercase());
        match ext.as_deref() {
            Some("pdf") => Some(Self::Pdf),
            Some("jpg") | Some("jpeg") | Some("png") | Some("bmp") | Some("tiff")
            | Some("tif") | Some("webp") => Some(Self::Image),
            _ => None,
        }
    }

    /// 返回格式对应的文件扩展名（不含点）。
    pub fn extension(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            // 图片统一用 jpg 扩展名存储备份
            Self::Image => "jpg",
        }
    }
}

/// 导入状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceStatus {
    /// 已备份，待处理。
    Pending,
    /// OCR 处理中。
    Processing,
    /// 完成。
    Completed,
    /// 失败（保留 raw 文件，可重试）。
    Failed,
}

impl Default for ReferenceStatus {
    fn default() -> Self {
        Self::Pending
    }
}

// ---------------------------------------------------------------------------
// 文献条目
// ---------------------------------------------------------------------------

/// 文献索引条目，对应 `references-index.json` 数组元素。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceEntry {
    /// 文献唯一 ID（`ref-{16位UUID4}`）。
    pub id: String,
    /// 标题（OCR 后从首个 H1 提取，回退到文件名）。
    pub title: String,
    /// 原始文件名。
    pub original_filename: String,
    /// 文件格式。
    pub format: ReferenceFormat,
    /// 文件内容 SHA-256 哈希（用于去重）。
    pub file_hash: String,
    /// 原文件备份路径（相对项目根，`references/raw/{id}.{ext}`）。
    pub file_path: String,
    /// Markdown 文件路径（相对项目根，`references/md/{id}.md`）。
    pub md_path: String,
    /// 资源目录路径（相对项目根，`references/md/resource/{id}`）。
    pub resource_dir: String,
    /// 添加时间（ISO 8601）。
    pub added_at: String,
    /// 来源网站（预留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// AI 摘要（预留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_summary: Option<String>,
    /// 导入状态。
    #[serde(default)]
    pub status: ReferenceStatus,
    /// 失败原因（`status == Failed` 时有值）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ReferenceEntry {
    /// 生成新的文献 ID（`ref-{16位UUID4}`）。
    pub fn generate_id() -> String {
        format!("ref-{}", uuid::Uuid::new_v4().simple())
    }

    /// 构建相对路径（`references/raw/{id}.{ext}`）。
    pub fn raw_rel_path(id: &str, format: ReferenceFormat) -> String {
        format!("references/raw/{id}.{}", format.extension())
    }

    /// 构建相对路径（`references/md/{id}.md`）。
    pub fn md_rel_path(id: &str) -> String {
        format!("references/md/{id}.md")
    }

    /// 构建相对路径（`references/md/resource/{id}`）。
    pub fn resource_rel_path(id: &str) -> String {
        format!("references/md/resource/{id}")
    }
}

// ---------------------------------------------------------------------------
// 批量导入任务
// ---------------------------------------------------------------------------

/// 批量导入任务句柄（`import_references` 立即返回）。
#[derive(Debug, Clone, Serialize)]
pub struct ImportJobHandle {
    /// 任务 ID。
    pub job_id: String,
    /// 所有待导入文件的文献 ID 列表（已预分配）。
    pub reference_ids: Vec<String>,
}

/// 批量导入任务状态。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportJobStatus {
    /// 运行中。
    Running {
        /// 总数。
        total: usize,
        /// 已完成数（含失败）。
        completed: usize,
        /// 失败数。
        failed: usize,
        /// 当前正在处理的文献 ID。
        current: Option<String>,
    },
    /// 已完成。
    Finished {
        total: usize,
        completed: usize,
        failed: usize,
        /// 成功导入的文献条目。
        results: Vec<ReferenceEntry>,
    },
    /// 已取消。
    Cancelled,
}

// ---------------------------------------------------------------------------
// 一致性校验报告
// ---------------------------------------------------------------------------

/// 一致性校验报告（`check_references_consistency` 返回）。
#[derive(Debug, Clone, Serialize)]
pub struct ConsistencyReport {
    /// 孤儿文件（raw/ 下有文件但 index 无记录）。
    pub orphan_files: Vec<String>,
    /// 缺失文件的条目（index 有记录但 raw/md 缺失）。
    pub broken_entries: Vec<String>,
    /// 已修复的孤儿文件数（移动到 .orphan/）。
    pub repaired: usize,
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_from_extension_pdf() {
        assert_eq!(ReferenceFormat::from_extension("paper.pdf"), Some(ReferenceFormat::Pdf));
        assert_eq!(ReferenceFormat::from_extension("PAPER.PDF"), Some(ReferenceFormat::Pdf));
    }

    #[test]
    fn format_from_extension_image() {
        assert_eq!(ReferenceFormat::from_extension("scan.jpg"), Some(ReferenceFormat::Image));
        assert_eq!(ReferenceFormat::from_extension("scan.PNG"), Some(ReferenceFormat::Image));
        assert_eq!(ReferenceFormat::from_extension("scan.webp"), Some(ReferenceFormat::Image));
    }

    #[test]
    fn format_from_extension_unsupported() {
        assert_eq!(ReferenceFormat::from_extension("doc.docx"), None);
        assert_eq!(ReferenceFormat::from_extension("doc.caj"), None);
        assert_eq!(ReferenceFormat::from_extension("noext"), None);
    }

    #[test]
    fn entry_paths() {
        assert_eq!(
            ReferenceEntry::raw_rel_path("ref-abc", ReferenceFormat::Pdf),
            "references/raw/ref-abc.pdf"
        );
        assert_eq!(
            ReferenceEntry::md_rel_path("ref-abc"),
            "references/md/ref-abc.md"
        );
        assert_eq!(
            ReferenceEntry::resource_rel_path("ref-abc"),
            "references/md/resource/ref-abc"
        );
    }

    #[test]
    fn entry_id_format() {
        let id = ReferenceEntry::generate_id();
        assert!(id.starts_with("ref-"));
        assert_eq!(id.len(), "ref-".len() + 32); // UUID4 simple = 32 chars
    }

    #[test]
    fn json_roundtrip() {
        let entry = ReferenceEntry {
            id: "ref-test".into(),
            title: "Test Paper".into(),
            original_filename: "test.pdf".into(),
            format: ReferenceFormat::Pdf,
            file_hash: "abc123".into(),
            file_path: "references/raw/ref-test.pdf".into(),
            md_path: "references/md/ref-test.md".into(),
            resource_dir: "references/md/resource/ref-test".into(),
            added_at: "2026-07-30T12:00:00Z".into(),
            source: None,
            ai_summary: None,
            status: ReferenceStatus::Completed,
            error: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: ReferenceEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "ref-test");
        assert_eq!(parsed.format, ReferenceFormat::Pdf);
        assert_eq!(parsed.status, ReferenceStatus::Completed);
    }
}
