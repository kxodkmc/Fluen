//! 跨平台标准路径解析。
//!
//! 统一管理 macOS / Windows / Linux 下的 Fluen 应用目录，
//! 遵循各平台官方开发者规范。对外提供四个语义清晰的接口，
//! 调用方无需关心平台差异，只需按用途选择对应函数：
//!
//! - [`fluen_config_dir`] — 配置文件目录（`app_config.json`、`llm_config.json` 等）
//! - [`fluen_data_dir`]   — 持久化应用数据目录（数据库、索引等）
//! - [`fluen_cache_dir`]  — 缓存目录（可随时清除，不影响功能）
//! - [`fluen_documents_dir`] — 用户文档目录（论文项目存储根目录）
//!
//! ## 各平台路径总览
//!
//! | 用途 | Windows | macOS | Linux |
//! |------|---------|-------|-------|
//! | 配置 | `%APPDATA%\Fluen` | `~/Library/Application Support/com.wppcp.fluen` | `$XDG_CONFIG_HOME/Fluen`（回退 `~/.config/Fluen`） |
//! | 数据 | `%LOCALAPPDATA%\Fluen` | `~/Library/Application Support/com.wppcp.fluen` | `$XDG_DATA_HOME/Fluen`（回退 `~/.local/share/Fluen`） |
//! | 缓存 | `%LOCALAPPDATA%\Fluen\Cache` | `~/Library/Caches/com.wppcp.fluen` | `$XDG_CACHE_HOME/Fluen`（回退 `~/.cache/Fluen`） |
//! | 文档 | `%USERPROFILE%\Documents\Fluen` | `~/Documents/Fluen` | `$XDG_DOCUMENTS_DIR/Fluen`（回退 `~/Documents/Fluen`） |
//!
//! ## 规范参考
//!
//! - **Windows**: [Known Folders / KNOWNFOLDERID](https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid)
//! - **macOS**: [Library Directory Details](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/FileSystemOverview/FileSystemOverview.html)
//! - **Linux**: [XDG Base Directory Specification v0.8](https://specifications.freedesktop.org/basedir-spec/basedir-spec-0.8.html)

use std::path::PathBuf;

/// Windows / Linux 下使用的应用目录名。
const FLUEN_DIR_NAME: &str = "Fluen";

/// macOS Bundle Identifier（与 `tauri.conf.json` 中 `identifier` 一致）。
///
/// Apple 规范要求在 `~/Library/Application Support/` 和 `~/Library/Caches/`
/// 下使用与 bundle identifier 一致的子目录。
#[cfg(target_os = "macos")]
const FLUEN_BUNDLE_ID: &str = "com.wppcp.fluen";

/// 平台路径解析错误。
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    /// 无法确定当前平台的标准目录（环境变量缺失或平台不支持）。
    #[error("无法确定应用目录（平台不支持或环境变量缺失）")]
    UnknownAppDir,
}

// ===========================================================================
// 内部辅助函数
// ===========================================================================

/// 获取 `$HOME` 环境变量（macOS / Linux 使用）。
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn home_dir() -> Result<PathBuf, PlatformError> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(PlatformError::UnknownAppDir)
}

/// 获取 XDG 环境变量对应的目录，若未设置或非绝对路径则回退到默认值。
///
/// XDG 规范要求：环境变量未设置或为空时使用默认路径；
/// 若设置了但值为相对路径，同样回退到默认路径。
#[cfg(target_os = "linux")]
fn xdg_dir(var: &str, default_sub: &str) -> Result<PathBuf, PlatformError> {
    if let Some(val) = std::env::var_os(var) {
        let path = PathBuf::from(&val);
        if path.is_absolute() {
            return Ok(path);
        }
    }
    Ok(home_dir()?.join(default_sub))
}

// ===========================================================================
// 公开 API：配置 / 数据 / 缓存 / 文档
// ===========================================================================

// ---------------------------------------------------------------------------
// 配置目录
// ---------------------------------------------------------------------------

/// 解析 Fluen **配置目录**。
///
/// 存放用户配置文件（如 `app_config.json`、`llm_config.json`）。
///
/// | 平台 | 路径 |
/// |------|------|
/// | Windows | `%APPDATA%\Fluen` |
/// | macOS | `~/Library/Application Support/com.wppcp.fluen` |
/// | Linux | `$XDG_CONFIG_HOME/Fluen`（回退 `~/.config/Fluen`） |
pub fn fluen_config_dir() -> Result<PathBuf, PlatformError> {
    #[cfg(target_os = "macos")]
    {
        return Ok(home_dir()?
            .join("Library")
            .join("Application Support")
            .join(FLUEN_BUNDLE_ID));
    }

    #[cfg(target_os = "windows")]
    {
        return std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|p| p.join(FLUEN_DIR_NAME))
            .ok_or(PlatformError::UnknownAppDir);
    }

    #[cfg(target_os = "linux")]
    {
        return Ok(xdg_dir("XDG_CONFIG_HOME", ".config")?.join(FLUEN_DIR_NAME));
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err(PlatformError::UnknownAppDir)
    }
}

// ---------------------------------------------------------------------------
// 数据目录
// ---------------------------------------------------------------------------

