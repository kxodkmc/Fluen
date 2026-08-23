//! 文献导入编排——preflight → 去重 → 备份 → 按模式处理 → 更新索引。
//!
//! ## 工作流
//!
//! ```text
//! import_single(file_path, project_dir, mode, force)
//!   │
//!   ├─ Phase 0: Pre-flight（无副作用）
//!   │   ├─ 文件存在 + 格式校验（PDF/图片）
//!   │   └─ AiOnly 模式仅支持 PDF
//!   │
//!   ├─ Phase 1: 去重检查（SHA-256）
//!   │   └─ 已存在且 !force → 返回 Duplicate 错误
//!   │
//!   ├─ Phase 2: 备份 + 写索引（status=Pending，记录 import_mode）
//!   │
//!   ├─ Phase 3: 按模式处理（status=Processing）
//!   │   ├─ Ocr:               OCR → 保存图片 + 写 MD
//!   │   ├─ OcrWithAiCorrection: OCR → 保存图片 → PDF 文本 → AI 校正 → 写 MD
//!   │   └─ AiOnly:             PDF 文本 → AI 校正 → 写 MD
//!   │
//!   └─ Phase 5: 完成收尾
//!       ├─ 提取标题（frontmatter title → H1 → H2/H3 → 文件名）
//!       ├─ 提取作者（frontmatter authors）
//!       └─ 更新索引（status=Completed）
//! ```
//!
//! ## 模式与组件
//!
//! [`ReferenceImporter::new`] 按 [`ReferenceImportMode`] 按需创建组件：
//! - 需 OCR（模式 1/2）→ 创建 `OcrProvider`
//! - 需 AI（模式 2/3）→ 创建 [`AiCorrector`]

use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use tokio_util::sync::CancellationToken;

use crate::ai_services::error::AiServiceError;
use crate::ai_services::model::{AiServicesConfig, ServiceCategory};
use crate::ai_services::paddleocr::OcrProgress;
use crate::ai_services::provider::{create_ocr_provider, OcrProvider};
use crate::llm_config::model::LlmConfig;

use super::ai_corrector::AiCorrector;
use super::error::ReferenceError;
use super::frontmatter::ReferenceFrontmatter;
use super::import_mode::ReferenceImportMode;
use super::model::{ReferenceEntry, ReferenceFormat, ReferenceStatus};
use super::pdf_text::extract_pdf_text;
use super::saver::{extract_title, save_ocr_images, write_markdown_file};
use super::storage::{backup_file, compute_file_hash, ReferenceIndex};

// ---------------------------------------------------------------------------
// 进度阶段
// ---------------------------------------------------------------------------

/// 导入进度阶段。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportProgress {
    /// 当前阶段：`"ocr"` / `"saving"` / `"pdf_text"` / `"ai_correction"`。
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
/// 按 [`ReferenceImportMode`] 持有所需组件（OCR provider / AI 校正器），
/// 依赖 trait 抽象，便于切换引擎。
pub struct ReferenceImporter {
    /// OCR 提供商（模式 1/2 使用，模式 3 为 None）。
    ocr_provider: Option<Box<dyn OcrProvider>>,
    /// AI 校正器（模式 2/3 使用，模式 1 为 None）。
    ai_corrector: Option<AiCorrector>,
}

impl ReferenceImporter {
    /// 按导入模式创建导入器，按需装配 OCR / AI 组件。
    ///
    /// # 错误
    /// - 模式需 OCR 但未配置 / 未启用 OCR 提供商
    /// - 模式需 AI 但未配置 LLM 提供商
    pub fn new(
        mode: ReferenceImportMode,
        ai_config: &AiServicesConfig,
        llm_config: &LlmConfig,
    ) -> Result<Self, ReferenceError> {
        let ocr_provider = if mode.requires_ocr() {
            Some(create_ocr_from_config(ai_config)?)
        } else {
            None
        };

        let ai_corrector = if mode.requires_ai() {
            // 使用用户可配置的「AI 最大响应时间」（秒）作为 LLM 请求超时
            let timeout_secs = ai_config.reference_import_timeout_secs;
            Some(AiCorrector::from_llm_config_with_timeout(
                llm_config,
                timeout_secs,
            )?)
        } else {
            None
        };

        Ok(Self {
            ocr_provider,
            ai_corrector,
        })
    }

