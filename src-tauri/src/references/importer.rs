//! 文献导入编排——preflight → 去重 → 备份 → OCR → 保存 → 更新索引。
//!
//! ## 工作流
//!
//! ```text
//! import_reference(file_path, project_path, force)
//!   │
//!   ├─ Phase 0: Pre-flight（无副作用）
//!   │   ├─ 文件存在 + 格式校验（PDF/图片）
//!   │   ├─ OCR 提供商已配置且启用
//!   │   └─ API Key 已配置
//!   │
//!   ├─ Phase 1: 去重检查（SHA-256）
//!   │   └─ 已存在且 !force → 返回 Duplicate 错误
//!   │
//!   ├─ Phase 2: 备份 + 写索引（status=Pending）
//!   │   ├─ 生成 ID：ref-{UUID4}
//!   │   ├─ 备份原文件 → references/raw/{id}.{ext}
//!   │   └─ 创建索引条目并原子写入
//!   │
//!   ├─ Phase 3: OCR 处理
//!   │   ├─ 更新 status=Processing
//!   │   ├─ 调用 OcrProvider.recognize()
//!   │   └─ 检查 cancel_token
//!   │
//!   ├─ Phase 4: 保存结果
//!   │   ├─ MD → references/md/{id}.md
//!   │   └─ 图片 → references/md/resource/{id}/（重写路径）
//!   │
//!   └─ Phase 5: 完成收尾
//!       ├─ 提取标题（H1 → H2/H3 → 文件名）
//!       └─ 更新索引（status=Completed）
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use tokio_util::sync::CancellationToken;

use crate::ai_services::error::AiServiceError;
use crate::ai_services::model::{AiServicesConfig, ServiceCategory};
use crate::ai_services::paddleocr::{OcrProgress, OcrResult};
use crate::ai_services::provider::{create_ocr_provider, OcrProvider};

use super::error::ReferenceError;
use super::model::{ReferenceEntry, ReferenceFormat, ReferenceStatus};
use super::storage::{backup_file, compute_file_hash, ReferenceIndex};

// ---------------------------------------------------------------------------
// 进度阶段
// ---------------------------------------------------------------------------

/// 导入进度阶段。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportProgress {
    /// 当前阶段：`"backup"` / `"ocr"` / `"saving"`。
    pub stage: String,
    /// OCR 进度（仅 `stage == "ocr"` 时有值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ocr_progress: Option<OcrProgress>,
}

// ---------------------------------------------------------------------------
// ReferenceImporter
// ---------------------------------------------------------------------------

/// 文献导入器。
///
/// 通过 [`ReferenceImporter::new`] 从 AI 服务配置创建，持有 OCR provider。
/// 依赖 [`OcrProvider`] trait 而非具体类型，便于切换 OCR 引擎。
pub struct ReferenceImporter {
    ocr_provider: Box<dyn OcrProvider>,
}

impl ReferenceImporter {
    /// 从 AI 服务配置创建导入器。
    ///
    /// # 错误
    /// - 未配置 OCR 提供商
    /// - 提供商配置不完整
    pub fn new(ai_config: &AiServicesConfig) -> Result<Self, ReferenceError> {
        let provider = ai_config
            .active_provider(ServiceCategory::Ocr)
            .ok_or_else(|| {
                ReferenceError::OcrUnavailable("未配置 OCR 提供商，请在设置中添加".into())
            })?;

        if !provider.enabled {
            return Err(ReferenceError::OcrUnavailable(
                "当前 OCR 提供商已禁用，请在设置中启用".into(),
            ));
        }

        if provider.api_key.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true) {
            return Err(ReferenceError::OcrUnavailable(
                "OCR 提供商缺少 API Key，请在设置中配置".into(),
            ));
        }

        let ocr_provider = create_ocr_provider(provider)
            .map_err(ReferenceError::Ocr)?;

