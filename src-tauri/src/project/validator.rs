//! 项目结构校验。
//!
//! 分为两层：
//! - **硬校验**：阻断加载，返回 `ProjectError`（路径不存在、config 损坏等）
//! - **软校验**：收集为 `Vec<ProjectWarning>`，不阻断加载
//!
//! 硬校验由调用方（如 [`loader`](super::loader)）在读取文件时直接触发；
//! 软校验由本模块的函数执行，返回警告列表供调用方合并到结果中。

use std::path::Path;

use super::model::{ProjectWarning, WarningKind};

/// 必需目录（相对项目根）。
const REQUIRED_DIRS: &[&str] = &[
    "references",
    "references/md",
    "references/md/resource",
    "references/translation-cache",
    "references/raw",
    "data",
    "data/experiments",
    "data/questionnaires",
    "manuscript",
    "manuscript/sections",
    "manuscript/assets",
];

/// 必需文件（相对项目根）。
const REQUIRED_FILES: &[&str] = &[
    "config.yaml",
    "references/references-index.json",
    "manuscript/sections/sections.json",
    "manuscript/main.md",
];

/// 软校验：检查目录结构完整性，返回警告列表（不阻断）。
pub fn check_structure(project_dir: &Path) -> Vec<ProjectWarning> {
    let mut warnings = Vec::new();

    for dir in REQUIRED_DIRS {
        if !project_dir.join(dir).is_dir() {
            warnings.push(ProjectWarning {
                kind: WarningKind::MissingDir,
                target: dir.to_string(),
                message: format!("目录缺失: {}", dir),
            });
        }
    }

    for file in REQUIRED_FILES {
        if !project_dir.join(file).is_file() {
            warnings.push(ProjectWarning {
                kind: WarningKind::MissingFile,
                target: file.to_string(),
                message: format!("文件缺失: {}", file),
            });
        }
    }

    warnings
}

/// 软校验：检查 sections.json 条目与实际文件的一致性。
///
/// - `orphan_entries`: sections.json 中有记录但文件不存在
/// - `orphan_files`: 存在 `sec-*.md` 文件但 sections.json 中无记录
pub fn check_section_consistency(
    project_dir: &Path,
    section_ids: &[String],
) -> Vec<ProjectWarning> {
    let mut warnings = Vec::new();
    let sections_dir = project_dir.join("manuscript").join("sections");

    // 检查 sections.json 中的条目是否有对应文件
    for id in section_ids {
        if !sections_dir.join(format!("{}.md", id)).is_file() {
            warnings.push(ProjectWarning {
                kind: WarningKind::OrphanSectionEntry,
                target: id.clone(),
                message: format!("sections.json 引用了不存在的章节文件: {}.md", id),
            });
        }
    }

    // 检查是否存在未被 sections.json 记录的 sec-*.md 文件
    if let Ok(entries) = std::fs::read_dir(&sections_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("sec-") && name_str.ends_with(".md") {
                let id = name_str.trim_end_matches(".md");
                if !section_ids.contains(&id.to_string()) {
                    warnings.push(ProjectWarning {
                        kind: WarningKind::OrphanSectionFile,
                        target: id.to_string(),
                        message: format!("存在未在 sections.json 中登记的章节文件: {}", name_str),
                    });
                }
            }
        }
    }

    warnings
}
