//! 文献阅读器命令——读取 MD 内容 + 批量解析图片资源。
//!
//! 渲染由前端 markdown-it 完成，后端只负责：
//! - 读取文献 MD 原文与元数据（`reference_read_content`）
//! - 将 MD 中的相对图片路径批量解析为 data URL（`reference_resolve_assets`）
//!
//! ## 资源解析策略
//!
//! MD 中的图片路径可能是：
//! - 相对 MD 文件的路径（`resource/{id}/page-001.png`）
//! - 相对 resource_dir 的路径（`page-001.png`）
//! - 相对项目根的路径（`references/md/resource/{id}/page-001.png`）
//!
//! 后端依次尝试这三个基准目录，首个命中即返回。未命中的路径不放入
//! 返回 map，前端据此标记 `data-failed-src` 显示加载失败占位。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose, Engine as _};
use serde::Serialize;

use super::error::ReferenceError;
use super::model::{ReferenceEntry, ReferenceFormat};
use super::storage::ReferenceIndex;

// ===========================================================================
// 命令
// ===========================================================================

/// 读取文献内容（MD 原文 + 元数据）。
///
/// 渲染由前端完成，本命令只返回原始 MD 文本与文献元信息。
#[tauri::command]
pub fn reference_read_content(
    project_path: String,
    reference_id: String,
) -> Result<ReaderContent, ReferenceError> {
    let project_dir = PathBuf::from(&project_path);
    let index = ReferenceIndex::new(&project_dir);
    let entry = index
        .find(&reference_id)?
        .ok_or_else(|| ReferenceError::NotFound(reference_id.clone()))?;

    let md_abs = project_dir.join(&entry.md_path);
    let md = std::fs::read_to_string(&md_abs)
        .map_err(|e| ReferenceError::Other(format!("读取文献 MD 失败: {}", e)))?;

    Ok(ReaderContent {
        md,
        meta: ReaderMeta::from(entry),
    })
}

/// 批量解析图片资源为 data URL。
///
/// 前端先用 markdown-it parse 收集 MD 中所有 image token 的 src，
/// 再批量调用本命令。返回 `{ 原始路径: dataUrl }`；
/// 未命中的路径不在返回 map 中，前端据此显示加载失败。
#[tauri::command]
pub fn reference_resolve_assets(
    project_path: String,
    reference_id: String,
    paths: Vec<String>,
) -> Result<HashMap<String, String>, ReferenceError> {
    let project_dir = PathBuf::from(&project_path);
    let index = ReferenceIndex::new(&project_dir);
    let entry = index
        .find(&reference_id)?
        .ok_or_else(|| ReferenceError::NotFound(reference_id.clone()))?;

    let md_abs = project_dir.join(&entry.md_path);
    let md_dir = md_abs
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| project_dir.clone());
    let resource_abs = project_dir.join(&entry.resource_dir);

    let mut result = HashMap::with_capacity(paths.len());
    for rel_path in &paths {
        // 跳过非本地资源（http/https/data/asset 协议）
        if is_remote_url(rel_path) {
            continue;
        }
        if let Ok(data_url) = resolve_single_asset(rel_path, &md_dir, &resource_abs, &project_dir)
        {
            result.insert(rel_path.clone(), data_url);
        }
        // 未命中的路径不放入 result，前端据此标记 data-failed-src
    }
    Ok(result)
}

// ===========================================================================
// 响应类型
// ===========================================================================

/// 文献读取结果（MD 原文 + 元数据）。
#[derive(Debug, Serialize)]
pub struct ReaderContent {
    /// 文献 Markdown 原文。
    pub md: String,
    /// 文献元数据。
    pub meta: ReaderMeta,
}

/// 文献元数据（从 ReferenceEntry 精简而来，仅含阅读器所需字段）。
#[derive(Debug, Serialize)]
pub struct ReaderMeta {
    pub id: String,
    pub title: String,
    pub original_filename: String,
    pub format: ReferenceFormat,
    pub added_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_summary: Option<String>,
    /// MD 文件相对路径（相对项目根）。
    pub md_path: String,
    /// 资源目录相对路径（相对项目根）。
    pub resource_dir: String,
}

impl From<ReferenceEntry> for ReaderMeta {
    fn from(e: ReferenceEntry) -> Self {
        Self {
            id: e.id,
            title: e.title,
            original_filename: e.original_filename,
            format: e.format,
            added_at: e.added_at,
            source: e.source,
            ai_summary: e.ai_summary,
            md_path: e.md_path,
            resource_dir: e.resource_dir,
        }
    }
}