    /// 导入单个文献。
    ///
    /// `mode` 决定处理路径，必须与 [`new`] 时传入的模式一致。
    pub async fn import_single(
        &self,
        file_path: &str,
        project_dir: &Path,
        index: &ReferenceIndex,
        reference_id: &str,
        mode: ReferenceImportMode,
        force: bool,
        on_progress: impl Fn(ImportProgress) + Send + Sync + 'static,
        cancel_token: &CancellationToken,
    ) -> Result<ReferenceEntry, ReferenceError> {
        let src_path = Path::new(file_path);
        let on_progress: Arc<dyn Fn(ImportProgress) + Send + Sync> = Arc::new(on_progress);

        tracing::info!(
            scope = "references", reference_id = %reference_id,
            file = %file_path, mode = ?mode, force,
            "文献导入开始"
        );

        // Phase 0: Pre-flight
        let format = preflight_check(src_path, mode).inspect_err(|e| {
            tracing::error!(
                scope = "references", reference_id = %reference_id,
                file = %file_path, error = %e,
                "Pre-flight 检查失败"
            );
        })?;

        // Phase 1: 去重
        let file_hash = compute_file_hash(src_path).inspect_err(|e| {
            tracing::error!(
                scope = "references", reference_id = %reference_id,
                error = %e, "计算文件哈希失败"
            );
        })?;
        if !force {
            if let Some(existing) = index.find_by_hash(&file_hash)? {
                if existing.id != reference_id {
                    tracing::warn!(
                        scope = "references", reference_id = %reference_id,
                        existing_id = %existing.id, existing_title = %existing.title,
                        "重复文献，跳过导入"
                    );
                    return Err(ReferenceError::Duplicate {
                        existing_title: existing.title,
                        existing_id: existing.id,
                    });
                }
            }
        }

        // Phase 2: 备份 + 写索引
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
        tracing::debug!(
            scope = "references", reference_id = %reference_id,
            dest = %raw_abs.display(), "原文件已备份"
        );

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
            authors: None,
            year: None,
            import_mode: mode,
            status: ReferenceStatus::Pending,
            error: None,
        };
        index.upsert(entry.clone())?;

        // Phase 3: 更新 Processing + 按模式分发
        index.update(|entries| {
            if let Some(e) = entries.iter_mut().find(|e| e.id == id) {
                e.status = ReferenceStatus::Processing;
            }
            Ok(())
        })?;

        let resource_abs = project_dir.join(&resource_rel);
        let md_abs = project_dir.join(&md_rel);

        let result = match mode {
            ReferenceImportMode::Ocr => {
                self.run_ocr(file_path, &resource_abs, &md_abs, &id, &on_progress, cancel_token)
                    .await
            }
            ReferenceImportMode::OcrWithAiCorrection => {
                self.run_ocr_with_ai(
                    file_path,
                    &resource_abs,
                    &md_abs,
                    &id,
                    &on_progress,
                    cancel_token,
                )
                .await
            }
            ReferenceImportMode::AiOnly => {
                self.run_ai_only(file_path, &md_abs, &on_progress, cancel_token)
                    .await
            }
        };

        let final_md = match result {
            Ok(md) => md,
            Err(ReferenceError::Cancelled) => {
                mark_failed(index, &id, "已取消")?;
                return Err(ReferenceError::Cancelled);
            }
            Err(e) => {
                let msg = e.to_string();
                tracing::error!(
                    scope = "references", reference_id = %reference_id,
                    error = %msg, "导入处理失败"
                );
                mark_failed(index, &id, &msg)?;
                return Err(e);
            }
        };

        if cancel_token.is_cancelled() {
            mark_failed(index, &id, "已取消")?;
            return Err(ReferenceError::Cancelled);
        }

        // Phase 5: 完成收尾——提取标题与作者，更新索引
        // 先剥离 AI 可能给整篇加的 ```markdown 围栏，再解析 frontmatter
        let unwrapped = crate::references::frontmatter::strip_enclosing_fence(&final_md);
        let title = extract_title(&unwrapped, &entry.original_filename);
        let (frontmatter, body) = ReferenceFrontmatter::split_from_markdown(&unwrapped);
        let authors = frontmatter.authors_for_index();
        let year = frontmatter.year.clone();

        // 归一化落盘：AI 可能用 ```yaml / ```markdown 代码块包裹，这里以标准 `---` 形式重写，
        // 既保证 frontmatter 可被再次解析，也不让包裹代码块以源码形式展示在正文里。
        let normalized_md = frontmatter.prepend_to_body(body);
        if normalized_md != final_md {
            write_markdown_file(&md_abs, &normalized_md)?;
        }

        let final_entry = ReferenceEntry {
            title,
            authors,
            year,
            status: ReferenceStatus::Completed,
            error: None,
            ..entry
        };
        index.upsert(final_entry.clone())?;

        tracing::info!(
            scope = "references", reference_id = %reference_id,
            title = %final_entry.title,
            has_authors = final_entry.authors.is_some(),
            "文献导入完成"
        );

