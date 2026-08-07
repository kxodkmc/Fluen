//! 文献保存辅助——图片落盘、Markdown 写入、标题提取。
//!
//! 从原 `importer.rs` 提取的公共逻辑，供三种导入模式复用：
//!
//! - [`save_ocr_images`]：OCR 结果中的图片复制到 resource 目录并重写 MD 路径
//! - [`write_markdown_file`]：写入 Markdown 文件（确保父目录存在）
//! - [`extract_title`]：从 Markdown（可能含 frontmatter）提取标题

use std::path::{Path, PathBuf};

use crate::ai_services::paddleocr::OcrResult;

use super::error::ReferenceError;
use super::frontmatter::ReferenceFrontmatter;

// ---------------------------------------------------------------------------
// 图片保存与路径重写
// ---------------------------------------------------------------------------

/// 保存 OCR 结果中的图片到 resource 目录，并返回合并后的 Markdown。
///
/// 多页用 `\n\n---\n\n` 分隔合并，图片引用路径重写为
/// `resource/{reference_id}/{img_name}`。完成后清理 OCR 临时目录。
pub fn save_ocr_images(
    result: &OcrResult,
    resource_dir: &Path,
    reference_id: &str,
) -> Result<String, ReferenceError> {
    std::fs::create_dir_all(resource_dir)?;

    let mut combined_md = String::new();
    for (i, page) in result.pages.iter().enumerate() {
        if i > 0 {
            combined_md.push_str("\n\n---\n\n");
        }

        let mut page_md = page.markdown.clone();

        // 移动图片到 resource 目录并重写路径
        for img in &page.images {
            let src_path = PathBuf::from(&img.path);
            if !src_path.exists() {
                continue;
            }

            // 图片名可能含子路径（如 "images/fig1.jpg"），保留结构
            let img_name = &img.name;
            let dst_path = resource_dir.join(img_name);
            if let Some(parent) = dst_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&src_path, &dst_path)?;

            // 重写 markdown 中的图片引用为相对路径
            let rel_path = format!("resource/{reference_id}/{img_name}");
            page_md = page_md.replace(img_name, &rel_path);
        }

        combined_md.push_str(&page_md);
    }

    // 清理 OCR 临时目录
    let temp_dir = PathBuf::from(&result.temp_dir);
    let _ = std::fs::remove_dir_all(&temp_dir);

    Ok(combined_md)
}

// ---------------------------------------------------------------------------
// Markdown 写入
// ---------------------------------------------------------------------------

/// 写入 Markdown 文件（确保父目录存在）。
pub fn write_markdown_file(md_path: &Path, content: &str) -> Result<(), ReferenceError> {
    if let Some(parent) = md_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(md_path, content)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 标题提取
// ---------------------------------------------------------------------------

/// 从 Markdown 提取标题——多级回退。
///
/// 1. 若含 frontmatter 且 `title` 非空 → 用 frontmatter title
/// 2. 首个 H1（`# 标题`）
/// 3. 首个 H2/H3（`## 标题` / `### 标题`）
/// 4. 文件名（去扩展名）
pub fn extract_title(markdown: &str, original_filename: &str) -> String {
    // 优先从 frontmatter 提取
    let (frontmatter, body) = ReferenceFrontmatter::split_from_markdown(markdown);
    if let Some(title) = frontmatter.extract_title() {
        return title.to_string();
    }

    // 回退到正文标题
    if let Some(title) = first_heading_level(body, 1) {
        let title = title.trim();
        if !title.is_empty() && title.chars().count() <= 500 {
            return title.to_string();
        }
    }
    for level in [2, 3] {
        if let Some(title) = first_heading_level(body, level) {
            let title = title.trim();
            if !title.is_empty() && title.chars().count() <= 500 {
                return title.to_string();
            }
        }
    }

    // 回退到文件名
    Path::new(original_filename)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "未命名文献".into())
}

/// 提取指定级别的首个标题文本。
fn first_heading_level(markdown: &str, level: usize) -> Option<String> {
    let prefix = "#".repeat(level);
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&prefix) {
            // 确保不是更高级别的标题（如 level=1 时匹配 "# " 但不匹配 "## "）
            let after = &trimmed[level..];
            if after.starts_with(' ') || after.is_empty() {
                return Some(after.trim().to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_title_h1() {
        let md = "Some intro\n# My Paper Title\nMore text";
        assert_eq!(extract_title(md, "file.pdf"), "My Paper Title");
    }

    #[test]
    fn extract_title_h2_fallback() {
        let md = "Intro\n## Section Title\nBody";
        assert_eq!(extract_title(md, "file.pdf"), "Section Title");
    }

    #[test]
    fn extract_title_h3_fallback() {
        let md = "Intro\n### Subsection\nBody";
        assert_eq!(extract_title(md, "file.pdf"), "Subsection");
    }

    #[test]
    fn extract_title_filename_fallback() {
        let md = "No headings here\nJust text";
        assert_eq!(extract_title(md, "my_paper.pdf"), "my_paper");
    }

    #[test]
    fn extract_title_empty_h1_falls_through() {
        let md = "# \n## Real Title";
        assert_eq!(extract_title(md, "file.pdf"), "Real Title");
    }

    #[test]
    fn extract_title_h1_not_confused_with_h2() {
        let md = "## H2 Title\n# H1 Title";
        assert_eq!(extract_title(md, "file.pdf"), "H1 Title");
    }

    #[test]
    fn extract_title_prefers_frontmatter() {
        let md = "---\ntitle: Frontmatter 标题\nauthors:\n  - 张三\n---\n# 正文标题";
        assert_eq!(extract_title(md, "file.pdf"), "Frontmatter 标题");
    }

    #[test]
    fn extract_title_frontmatter_empty_falls_to_h1() {
        let md = "---\ntitle: ''\n---\n# H1 标题";
        assert_eq!(extract_title(md, "file.pdf"), "H1 标题");
    }

    #[test]
    fn extract_title_frontmatter_whitespace_falls_to_h1() {
        let md = "---\ntitle: '   '\n---\n# H1 标题";
        assert_eq!(extract_title(md, "file.pdf"), "H1 标题");
    }
}
