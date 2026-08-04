//! 文献标记（marks）—— 选区高亮的持久化与 CRUD。
//!
//! ## 存储位置
//!
//! `references/marks/{reference_id}.json`，每篇文献独立一份。
//! 写入采用原子操作（先写 `.tmp` 再 `rename`），避免读到半写状态。
//!
//! ## 数据模型
//!
//! ```text
//! MarksFile
//! ├── reference_id: String
//! └── marks: Vec<Mark>
//!                 ├── id: "mk-{16位hex}"
//!                 ├── reference_id
//!                 ├── anchor: MarkAnchor
//!                 │       ├── block_key: BlockKey { block_type, source_line, occurrence }
//!                 │       ├── block_fingerprint: Fingerprint { hash, prefix, suffix }
//!                 │       └── range: TextRange { start_offset, end_offset }
//!                 ├── text: String           // 划线文本快照
//!                 ├── color: MarkColor
//!                 ├── note: Option<String>
//!                 ├── status: MarkStatus
//!                 ├── last_resolved_at
//!                 ├── created_at
//!                 └── updated_at
//! ```
//!
//! ## 锚点设计
//!
//! 每条标记通过三重保险定位：
//! 1. `block_key` — 复合键（type + line + occurrence），快速查找
//! 2. `block_fingerprint` — 块内容指纹（hash + prefix + suffix），迁移校验
//! 3. `range` — 块内字符偏移，精确定位选区
//!
//! 当文献重新渲染后，前端按 `block_key` 查找新块，
//! 比对 `fingerprint.hash` 判断是否迁移（`status` → `migrated`）或失效（`orphaned`）。
//!
//! ## 并发
//!
//! 单用户桌面应用，UI 串行触发写操作。原子写入防止数据损坏；
//! 读改写事务在单进程内串行（每次命令独立读全量 → 改 → 写）。

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::error::ReferenceError;

// ===========================================================================
// 数据模型
// ===========================================================================

/// 可标注的块级元素类型。
///
/// 与前端 `BlockType` 一一对应（serde snake_case）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockType {
    Paragraph,
    Heading,
    ListItem,
    CodeBlock,
    BlockQuote,
    Table,
    TableRow,
    Image,
    Hr,
    Other,
}

/// 块级元素的复合键——唯一标识一个块。
///
/// 由 `block_type + source_line + occurrence` 三元组构成。
/// 序列化为字符串时使用 `"{type}:{line}:{occurrence}"` 格式（前端拼装）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockKey {
    pub block_type: BlockType,
    pub source_line: u32,
    pub occurrence: u32,
}

/// 块内容指纹——校验与迁移依据。
///
/// `hash` 为块纯文本哈希前 16 hex（前端 cyrb53），
/// `prefix` / `suffix` 为可读快照，便于人工核对或低概率哈希碰撞时的兜底。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fingerprint {
    pub hash: String,
    pub prefix: String,
    pub suffix: String,
}

/// 块内字符偏移范围（半开区间：`[start_offset, end_offset)`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextRange {
    pub start_offset: u32,
    pub end_offset: u32,
}

/// 标记锚点——定位选区在文档中的位置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkAnchor {
    pub block_key: BlockKey,
    pub block_fingerprint: Fingerprint,
    pub range: TextRange,
}

/// 标记颜色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkColor {
    Yellow,
    Green,
    Blue,
    Pink,
}

/// 标记状态。
///
/// - `active`：正常显示高亮
/// - `migrated`：文献重渲染后，块指纹变化但可通过模糊匹配迁移
/// - `degraded`：块存在但偏移可能不准（块内容部分变化）
/// - `orphaned`：目标块完全消失，仅保留文本快照供查阅
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkStatus {
    Active,
    Migrated,
    Degraded,
    Orphaned,
}

impl Default for MarkStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// 单条标记。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mark {
    /// 标记 ID（`mk-{16位hex}`）。
    pub id: String,
    /// 所属文献 ID。
    pub reference_id: String,
    /// 锚点（定位信息）。
    pub anchor: MarkAnchor,
    /// 划线文本快照（列表展示用）。
    pub text: String,
    /// 颜色。
    pub color: MarkColor,
    /// 附注（可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// 状态。
    #[serde(default)]
    pub status: MarkStatus,
    /// 最近一次解析（重渲染后比对指纹）的时间。
    pub last_resolved_at: String,
    /// 创建时间（RFC 3339）。
    pub created_at: String,
    /// 最近更新时间（RFC 3339）。
    pub updated_at: String,
}

/// 标记文件——每篇文献一份。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MarksFile {
    reference_id: String,
    #[serde(default)]
    marks: Vec<Mark>,
}

