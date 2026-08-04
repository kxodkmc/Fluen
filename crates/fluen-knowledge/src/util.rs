//! 共享工具函数。

use std::path::Path;

use crate::error::Result;

/// 原子写入文件：先写临时文件，再 rename 覆盖。
///
/// 确保文件内容要么完全更新，要么保持不变，避免部分写入导致的数据损坏。
pub fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp = dir.join(format!(".ark-fluen-wiki-{}", uuid::Uuid::new_v4().simple()));

    std::fs::write(&tmp, content)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}
