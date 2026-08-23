//! 同步原子写——语义镜像 referee `fs_common::atomic_write`
//! （同目录临时文件 + 落盘 sync + rename 发布）。
//!
//! referee 的原子写为 async 实现；项目持久化链路（`save_document` /
//! 章节索引同步）是同步调用，为避免为三处写点把整条链路异步化，
//! 此处提供同步版本并保持相同保障：
//!
//! - 数据先落盘（`sync_all`）再 rename 原子可见；
//! - 覆盖已有文件时保留其权限位；
//! - 任一步失败清理临时文件，不留半写状态。

use std::io::Write;
use std::path::{Path, PathBuf};

use super::error::ProjectError;

/// 同步原子写入文件内容。
pub(crate) fn atomic_write(path: &Path, content: &[u8]) -> Result<(), ProjectError> {
    let tmp = tmp_path(path);
    let existing_perm = std::fs::metadata(path).map(|m| m.permissions()).ok();

    let write_result = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(content)?;
        f.sync_all()?;
        if let Some(perm) = existing_perm {
            std::fs::set_permissions(&tmp, perm)?;
        }
        Ok(())
    })();
    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }

    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.into()
    })
}

/// 生成同目录隐藏临时文件路径（与目标同目录，保证 rename 原子 / 同文件系统）。
fn tmp_path(path: &Path) -> PathBuf {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("tmp");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    parent.join(format!(".{file_name}.{nanos}.tmp"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn atomic_write_publishes_and_leaves_no_tmp() {
        let dir = std::env::temp_dir().join(format!(
            "fluen_atomic_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("out.md");

        // 创建
        atomic_write(&file, b"hello").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "hello");

        // 覆盖
        atomic_write(&file, b"world").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "world");

        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "临时文件泄漏: {leftovers:?}");

        let _ = fs::remove_dir_all(&dir);
    }
}