        Ok(final_entry)
    }

    /// 模式 1：纯 OCR——OCR 识别 → 保存图片 → 写 MD。
    async fn run_ocr(
        &self,
        file_path: &str,
        resource_dir: &Path,
        md_path: &Path,
        id: &str,
        on_progress: &Arc<dyn Fn(ImportProgress) + Send + Sync>,
        cancel_token: &CancellationToken,
    ) -> Result<String, ReferenceError> {
        let ocr_provider = self.ocr_provider.as_ref().ok_or_else(|| {
            ReferenceError::Other("OCR 模式未配置 OCR provider".into())
        })?;

        // OCR 识别
        (*on_progress)(ImportProgress {
            stage: "ocr".into(),
            ocr_progress: None,
        });
        let on_progress_clone = on_progress.clone();
        let ocr_result = ocr_provider
            .recognize(
                file_path,
                Box::new(move |p: OcrProgress| {
                    (*on_progress_clone)(ImportProgress {
                        stage: "ocr".into(),
                        ocr_progress: Some(p),
                    });
                }),
                cancel_token,
            )
            .await
            .map_err(map_ocr_error)?;

        if cancel_token.is_cancelled() {
            return Err(ReferenceError::Cancelled);
        }

        // 保存图片 + 写 MD
        (*on_progress)(ImportProgress {
            stage: "saving".into(),
            ocr_progress: None,
        });
        let md = save_ocr_images(&ocr_result, resource_dir, id)?;
        write_markdown_file(md_path, &md)?;
        Ok(md)
    }

    /// 模式 2：OCR + AI 校正——OCR → 保存图片 → PDF 文本 → AI 校正 → 写 MD。
    async fn run_ocr_with_ai(
        &self,
        file_path: &str,
        resource_dir: &Path,
        md_path: &Path,
        id: &str,
        on_progress: &Arc<dyn Fn(ImportProgress) + Send + Sync>,
        cancel_token: &CancellationToken,
    ) -> Result<String, ReferenceError> {
        let ocr_provider = self.ocr_provider.as_ref().ok_or_else(|| {
            ReferenceError::Other("OCR+AI 模式未配置 OCR provider".into())
        })?;
        let ai_corrector = self.ai_corrector.as_ref().ok_or_else(|| {
            ReferenceError::Other("OCR+AI 模式未配置 AI 校正器".into())
        })?;

        // 1. OCR 识别
        (*on_progress)(ImportProgress {
            stage: "ocr".into(),
            ocr_progress: None,
        });
        let on_progress_clone = on_progress.clone();
        let ocr_result = ocr_provider
            .recognize(
                file_path,
                Box::new(move |p: OcrProgress| {
                    (*on_progress_clone)(ImportProgress {
                        stage: "ocr".into(),
                        ocr_progress: Some(p),
                    });
                }),
                cancel_token,
            )
            .await
            .map_err(map_ocr_error)?;

        if cancel_token.is_cancelled() {
            return Err(ReferenceError::Cancelled);
        }

        // 2. 保存图片 + 重写 OCR MD 路径（不写 MD 文件，AI 校正后统一写）
        (*on_progress)(ImportProgress {
            stage: "saving".into(),
            ocr_progress: None,
        });
        let ocr_md = save_ocr_images(&ocr_result, resource_dir, id)?;

        // 3. PDF 文本提取
        (*on_progress)(ImportProgress {
            stage: "pdf_text".into(),
            ocr_progress: None,
        });
        let pdf_text = extract_pdf_text(Path::new(file_path))?;

        if cancel_token.is_cancelled() {
            return Err(ReferenceError::Cancelled);
        }

        // 4. AI 校正
        (*on_progress)(ImportProgress {
            stage: "ai_correction".into(),
            ocr_progress: None,
        });
        let corrected_md = ai_corrector
            .correct(Some(&ocr_md), &pdf_text, ReferenceImportMode::OcrWithAiCorrection, cancel_token)
            .await?;

        // 5. 写最终 MD
        write_markdown_file(md_path, &corrected_md)?;
        Ok(corrected_md)
    }

    /// 模式 3：纯 AI——PDF 文本 → AI 校正 → 写 MD。
    async fn run_ai_only(
        &self,
        file_path: &str,
        md_path: &Path,
        on_progress: &Arc<dyn Fn(ImportProgress) + Send + Sync>,
        cancel_token: &CancellationToken,
    ) -> Result<String, ReferenceError> {
        let ai_corrector = self.ai_corrector.as_ref().ok_or_else(|| {
            ReferenceError::Other("纯 AI 模式未配置 AI 校正器".into())
        })?;

        // 1. PDF 文本提取
        (*on_progress)(ImportProgress {
            stage: "pdf_text".into(),
            ocr_progress: None,
        });
        let pdf_text = extract_pdf_text(Path::new(file_path))?;

        if cancel_token.is_cancelled() {
            return Err(ReferenceError::Cancelled);
        }

        // 2. AI 校正
        (*on_progress)(ImportProgress {
            stage: "ai_correction".into(),
            ocr_progress: None,
        });
        let corrected_md = ai_corrector
            .correct(None, &pdf_text, ReferenceImportMode::AiOnly, cancel_token)
            .await?;

        // 3. 写 MD
        write_markdown_file(md_path, &corrected_md)?;
        Ok(corrected_md)
    }
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 从 AI 服务配置创建 OCR provider（含配置校验）。
fn create_ocr_from_config(
    ai_config: &AiServicesConfig,
) -> Result<Box<dyn OcrProvider>, ReferenceError> {
    let provider = ai_config
        .active_provider(ServiceCategory::Ocr)
        .ok_or_else(|| {
            let msg = "未配置 OCR 提供商，请在设置中添加".to_string();
            tracing::error!(scope = "references", error = %msg, "创建 OCR provider 失败");
            ReferenceError::OcrUnavailable(msg)
        })?;

    if !provider.enabled {
        let msg = "当前 OCR 提供商已禁用，请在设置中启用".to_string();
        tracing::error!(scope = "references", error = %msg);
        return Err(ReferenceError::OcrUnavailable(msg));
    }

    if provider.api_key.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true) {
        let msg = "OCR 提供商缺少 API Key，请在设置中配置".to_string();
        tracing::error!(scope = "references", error = %msg);
        return Err(ReferenceError::OcrUnavailable(msg));
    }

    create_ocr_provider(provider)
        .inspect_err(|e| {
            tracing::error!(scope = "references", error = %e, "创建 OCR provider 失败");
        })
        .map_err(ReferenceError::Ocr)
}