/// 解析 Fluen **数据目录**。
///
/// 存放持久化应用数据（如数据库、索引文件等），与配置文件分离。
/// macOS 下数据与配置同在 `Application Support` 下（符合 Apple 规范）。
///
/// | 平台 | 路径 |
/// |------|------|
/// | Windows | `%LOCALAPPDATA%\Fluen` |
/// | macOS | `~/Library/Application Support/com.wppcp.fluen` |
/// | Linux | `$XDG_DATA_HOME/Fluen`（回退 `~/.local/share/Fluen`） |
#[allow(dead_code)]
pub fn fluen_data_dir() -> Result<PathBuf, PlatformError> {
    #[cfg(target_os = "macos")]
    {
        return Ok(home_dir()?
            .join("Library")
            .join("Application Support")
            .join(FLUEN_BUNDLE_ID));
    }

    #[cfg(target_os = "windows")]
    {
        return std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|p| p.join(FLUEN_DIR_NAME))
            .ok_or(PlatformError::UnknownAppDir);
    }

    #[cfg(target_os = "linux")]
    {
        return Ok(xdg_dir("XDG_DATA_HOME", ".local/share")?.join(FLUEN_DIR_NAME));
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err(PlatformError::UnknownAppDir)
    }
}

// ---------------------------------------------------------------------------
// 缓存目录
// ---------------------------------------------------------------------------

/// 解析 Fluen **缓存目录**。
///
/// 存放可随时清除的临时/缓存数据，不影响应用功能。
///
/// | 平台 | 路径 |
/// |------|------|
/// | Windows | `%LOCALAPPDATA%\Fluen\Cache` |
/// | macOS | `~/Library/Caches/com.wppcp.fluen` |
/// | Linux | `$XDG_CACHE_HOME/Fluen`（回退 `~/.cache/Fluen`） |
#[allow(dead_code)]
pub fn fluen_cache_dir() -> Result<PathBuf, PlatformError> {
    #[cfg(target_os = "macos")]
    {
        return Ok(home_dir()?
            .join("Library")
            .join("Caches")
            .join(FLUEN_BUNDLE_ID));
    }

    #[cfg(target_os = "windows")]
    {
        return std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|p| p.join(FLUEN_DIR_NAME).join("Cache"))
            .ok_or(PlatformError::UnknownAppDir);
    }

    #[cfg(target_os = "linux")]
    {
        return Ok(xdg_dir("XDG_CACHE_HOME", ".cache")?.join(FLUEN_DIR_NAME));
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err(PlatformError::UnknownAppDir)
    }
}

// ---------------------------------------------------------------------------
// 文档目录（论文项目存储路径）
// ---------------------------------------------------------------------------

/// 解析 Fluen **文档目录**（论文项目的默认存储根目录）。
///
/// | 平台 | 路径 |
/// |------|------|
/// | Windows | `%USERPROFILE%\Documents\Fluen` |
/// | macOS | `~/Documents/Fluen` |
/// | Linux | `$XDG_DOCUMENTS_DIR/Fluen`（回退 `~/Documents/Fluen`） |
pub fn fluen_documents_dir() -> Result<PathBuf, PlatformError> {
    Ok(platform_documents_dir()?.join(FLUEN_DIR_NAME))
}

/// 解析当前平台的**系统文档目录**（不含 `Fluen/` 子目录）。
fn platform_documents_dir() -> Result<PathBuf, PlatformError> {
    #[cfg(target_os = "macos")]
    {
        return Ok(home_dir()?.join("Documents"));
    }

    #[cfg(target_os = "windows")]
    {
        return std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .map(|p| p.join("Documents"))
            .ok_or(PlatformError::UnknownAppDir);
    }

    #[cfg(target_os = "linux")]
    {
        // Freedesktop 规范：优先使用 XDG_DOCUMENTS_DIR（必须为绝对路径）
        if let Some(xdg_docs) = std::env::var_os("XDG_DOCUMENTS_DIR") {
            let path = PathBuf::from(&xdg_docs);
            if path.is_absolute() {
                return Ok(path);
            }
        }
        return Ok(home_dir()?.join("Documents"));
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err(PlatformError::UnknownAppDir)
    }
}

// ===========================================================================
// 单元测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_dir_resolves() {
        let dir = fluen_config_dir();
        assert!(dir.is_ok(), "fluen_config_dir 应在当前平台成功解析");
        let dir = dir.unwrap();
        // Windows/Linux: 以 "Fluen" 结尾；macOS: 以 bundle ID 结尾
        #[cfg(target_os = "macos")]
        assert!(
            dir.ends_with(FLUEN_BUNDLE_ID),
            "macOS 配置目录应以 bundle ID 结尾: {dir:?}"
        );
        #[cfg(not(target_os = "macos"))]
        assert!(
            dir.ends_with(FLUEN_DIR_NAME),
            "配置目录应以应用名结尾: {dir:?}"
        );
    }

    #[test]
    fn data_dir_resolves() {
        let dir = fluen_data_dir();
        assert!(dir.is_ok(), "fluen_data_dir 应在当前平台成功解析");
    }

    #[test]
    fn cache_dir_resolves() {
        let dir = fluen_cache_dir();
        assert!(dir.is_ok(), "fluen_cache_dir 应在当前平台成功解析");
    }

    #[test]
    fn documents_dir_resolves() {
        let dir = fluen_documents_dir();
        assert!(dir.is_ok(), "fluen_documents_dir 应在当前平台成功解析");
        let dir = dir.unwrap();
        assert!(
            dir.ends_with(FLUEN_DIR_NAME),
            "文档目录应以 'Fluen' 结尾: {dir:?}"
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn xdg_dir_falls_back_to_home() {
        // XDG_CONFIG_HOME 未设置或相对路径时，应回退到 ~/.config
        let dir = xdg_dir("XDG_CONFIG_HOME", ".config").unwrap();
        // 无论是否设置了 XDG_CONFIG_HOME，结果都应该是一个有效路径
        assert!(dir.is_absolute(), "XDG 回退路径应为绝对路径");
    }
}