// ===========================================================================
// 存储路径
// ===========================================================================

/// 返回标记文件路径：`{project_dir}/references/marks/{reference_id}.json`。
fn marks_file_path(project_dir: &Path, reference_id: &str) -> PathBuf {
    project_dir
        .join("references")
        .join("marks")
        .join(format!("{}.json", reference_id))
}

// ===========================================================================
// I/O
// ===========================================================================

/// 读取标记文件。文件不存在时返回空列表（不视为错误）。
fn load_marks_file(path: &Path) -> Result<Vec<Mark>, ReferenceError> {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            if content.trim().is_empty() {
                return Ok(Vec::new());
            }
            let file: MarksFile = serde_json::from_str(&content)?;
            Ok(file.marks)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}

/// 原子写入标记文件（先写 `.tmp` 再 `rename`）。
fn save_marks_file(path: &Path, reference_id: &str, marks: &[Mark]) -> Result<(), ReferenceError> {
    let file = MarksFile {
        reference_id: reference_id.to_string(),
        marks: marks.to_vec(),
    };
    let bytes = serde_json::to_vec_pretty(&file)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

// ===========================================================================
// 命令
// ===========================================================================

/// 列出指定文献的全部标记。
///
/// 文件不存在时返回空数组。
#[tauri::command]
pub fn reference_list_marks(
    project_path: String,
    reference_id: String,
) -> Result<Vec<Mark>, ReferenceError> {
    let path = marks_file_path(Path::new(&project_path), &reference_id);
    load_marks_file(&path)
}

/// 创建标记。
///
/// 由前端在用户选区后调用，传入完整锚点与文本快照。
/// 返回创建后的 `Mark`（含服务端生成的 id / 时间戳）。
#[tauri::command]
pub fn reference_create_mark(
    project_path: String,
    reference_id: String,
    anchor: MarkAnchor,
    text: String,
    color: MarkColor,
) -> Result<Mark, ReferenceError> {
    let path = marks_file_path(Path::new(&project_path), &reference_id);
    let mut marks = load_marks_file(&path)?;
    let now = Utc::now().to_rfc3339();
    let mark = Mark {
        id: generate_mark_id(),
        reference_id: reference_id.clone(),
        anchor,
        text,
        color,
        note: None,
        status: MarkStatus::Active,
        last_resolved_at: now.clone(),
        created_at: now.clone(),
        updated_at: now,
    };
    marks.push(mark.clone());
    save_marks_file(&path, &reference_id, &marks)?;
    Ok(mark)
}

/// 更新标记（附注 / 颜色）。
///
/// 仅 `note` 与 `color` 可更新；锚点不可变（变更锚点应删除后重建）。
/// 返回更新后的 `Mark`。
#[tauri::command]
pub fn reference_update_mark(
    project_path: String,
    reference_id: String,
    mark_id: String,
    note: Option<String>,
    color: MarkColor,
) -> Result<Mark, ReferenceError> {
    let path = marks_file_path(Path::new(&project_path), &reference_id);
    let mut marks = load_marks_file(&path)?;
    let mark = marks
        .iter_mut()
        .find(|m| m.id == mark_id)
        .ok_or_else(|| ReferenceError::Other(format!("标记不存在: {}", mark_id)))?;
    mark.note = note;
    mark.color = color;
    mark.updated_at = Utc::now().to_rfc3339();
    let updated = mark.clone();
    save_marks_file(&path, &reference_id, &marks)?;
    Ok(updated)
}

/// 删除标记。
///
/// 不存在时静默成功（幂等）。
#[tauri::command]
pub fn reference_delete_mark(
    project_path: String,
    reference_id: String,
    mark_id: String,
) -> Result<(), ReferenceError> {
    let path = marks_file_path(Path::new(&project_path), &reference_id);
    let mut marks = load_marks_file(&path)?;
    let before = marks.len();
    marks.retain(|m| m.id != mark_id);
    // 仅在确实删除时写入，避免无谓 I/O
    if marks.len() != before {
        save_marks_file(&path, &reference_id, &marks)?;
    }
    Ok(())
}

// ===========================================================================
// 辅助函数
// ===========================================================================

/// 生成标记 ID：`mk-{16位hex}`（取 UUID4 simple 前 16 字符）。
fn generate_mark_id() -> String {
    let hex = Uuid::new_v4().simple().to_string();
    format!("mk-{}", &hex[..16])
}

// ===========================================================================
// 单元测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluen_marks_test_{}_{}_{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_anchor() -> MarkAnchor {
        MarkAnchor {
            block_key: BlockKey {
                block_type: BlockType::Paragraph,
                source_line: 10,
                occurrence: 0,
            },
            block_fingerprint: Fingerprint {
                hash: "abcdef0123456789".into(),
                prefix: "This is the prefix of the block".into(),
                suffix: "This is the suffix of the block".into(),
            },
            range: TextRange {
                start_offset: 5,
                end_offset: 20,
            },
        }
    }

    #[test]
    fn generate_mark_id_format() {
        let id = generate_mark_id();
        assert!(id.starts_with("mk-"));
        assert_eq!(id.len(), "mk-".len() + 16);
        let hex = &id[3..];
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let dir = temp_dir("load_missing");
        let path = marks_file_path(&dir, "ref-xxx");
        let marks = load_marks_file(&path).unwrap();
        assert!(marks.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = temp_dir("roundtrip");
        let ref_id = "ref-abc";
        let path = marks_file_path(&dir, ref_id);

        let mark = Mark {
            id: "mk-0000000000000001".into(),
            reference_id: ref_id.into(),
            anchor: sample_anchor(),
            text: "highlighted text".into(),
            color: MarkColor::Yellow,
            note: Some("a note".into()),
            status: MarkStatus::Active,
            last_resolved_at: "2026-08-01T00:00:00+00:00".into(),
            created_at: "2026-08-01T00:00:00+00:00".into(),
            updated_at: "2026-08-01T00:00:00+00:00".into(),
        };
        save_marks_file(&path, ref_id, &[mark.clone()]).unwrap();

        let loaded = load_marks_file(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, mark.id);
        assert_eq!(loaded[0].text, mark.text);
        assert_eq!(loaded[0].color, MarkColor::Yellow);
        assert_eq!(loaded[0].note.as_deref(), Some("a note"));
        assert_eq!(loaded[0].anchor.block_key.block_type, BlockType::Paragraph);
        assert_eq!(loaded[0].anchor.range.start_offset, 5);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_creates_parent_dirs() {
        let dir = temp_dir("parent_dirs");
        let ref_id = "ref-xyz";
        let path = marks_file_path(&dir, ref_id);
        // 父目录 references/marks/ 尚不存在
        assert!(!path.parent().unwrap().exists());

        save_marks_file(&path, ref_id, &[]).unwrap();
        assert!(path.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_atomic_no_tmp_residue() {
        let dir = temp_dir("atomic");
        let ref_id = "ref-atom";
        let path = marks_file_path(&dir, ref_id);
        save_marks_file(&path, ref_id, &[]).unwrap();

        let tmp = path.with_extension("json.tmp");
        assert!(!tmp.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_empty_file_returns_empty() {
        let dir = temp_dir("empty_file");
        let ref_id = "ref-empty";
        let path = marks_file_path(&dir, ref_id);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "").unwrap();

        let marks = load_marks_file(&path).unwrap();
        assert!(marks.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_create_mark_persists_and_returns() {
        let dir = temp_dir("cmd_create");
        let ref_id = "ref-c1";

        let mark = reference_create_mark(
            dir.to_string_lossy().into(),
            ref_id.into(),
            sample_anchor(),
            "selected text".into(),
            MarkColor::Blue,
        )
        .unwrap();

        assert!(mark.id.starts_with("mk-"));
        assert_eq!(mark.reference_id, ref_id);
        assert_eq!(mark.text, "selected text");
        assert_eq!(mark.color, MarkColor::Blue);
        assert_eq!(mark.status, MarkStatus::Active);
        assert!(mark.note.is_none());
        assert!(!mark.created_at.is_empty());

        // 持久化校验
        let marks = reference_list_marks(dir.to_string_lossy().into(), ref_id.into()).unwrap();
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].id, mark.id);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_list_marks_empty_for_new_reference() {
        let dir = temp_dir("cmd_list_empty");
        let marks = reference_list_marks(dir.to_string_lossy().into(), "ref-new".into()).unwrap();
        assert!(marks.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_update_mark_changes_note_and_color() {
        let dir = temp_dir("cmd_update");
        let ref_id = "ref-u1";
        let mark = reference_create_mark(
            dir.to_string_lossy().into(),
            ref_id.into(),
            sample_anchor(),
            "text".into(),
            MarkColor::Yellow,
        )
        .unwrap();

        let updated = reference_update_mark(
            dir.to_string_lossy().into(),
            ref_id.into(),
            mark.id.clone(),
            Some("my note".into()),
            MarkColor::Pink,
        )
        .unwrap();
        assert_eq!(updated.note.as_deref(), Some("my note"));
        assert_eq!(updated.color, MarkColor::Pink);
        assert_ne!(updated.updated_at, mark.updated_at);

        // 持久化校验
        let marks = reference_list_marks(dir.to_string_lossy().into(), ref_id.into()).unwrap();
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].note.as_deref(), Some("my note"));
        assert_eq!(marks[0].color, MarkColor::Pink);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_update_mark_unknown_id_errors() {
        let dir = temp_dir("cmd_update_missing");
        let ref_id = "ref-u2";
        reference_create_mark(
            dir.to_string_lossy().into(),
            ref_id.into(),
            sample_anchor(),
            "text".into(),
            MarkColor::Yellow,
        )
        .unwrap();

        let result = reference_update_mark(
            dir.to_string_lossy().into(),
            ref_id.into(),
            "mk-doesnotexist".into(),
            None,
            MarkColor::Blue,
        );
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_delete_mark_removes_it() {
        let dir = temp_dir("cmd_delete");
        let ref_id = "ref-d1";
        let mark = reference_create_mark(
            dir.to_string_lossy().into(),
            ref_id.into(),
            sample_anchor(),
            "text".into(),
            MarkColor::Yellow,
        )
        .unwrap();
        // 再加一条
        reference_create_mark(
            dir.to_string_lossy().into(),
            ref_id.into(),
            sample_anchor(),
            "text2".into(),
            MarkColor::Green,
        )
        .unwrap();
        assert_eq!(
            reference_list_marks(dir.to_string_lossy().into(), ref_id.into())
                .unwrap()
                .len(),
            2
        );

        reference_delete_mark(dir.to_string_lossy().into(), ref_id.into(), mark.id.clone()).unwrap();

        let marks = reference_list_marks(dir.to_string_lossy().into(), ref_id.into()).unwrap();
        assert_eq!(marks.len(), 1);
        assert_ne!(marks[0].id, mark.id);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_delete_mark_idempotent_for_unknown_id() {
        let dir = temp_dir("cmd_delete_idem");
        let ref_id = "ref-d2";
        // 文件不存在也应当成功
        reference_delete_mark(
            dir.to_string_lossy().into(),
            ref_id.into(),
            "mk-unknown".into(),
        )
        .unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn json_roundtrip_preserves_all_variants() {
        let mark = Mark {
            id: "mk-test".into(),
            reference_id: "ref-test".into(),
            anchor: MarkAnchor {
                block_key: BlockKey {
                    block_type: BlockType::Heading,
                    source_line: 3,
                    occurrence: 1,
                },
                block_fingerprint: Fingerprint {
                    hash: "deadbeef".into(),
                    prefix: "p".into(),
                    suffix: "s".into(),
                },
                range: TextRange {
                    start_offset: 0,
                    end_offset: 10,
                },
            },
            text: "t".into(),
            color: MarkColor::Pink,
            note: None,
            status: MarkStatus::Migrated,
            last_resolved_at: "2026-08-01T00:00:00+00:00".into(),
            created_at: "2026-08-01T00:00:00+00:00".into(),
            updated_at: "2026-08-01T00:00:00+00:00".into(),
        };
        let json = serde_json::to_string(&mark).unwrap();
        let parsed: Mark = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.color, MarkColor::Pink);
        assert_eq!(parsed.status, MarkStatus::Migrated);
        assert_eq!(parsed.anchor.block_key.block_type, BlockType::Heading);
        assert_eq!(parsed.anchor.block_key.occurrence, 1);

        // 验证 snake_case 序列化
        assert!(json.contains("\"block_type\""));
        assert!(json.contains("\"source_line\""));
        assert!(json.contains("\"pink\""));
        assert!(json.contains("\"migrated\""));
    }

    #[test]
    fn marks_file_isolated_per_reference() {
        let dir = temp_dir("isolated");
        let ref_a = "ref-iso-a";
        let ref_b = "ref-iso-b";

        reference_create_mark(
            dir.to_string_lossy().into(),
            ref_a.into(),
            sample_anchor(),
            "a text".into(),
            MarkColor::Yellow,
        )
        .unwrap();
        reference_create_mark(
            dir.to_string_lossy().into(),
            ref_b.into(),
            sample_anchor(),
            "b text".into(),
            MarkColor::Blue,
        )
        .unwrap();

        let marks_a = reference_list_marks(dir.to_string_lossy().into(), ref_a.into()).unwrap();
        let marks_b = reference_list_marks(dir.to_string_lossy().into(), ref_b.into()).unwrap();
        assert_eq!(marks_a.len(), 1);
        assert_eq!(marks_b.len(), 1);
        assert_ne!(marks_a[0].id, marks_b[0].id);
        assert_eq!(marks_a[0].text, "a text");
        assert_eq!(marks_b[0].text, "b text");

        let _ = fs::remove_dir_all(&dir);
    }
}
