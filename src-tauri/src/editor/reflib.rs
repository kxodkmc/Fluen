//! 文献库桥接——为渲染管线加载项目文献索引并转换为 fluen-markup 条目。
//!
//! ## 职责
//!
//! - 读取 `references-index.json`，把宿主 [`ReferenceEntry`] 转换为
//!   fluen-markup 文献条目（引用键 = 文献 id，如 `ref-a1b2c3d4…`）；
//! - 惰性回填：旧版索引缺 `authors` / `year` 时从 MD frontmatter 补齐一次
//!   （之后索引齐备，回填零 IO）。
//!
//! ## 设计取舍
//!
//! - **不做内存缓存**：预览渲染经前端 300ms 防抖，小 JSON 读取由 OS 页缓存
//!   承担；换取导入新文献后引用立即可见（无缓存陈旧问题）。
//! - **失败降级**：索引读取失败回退空库（引用按 fallback 降级显示），
//!   预览渲染不因文献库加载失败而失败。
//!
//! ## 跨模块依赖
//!
//! ```text
//! editor::reflib
//!   ├── references::storage::ReferenceIndex     # 索引读取（带锁 + 缓存）
//!   └── references::consistency                 # 元数据回填
//! ```

use std::path::Path;

use fluen_markup::{InMemoryReferences, ReferenceEntry as MarkupEntry};

use crate::references::consistency;
use crate::references::model::ReferenceEntry;
use crate::references::storage::ReferenceIndex;

/// 为项目加载 fluen-markup 文献库。
///
/// `project_path` 为 `None`（引擎未绑定项目）时返回空库。
pub fn load_for_project(project_path: Option<&str>) -> InMemoryReferences {
    let Some(path) = project_path else {
        return InMemoryReferences::new();
    };
    let index = ReferenceIndex::new(path);

    // 惰性回填旧版索引缺失的 authors/year；失败仅记录，不影响渲染
    if let Err(e) = consistency::backfill_missing_metadata(Path::new(path), &index) {
        tracing::warn!(scope = "editor", project = %path, error = %e, "文献元数据回填失败");
    }

    match index.list() {
        Ok(entries) => to_markup_library(&entries),
        Err(e) => {
            tracing::warn!(
                scope = "editor", project = %path, error = %e,
                "文献索引读取失败，引用将降级显示"
            );
            InMemoryReferences::new()
        }
    }
}

/// 宿主索引条目 → fluen-markup 文献库。
///
/// `authors` 缺失（部分导入模式不识别作者）时按空列表处理，
/// fluen-markup 将显示 `Anonymous, {year}`。
fn to_markup_library(entries: &[ReferenceEntry]) -> InMemoryReferences {
    let mut lib = InMemoryReferences::new();
    for e in entries {
        lib.insert(
            &e.id,
            MarkupEntry {
                authors: e.authors.clone().unwrap_or_default(),
                year: e.year.clone(),
                title: Some(e.title.clone()),
            },
        );
    }
    lib
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::references::import_mode::ReferenceImportMode;
    use crate::references::model::{ReferenceFormat, ReferenceStatus};

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluen_reflib_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_entry(
        id: &str,
        authors: Option<Vec<String>>,
        year: Option<String>,
    ) -> ReferenceEntry {
        ReferenceEntry {
            id: id.into(),
            title: format!("Title {id}"),
            original_filename: format!("{id}.pdf"),
            format: ReferenceFormat::Pdf,
            file_hash: format!("hash-{id}"),
            file_path: format!("references/raw/{id}.pdf"),
            md_path: format!("references/md/{id}.md"),
            resource_dir: format!("references/md/resource/{id}"),
            added_at: "2026-08-23T00:00:00Z".into(),
            source: None,
            ai_summary: None,
            authors,
            year,
            import_mode: ReferenceImportMode::Ocr,
            status: ReferenceStatus::Completed,
            error: None,
        }
    }

    // None 项目路径 → 空库
    #[test]
    fn none_path_returns_empty_library() {
        let lib = load_for_project(None);
        let html = render_cite(&lib, "ref-a");
        assert!(html.contains("⚠"), "无项目时引用应降级: {html}");
    }

    // 索引条目转换为 fluen-markup 库后，引用可命中
    #[test]
    fn library_resolves_cite_from_index() {
        let dir = temp_dir();
        let index = ReferenceIndex::new(&dir);
        index
            .upsert(sample_entry(
                "ref-hit",
                Some(vec!["马晓飞".into(), "李四".into()]),
                Some("2026".into()),
            ))
            .unwrap();

        let lib = load_for_project(Some(dir.to_str().unwrap()));
        let html = render_cite(&lib, "ref-hit");
        assert!(!html.contains("⚠"), "命中引用不应降级: {html}");
        assert!(html.contains("马晓飞"), "应显示第一作者: {html}");
        assert!(html.contains("2026"), "应显示年份: {html}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // 索引中不存在的引用键 → 降级显示 fallback + ⚠
    #[test]
    fn missing_reference_degrades() {
        let dir = temp_dir();
        ReferenceIndex::new(&dir)
            .upsert(sample_entry("ref-a", None, None))
            .unwrap();

        let lib = load_for_project(Some(dir.to_str().unwrap()));
        let html = render_cite(&lib, "ref-not-exist");
        assert!(html.contains("⚠"), "未命中引用应降级: {html}");
        assert!(html.contains("未知文献"), "应显示 fallback: {html}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // 旧版索引缺 authors/year → 从 MD frontmatter 惰性回填后可命中
    #[test]
    fn backfills_legacy_index_on_load() {
        let dir = temp_dir();
        let md_dir = dir.join("references").join("md");
        std::fs::create_dir_all(&md_dir).unwrap();
        std::fs::write(
            md_dir.join("ref-legacy.md"),
            "---\ntitle: 标题\nauthors:\n  - 王五\nyear: '2024'\n---\n# 正文",
        )
        .unwrap();

        ReferenceIndex::new(&dir)
            .upsert(sample_entry("ref-legacy", None, None))
            .unwrap();

        let lib = load_for_project(Some(dir.to_str().unwrap()));
        let html = render_cite(&lib, "ref-legacy");
        assert!(!html.contains("⚠"), "回填后引用应命中: {html}");
        assert!(html.contains("王五"), "应显示回填的作者: {html}");
        assert!(html.contains("2024"), "应显示回填的年份: {html}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // 项目路径不存在 → 空库，不 panic
    #[test]
    fn nonexistent_project_returns_empty_library() {
        let lib = load_for_project(Some("/nonexistent/fluen/project"));
        let html = render_cite(&lib, "ref-a");
        assert!(html.contains("⚠"));
    }

    /// 辅助：用给定文献库以作者-年风格渲染一条带 fallback 的引用，返回 HTML。
    fn render_cite(lib: &InMemoryReferences, ref_id: &str) -> String {
        let md = format!(r#"引用<f-cite ref="{ref_id}" fallback="未知文献"/>测试"#);
        let mut options = fluen_markup::Options::default();
        options.cite_style = fluen_markup::CiteStyle::AuthorYear;
        super::super::render::render_to_html(&md, &options, lib).unwrap()
    }
}
