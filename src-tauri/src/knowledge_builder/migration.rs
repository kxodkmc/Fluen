//! 存量知识库一次性迁移（KB 迁移 M3，设计 §10.3）。
//!
//! 写入时机：宿主首次用新库打开旧项目时自动执行，迁移前弹窗确认
//! （`tauri-plugin-dialog` 原生阻塞对话框，前端无需改动）。
//!
//! 流程：检测 v1 → 弹窗确认 → 备份 `references/wiki/` →
//! `references/wiki.bak-{时间戳}/` → 删除 `index.db`（派生缓存，安全）→
//! `KbBuilder::open`（全新 v2）→ `ops.migrate()`（frontmatter source 包裹
//! summary、authors → 关联行、删 tags 键、source 归一）→
//! `IndexHandle::rebuild()` → `ops.lint()` 体检（Error 级弹窗呈现）。
//!
//! 未溯源的旧 concept/entity 正文保持原样（设计允许未溯源内容，lint 警告
//! 不阻塞）；M3 不提供"重跑构建补溯源"入口（待决策项）。

use std::path::Path;

use fluen_kb::KbBuilder;
use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use super::error::KnowledgeBuilderError;
use super::kb_adapter::is_legacy_schema;

/// 迁移结果。
#[derive(Debug)]
pub enum MigrateOutcome {
    /// 无需迁移（新库或空库）。
    NotNeeded,
    /// 已完成迁移。
    Migrated {
        /// 备份目录路径。
        backup: std::path::PathBuf,
        /// 转换统计。
        report: fluen_kb::ops::MigrateReport,
        /// lint Error 级问题（已弹窗呈现给用户）。
        lint_errors: Vec<String>,
    },
}

/// 确保知识库已迁移到新 schema（必要时弹窗确认后自动迁移）。
///
/// 在打开知识库前调用；`index.db` 不存在或已是 v2 时为 no-op。
pub async fn ensure_migrated(
    app: &AppHandle,
    refs_dir: &Path,
) -> Result<MigrateOutcome, KnowledgeBuilderError> {
    let wiki_db = refs_dir.join("wiki").join("index.db");
    if !wiki_db.exists() {
        return Ok(MigrateOutcome::NotNeeded);
    }
    if !is_legacy_schema(&wiki_db)? {
        return Ok(MigrateOutcome::NotNeeded);
    }

    // 文件 IO + 阻塞对话框：整体放 spawn_blocking
    let app = app.clone();
    let refs_dir = refs_dir.to_path_buf();
    tokio::task::spawn_blocking(move || migrate_blocking(&app, &refs_dir))
        .await
        .map_err(|e| {
            KnowledgeBuilderError::TaskQueue(format!("迁移任务 join 失败: {e}"))
        })?
}

/// 同步迁移主体（阻塞线程内执行：备份 + 弹窗 + 迁移 + 重建 + 体检）。
fn migrate_blocking(
    app: &AppHandle,
    refs_dir: &Path,
) -> Result<MigrateOutcome, KnowledgeBuilderError> {
    tracing::info!(refs_dir = %refs_dir.display(), "检测到旧版知识库，开始迁移流程");

    // 1. 弹窗确认
    let confirmed = app
        .dialog()
        .message(
            "检测到旧版知识库（v1）。将自动执行以下操作：\n\n\
             1. 备份 references/wiki/ 到 references/wiki.bak-<时间戳>/\n\
             2. 重建索引缓存（index.db）\n\
             3. 迁移条目格式（溯源标记 / 关联行 / 移除标签）\n\n\
             原始文件不会丢失。是否继续？",
        )
        .title("知识库升级")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "开始迁移".to_string(),
            "取消".to_string(),
        ))
        .blocking_show();
    if !confirmed {
        tracing::info!("用户取消了知识库迁移");
        return Err(KnowledgeBuilderError::Config(
            "已取消知识库迁移：旧版知识库需迁移后才能使用（可重新触发操作再次确认）".into(),
        ));
    }

    // 2. 备份 references/wiki/ → references/wiki.bak-{时间戳}/
    let wiki_dir = refs_dir.join("wiki");
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let backup = refs_dir.join(format!("wiki.bak-{stamp}"));
    copy_dir_recursive(&wiki_dir, &backup).map_err(KnowledgeBuilderError::from)?;
    tracing::info!(backup = %backup.display(), "旧知识库已备份");

    // 3. 删除 index.db（派生缓存，安全；含 WAL/SHM 伴生文件）
    for suffix in ["index.db", "index.db-wal", "index.db-shm"] {
        let path = wiki_dir.join(suffix);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(KnowledgeBuilderError::Io(e));
            }
        }
    }

    // 4. 全新 v2 打开 + migrate + rebuild + lint
    let kb = KbBuilder::new(refs_dir).open().map_err(KnowledgeBuilderError::from)?;
    let report = kb.ops().migrate().map_err(KnowledgeBuilderError::from)?;
    kb.index().rebuild().map_err(KnowledgeBuilderError::from)?;

    let issues = kb.ops().lint().map_err(KnowledgeBuilderError::from)?;
    let lint_errors: Vec<String> = issues
        .iter()
        .filter(|i| i.level == fluen_kb::LintLevel::Error)
        .map(|i| match &i.entry {
            Some(id) => format!("{id}: {}", i.message),
            None => i.message.clone(),
        })
        .collect();

    // Error 级问题弹窗呈现给用户
    if !lint_errors.is_empty() {
        let preview: Vec<String> = lint_errors.iter().take(8).cloned().collect();
        let more = if lint_errors.len() > 8 {
            format!("\n… 以及另外 {} 条", lint_errors.len() - 8)
        } else {
            String::new()
        };
        let _ = app
            .dialog()
            .message(format!(
                "知识库迁移完成，但体检发现 {} 个错误级问题（不影响使用，详情见日志）：\n\n{}{}",
                lint_errors.len(),
                preview.join("\n"),
                more
            ))
            .title("迁移体检报告")
            .kind(MessageDialogKind::Warning)
            .blocking_show();
    }

    tracing::info!(
        wrapped = report.wrapped,
        authors_converted = report.authors_converted,
        tags_removed = report.tags_removed,
        lint_errors = lint_errors.len(),
        "知识库迁移完成"
    );
    Ok(MigrateOutcome::Migrated {
        backup,
        report,
        lint_errors,
    })
}

/// 递归复制目录。
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
