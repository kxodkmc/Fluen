//! PDF 文本提取——基于 `pdf_oxide` crate。
//!
//! 模式 2（OCR + AI 校正）与模式 3（纯 AI 识别）需要 PDF 原始文本作为
//! AI 校正的参照 / 唯一输入源。本模块封装逐页提取逻辑，输出用分页符
//! `---` 分隔的纯文本，供 AI 校正使用。
//!
//! ## 设计要点
//!
//! - 纯文本提取（`extract_text`），不保留排版——AI 仅需文本内容做格式校正
//! - 多页用 `\n\n---\n\n` 分隔，与 OCR 多页合并格式一致
//! - 扫描件 PDF 提取结果为空时返回错误，提示用户改用 OCR 模式

use std::path::Path;

use pdf_oxide::{Error as PdfError, PdfDocument};

use super::error::ReferenceError;

/// 页面分隔符（与 OCR 多页合并保持一致）。
const PAGE_SEPARATOR: &str = "\n\n---\n\n";

/// 从 PDF 文件提取全部页面文本。
///
/// 逐页调用 `pdf_oxide::PdfDocument::extract_text`，用 [`PAGE_SEPARATOR`]
/// 拼接。若所有页面提取结果均为空（扫描件无文本层），返回错误。
///
/// # 错误
/// - [`ReferenceError::PdfExtract`]：PDF 打开失败 / 页数读取失败 / 无文本层
pub fn extract_pdf_text(file_path: &Path) -> Result<String, ReferenceError> {
    let doc = PdfDocument::open(file_path).map_err(map_pdf_error(file_path))?;
    let page_count = doc.page_count().map_err(map_pdf_error(file_path))?;
    if page_count == 0 {
        return Err(ReferenceError::PdfExtract("PDF 无页面".into()));
    }

    tracing::debug!(
        file = %file_path.display(),
        pages = page_count,
        "开始 PDF 文本提取"
    );

    let mut pages_text = Vec::with_capacity(page_count);
    let mut total_chars = 0usize;

    for page_idx in 0..page_count {
        match doc.extract_text(page_idx) {
            Ok(text) => {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    total_chars += trimmed.chars().count();
                    pages_text.push(trimmed.to_string());
                } else {
                    pages_text.push(String::new());
                }
            }
            Err(e) => {
                tracing::warn!(
                    page = page_idx,
                    error = %e,
                    "页面文本提取失败，跳过该页"
                );
                pages_text.push(String::new());
            }
        }
    }

    if total_chars == 0 {
        return Err(ReferenceError::PdfExtract(
            "PDF 未提取到任何文本（可能是扫描件，请改用 OCR 模式）".into(),
        ));
    }

    let result = pages_text.join(PAGE_SEPARATOR);
    tracing::info!(
        file = %file_path.display(),
        pages = page_count,
        chars = total_chars,
        "PDF 文本提取完成"
    );
    Ok(result)
}

/// 将 [`PdfError`] 映射为 [`ReferenceError::PdfExtract`]，附带文件路径上下文。
fn map_pdf_error(file_path: &Path) -> impl Fn(PdfError) -> ReferenceError + '_ {
    move |e| {
        let msg = e.to_string();
        tracing::error!(
            file = %file_path.display(),
            error = %msg,
            "PDF 处理失败"
        );
        ReferenceError::PdfExtract(msg)
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_nonexistent_file_returns_error() {
        let result = extract_pdf_text(Path::new("/nonexistent/file.pdf"));
        assert!(matches!(result, Err(ReferenceError::PdfExtract(_))));
    }
}
