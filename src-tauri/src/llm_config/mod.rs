//! # llm_config
//!
//! LLM 用户配置模块——管理用户存储的本地 LLM 提供商与模型设置。
//!
//! ## 模块结构
//!
//! | 子模块 | 职责 |
//! |--------|------|
//! | [`model`] | 纯数据模型与校验逻辑（`LlmConfig`、`ProviderConfig`、`ModelConfig` 等） |
//! | [`error`] | 统一错误类型 [`LlmConfigError`] |
//! | [`storage`] | 跨平台路径解析与文件读写（原子写入） |
//! | [`commands`] | Tauri commands，供前端调用 |
//!
//! ## 存储位置
//!
//! | 平台 | 路径 |
//! |------|------|
//! | macOS | `~/Library/Preferences/Fluen/llm_config.json` |
//! | Windows | `%APPDATA%\Fluen\llm_config.json` |
//! | Linux | `~/.config/Fluen/llm_config.json` |

pub mod commands;
pub mod error;
pub mod model;
pub mod storage;