// ===========================================================================
// 资源解析内部实现
// ===========================================================================

/// 判断是否为远程/嵌入式 URL（无需本地解析）。
fn is_remote_url(s: &str) -> bool {
    s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("data:")
        || s.starts_with("asset:")
        || s.starts_with("blob:")
        || s.starts_with("file://")
}

/// 解析单个资源路径为 data URL。
///
/// 依次尝试三个基准目录：md_dir、resource_dir、project_dir。
/// 首个命中的文件读取并编码为 `data:{mime};base64,...`。
fn resolve_single_asset(
    rel_path: &str,
    md_dir: &Path,
    resource_dir: &Path,
    project_dir: &Path,
) -> Result<String, ReferenceError> {
    // 去除可能的 URL 编码（OCR 产物路径通常无编码，但兼容处理）
    let decoded = percent_decode_simple(rel_path);

    let candidates = [
        md_dir.join(&decoded),
        resource_dir.join(&decoded),
        project_dir.join(&decoded),
    ];

    for candidate in &candidates {
        if candidate.is_file() {
            let bytes = std::fs::read(candidate)?;
            let mime = guess_mime(candidate);
            let b64 = general_purpose::STANDARD.encode(&bytes);
            return Ok(format!("data:{};base64,{}", mime, b64));
        }
    }

    Err(ReferenceError::FileNotFound(rel_path.into()))
}

/// 简易 percent-decoding（仅处理 %XX 形式）。
///
/// 不引入额外 crate，覆盖常见场景（%20 空格、%2F 斜杠等）。
/// 对于无效的 percent 序列，原样保留。
fn percent_decode_simple(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_digit(bytes[i + 1]), hex_digit(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// 根据扩展名猜测 MIME 类型。
fn guess_mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("svg") => "image/svg+xml",
        Some("tiff") | Some("tif") => "image/tiff",
        _ => "application/octet-stream",
    }
}

// ===========================================================================
// 单元测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// 创建唯一临时目录。
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluen_reader_test_{}_{}",
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn is_remote_url_detects_protocols() {
        assert!(is_remote_url("http://example.com/a.png"));
        assert!(is_remote_url("https://example.com/a.png"));
        assert!(is_remote_url("data:image/png;base64,xxx"));
        assert!(is_remote_url("asset://localhost/a.png"));
        assert!(!is_remote_url("resource/ref-xxx/a.png"));
        assert!(!is_remote_url("a.png"));
    }

    #[test]
    fn percent_decode_handles_common_cases() {
        assert_eq!(percent_decode_simple("a%20b"), "a b");
        assert_eq!(percent_decode_simple("a%2Fb"), "a/b");
        assert_eq!(percent_decode_simple("no-encoding"), "no-encoding");
        // 无效 percent 序列原样保留
        assert_eq!(percent_decode_simple("100%"), "100%");
    }

    #[test]
    fn guess_mime_recognizes_common_formats() {
        assert_eq!(guess_mime(Path::new("a.png")), "image/png");
        assert_eq!(guess_mime(Path::new("a.JPG")), "image/jpeg");
        assert_eq!(guess_mime(Path::new("a.webp")), "image/webp");
        assert_eq!(guess_mime(Path::new("a.unknown")), "application/octet-stream");
        assert_eq!(guess_mime(Path::new("noext")), "application/octet-stream");
    }

    #[test]
    fn resolve_single_asset_finds_file_in_md_dir() {
        let dir = temp_dir("md_dir");
        let img_path = dir.join("page-001.png");
        fs::write(&img_path, b"fake-png-bytes").unwrap();

        let data_url =
            resolve_single_asset("page-001.png", &dir, &dir, &dir).unwrap();
        assert!(data_url.starts_with("data:image/png;base64,"));
        assert!(data_url.contains("ZmFrZS1wbmctYnl0ZXM=")); // base64 of "fake-png-bytes"

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_single_asset_fails_for_missing_file() {
        let dir = temp_dir("missing");
        let result = resolve_single_asset("nonexistent.png", &dir, &dir, &dir);
        assert!(matches!(result, Err(ReferenceError::FileNotFound(_))));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_single_asset_skips_remote_urls_returning_err() {
        let dir = temp_dir("remote");
        // is_remote_url 在调用前过滤，但 resolve_single_asset 自身不检查
        // 这里验证它对 http 路径找不到本地文件时返回 FileNotFound
        let result = resolve_single_asset("http://example.com/a.png", &dir, &dir, &dir);
        assert!(result.is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