        Ok(Self { ocr_provider })
    }

    /// 导入单个文献。
    ///
    /// `on_progress` 回调用于推送导入进度事件。
    /// `cancel_token` 用于取消 OCR 任务。
    ///
    /// `on_progress` 要求 `'static`，因为内部需要将其移动到 OCR provider 的
    /// `Box<dyn Fn + Send + Sync>` 回调中。调用方应使用 `move` 闭包。
    pub async fn import_single(
        &self,
        file_path: &str,
        project_dir: &Path,
        index: &ReferenceIndex,
        reference_id: &str,
        force: bool,
        on_progress: impl Fn(ImportProgress) + Send + Sync + 'static,
        cancel_token: &CancellationToken,
    ) -> Result<ReferenceEntry, ReferenceError> {
        let src_path = Path::new(file_path);
        // 用 Arc 共享 on_progress：直接调用 + 移入 Boxed 闭包
        let on_progress = Arc::new(on_progress);

        // Phase 0: Pre-flight
        let format = preflight_check(src_path)?;

        // Phase 1: 去重
        let file_hash = compute_file_hash(src_path)?;
        if !force {
            if let Some(existing) = index.find_by_hash(&file_hash)? {
                return Err(ReferenceError::Duplicate {
                    existing_title: existing.title,
                    existing_id: existing.id,
                });
            }
        }

        // Phase 2: 备份 + 写索引（ID 由调用方提供，确保事件流一致）
        let id = reference_id.to_string();
        let original_filename = src_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".into());

        let raw_rel = ReferenceEntry::raw_rel_path(&id, format);
        let md_rel = ReferenceEntry::md_rel_path(&id);
        let resource_rel = ReferenceEntry::resource_rel_path(&id);

        let raw_abs = project_dir.join(&raw_rel);
        backup_file(src_path, &raw_abs)?;

        let entry = ReferenceEntry {
            id: id.clone(),
            title: Path::new(&original_filename)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "未命名文献".into()),
            original_filename,
            format,
            file_hash,
            file_path: raw_rel,
            md_path: md_rel.clone(),
            resource_dir: resource_rel.clone(),
            added_at: Utc::now().to_rfc3339(),
            source: None,
            ai_summary: None,
            status: ReferenceStatus::Pending,
            error: None,
        };
        index.upsert(entry.clone())?;

        // Phase 3: OCR 处理
        index.update(|entries| {
            if let Some(e) = entries.iter_mut().find(|e| e.id == id) {
                e.status = ReferenceStatus::Processing;
            }
            Ok(())
        })?;

        (*on_progress)(ImportProgress {
            stage: "ocr".into(),
            ocr_progress: None,
        });

        let on_progress_for_ocr = on_progress.clone();
        let ocr_result = match self
            .ocr_provider
            .recognize(
                file_path,
                Box::new(move |p: OcrProgress| {
                    (*on_progress_for_ocr)(ImportProgress {
                        stage: "ocr".into(),
                        ocr_progress: Some(p),
                    });
                }),
                cancel_token,
            )
            .await
        {
            Ok(result) => result,
            Err(AiServiceError::Cancelled) => {
                // 取消时标记 Failed
                index.update(|entries| {
                    if let Some(e) = entries.iter_mut().find(|e| e.id == id) {
                        e.status = ReferenceStatus::Failed;
                        e.error = Some("已取消".into());
                    }
                    Ok(())
                })?;
                return Err(ReferenceError::Cancelled);
            }
            Err(e) => {
                let err_msg = e.to_string();
                index.update(|entries| {
                    if let Some(e) = entries.iter_mut().find(|e| e.id == id) {
                        e.status = ReferenceStatus::Failed;
                        e.error = Some(err_msg.clone());
                    }
                    Ok(())
                })?;
                return Err(ReferenceError::Ocr(e));
            }
        };

        if cancel_token.is_cancelled() {
            return Err(ReferenceError::Cancelled);
        }

        // Phase 4: 保存结果
        (*on_progress)(ImportProgress {
            stage: "saving".into(),
            ocr_progress: None,
        });

        let resource_abs = project_dir.join(&resource_rel);
        let md_abs = project_dir.join(&md_rel);

        let combined_md = save_ocr_result(&ocr_result, &md_abs, &resource_abs, &id)?;

        // Phase 5: 完成收尾
        let title = extract_title(&combined_md, &entry.original_filename);
        let final_entry = ReferenceEntry {
            title,
            status: ReferenceStatus::Completed,
            error: None,
            ..entry
        };
        index.upsert(final_entry.clone())?;

        Ok(final_entry)
    }
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// Pre-flight 检查——文件存在 + 格式校验。
fn preflight_check(src: &Path) -> Result<ReferenceFormat, ReferenceError> {
    if !src.exists() {
        return Err(ReferenceError::FileNotFound(
            src.to_string_lossy().to_string(),
        ));
    }
    let filename = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| ReferenceError::Other("无法获取文件名".into()))?;

    ReferenceFormat::from_extension(&filename).ok_or_else(|| {
        let ext = filename
            .rsplit('.')
            .next()
            .unwrap_or("unknown");
        ReferenceError::UnsupportedFormat {
            extension: ext.to_string(),
        }
    })
}

/// 保存 OCR 结果——合并 markdown + 移动图片到 resource 目录 + 重写图片路径。
///
/// 返回合并后的完整 markdown 文本。
fn save_ocr_result(
    result: &OcrResult,
    md_path: &Path,
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

    // 确保 md 父目录存在
    if let Some(parent) = md_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(md_path, &combined_md)?;

    // 清理 OCR 临时目录
    let temp_dir = PathBuf::from(&result.temp_dir);
    let _ = std::fs::remove_dir_all(&temp_dir);

    Ok(combined_md)
}

/// 从 markdown 提取标题——多级回退。
///
/// 1. 首个 H1（`# 标题`）
/// 2. 首个 H2/H3（`## 标题` / `### 标题`）
/// 3. 文件名（去扩展名）
fn extract_title(markdown: &str, original_filename: &str) -> String {
    // 尝试 H1
    if let Some(title) = first_heading_level(markdown, 1) {
        let title = title.trim();
        if !title.is_empty() && title.chars().count() <= 500 {
            return title.to_string();
        }
    }
    // 尝试 H2 / H3
    for level in [2, 3] {
        if let Some(title) = first_heading_level(markdown, level) {
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
    fn preflight_pdf() {
        let dir = std::env::temp_dir().join("fluen_preflight_test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test.pdf");
        std::fs::write(&file, b"fake pdf").unwrap();

        let format = preflight_check(&file).unwrap();
        assert_eq!(format, ReferenceFormat::Pdf);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preflight_unsupported() {
        let dir = std::env::temp_dir().join("fluen_preflight_test2");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test.docx");
        std::fs::write(&file, b"fake docx").unwrap();

        let result = preflight_check(&file);
        assert!(matches!(result, Err(ReferenceError::UnsupportedFormat { .. })));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preflight_not_found() {
        let result = preflight_check(Path::new("/nonexistent/file.pdf"));
        assert!(matches!(result, Err(ReferenceError::FileNotFound(_))));
    }
}
