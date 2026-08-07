//! 文献导入模式定义。
//!
//! 三种核心导入模式，决定文献从原始文件转换为 Markdown 的处理路径：
//!
//! | 模式 | 处理路径 | 图片 | 作者信息 |
//! |------|----------|------|----------|
//! | [`Ocr`](ReferenceImportMode::Ocr) | 纯 OCR 识别 | 保留 | 不解析 |
//! | [`OcrWithAiCorrection`](ReferenceImportMode::OcrWithAiCorrection) | OCR + AI 格式校正 | 保留 | AI 识别 |
//! | [`AiOnly`](ReferenceImportMode::AiOnly) | PDF 转文本 + AI 格式校正 | 无 | AI 识别 |
//!
//! ## 模式选择规则
//!
//! - **Ocr**：扫描件 / 图片优先用此模式（PDF 转文本对扫描件无效）
//! - **OcrWithAiCorrection**：OCR 结果含排版噪声时启用，AI 依据 PDF 原文校正格式
//! - **AiOnly**：原生 PDF（非扫描件）可直接提取文本，跳过 OCR 更快更准

use serde::{Deserialize, Serialize};

/// 文献导入模式。
///
/// 序列化为 snake_case 字符串，便于前端与任务队列持久化。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceImportMode {
    /// 纯 OCR 模式——仅使用 OCR 结果，不经过 AI 校正。
    ///
    /// 适用于扫描件 / 图片，或对 OCR 结果质量满意的场景。
    Ocr,
    /// OCR + AI 校正模式——将 OCR 结果与 PDF 转文本一同交给 AI，
    /// 由 AI 输出标准 Markdown（标题层级、列表、表格等）。
    ///
    /// 规则：
    /// - 图片标签保留不变（AI 不接收图片，仅校正文本格式）
    /// - 文本冲突时以 PDF 转文本为准
    /// - AI 不私自改动正文内容，仅做格式纠错
    /// - 解析作者信息写入 frontmatter
    OcrWithAiCorrection,
    /// 纯 AI 识别模式——直接将 PDF 转文本交给 AI 校正格式，
    /// 不经过 OCR。
    ///
    /// 适用于原生 PDF（非扫描件），速度快且无 OCR 噪声。
    /// 无图片资源；解析作者信息写入 frontmatter。
    AiOnly,
}

impl Default for ReferenceImportMode {
    fn default() -> Self {
        Self::Ocr
    }
}

impl ReferenceImportMode {
    /// 是否需要 AI 校正（模式 2 / 3）。
    pub fn requires_ai(self) -> bool {
        matches!(self, Self::OcrWithAiCorrection | Self::AiOnly)
    }

    /// 是否需要 OCR（模式 1 / 2）。
    pub fn requires_ocr(self) -> bool {
        matches!(self, Self::Ocr | Self::OcrWithAiCorrection)
    }

    /// 是否需要 PDF 文本提取（模式 2 / 3）。
    pub fn requires_pdf_text(self) -> bool {
        matches!(self, Self::OcrWithAiCorrection | Self::AiOnly)
    }

    /// 是否解析作者信息（模式 2 / 3）。
    pub fn parses_frontmatter(self) -> bool {
        self.requires_ai()
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_ocr() {
        assert_eq!(ReferenceImportMode::default(), ReferenceImportMode::Ocr);
    }

    #[test]
    fn capability_flags_ocr() {
        let m = ReferenceImportMode::Ocr;
        assert!(m.requires_ocr());
        assert!(!m.requires_ai());
        assert!(!m.requires_pdf_text());
        assert!(!m.parses_frontmatter());
    }

    #[test]
    fn capability_flags_ocr_with_ai() {
        let m = ReferenceImportMode::OcrWithAiCorrection;
        assert!(m.requires_ocr());
        assert!(m.requires_ai());
        assert!(m.requires_pdf_text());
        assert!(m.parses_frontmatter());
    }

    #[test]
    fn capability_flags_ai_only() {
        let m = ReferenceImportMode::AiOnly;
        assert!(!m.requires_ocr());
        assert!(m.requires_ai());
        assert!(m.requires_pdf_text());
        assert!(m.parses_frontmatter());
    }

    #[test]
    fn serde_snake_case() {
        let json = serde_json::to_string(&ReferenceImportMode::OcrWithAiCorrection).unwrap();
        assert_eq!(json, "\"ocr_with_ai_correction\"");
        let parsed: ReferenceImportMode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ReferenceImportMode::OcrWithAiCorrection);
    }
}