/// 将 OCR 调用错误映射为导入错误（取消 → Cancelled，其他 → Ocr）。
fn map_ocr_error(e: AiServiceError) -> ReferenceError {
    match e {
        AiServiceError::Cancelled => ReferenceError::Cancelled,
        other => ReferenceError::Ocr(other),
    }
}

/// 标记索引条目为 Failed。
fn mark_failed(index: &ReferenceIndex, id: &str, error: &str) -> Result<(), ReferenceError> {
    index.update(|entries| {
        if let Some(e) = entries.iter_mut().find(|e| e.id == id) {
            e.status = ReferenceStatus::Failed;
            e.error = Some(error.into());
        }
        Ok(())
    })?;
    Ok(())
}

/// Pre-flight 检查——文件存在 + 格式校验 + 模式兼容性。
fn preflight_check(
    src: &Path,
    mode: ReferenceImportMode,
) -> Result<ReferenceFormat, ReferenceError> {
    if !src.exists() {
        return Err(ReferenceError::FileNotFound(
            src.to_string_lossy().to_string(),
        ));
    }
    let filename = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| ReferenceError::Other("无法获取文件名".into()))?;

    let format = ReferenceFormat::from_extension(&filename).ok_or_else(|| {
        let ext = filename.rsplit('.').next().unwrap_or("unknown");
        ReferenceError::UnsupportedFormat {
            extension: ext.to_string(),
        }
    })?;

    // 纯 AI 模式仅支持 PDF（图片无法提取文本）
    if mode == ReferenceImportMode::AiOnly && format != ReferenceFormat::Pdf {
        return Err(ReferenceError::Other(format!(
            "纯 AI 识别模式仅支持 PDF 文件，当前文件格式为 {:?}",
            format
        )));
    }

    Ok(format)
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preflight_pdf_ocr_mode() {
        let dir = std::env::temp_dir().join("fluen_preflight_test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test.pdf");
        std::fs::write(&file, b"fake pdf").unwrap();

        let format = preflight_check(&file, ReferenceImportMode::Ocr).unwrap();
        assert_eq!(format, ReferenceFormat::Pdf);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preflight_ai_only_rejects_image() {
        let dir = std::env::temp_dir().join("fluen_preflight_ai_only");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("scan.jpg");
        std::fs::write(&file, b"fake jpg").unwrap();

        let result = preflight_check(&file, ReferenceImportMode::AiOnly);
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preflight_ai_only_accepts_pdf() {
        let dir = std::env::temp_dir().join("fluen_preflight_ai_only_pdf");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("paper.pdf");
        std::fs::write(&file, b"fake pdf").unwrap();

        let format = preflight_check(&file, ReferenceImportMode::AiOnly).unwrap();
        assert_eq!(format, ReferenceFormat::Pdf);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preflight_unsupported() {
        let dir = std::env::temp_dir().join("fluen_preflight_test2");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test.docx");
        std::fs::write(&file, b"fake docx").unwrap();

        let result = preflight_check(&file, ReferenceImportMode::Ocr);
        assert!(matches!(result, Err(ReferenceError::UnsupportedFormat { .. })));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preflight_not_found() {
        let result = preflight_check(Path::new("/nonexistent/file.pdf"), ReferenceImportMode::Ocr);
        assert!(matches!(result, Err(ReferenceError::FileNotFound(_))));
    }
}
