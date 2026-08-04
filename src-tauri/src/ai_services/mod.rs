//! # ai_services
//!
//! AI 服务模块——管理 OCR、TTS、ASR 等 AI 能力的提供商配置与调用。
//!
//! ## 设计目标
//!
//! - **可扩展**：通过 [`ServiceCategory`] 区分服务类型，新增 TTS / ASR 只需添加
//!   对应的 provider 实现，配置模型与存储层无需改动。
//! - **双部署模式**：每个提供商支持 `Api`（云端 API）或 `Local`（本地部署）模式。
//! - **类型安全**：通用字段（`api_base_url`、`api_key`、`auth_scheme`）在
//!   [`AiServiceProvider`] 上强类型化；提供商专属配置存于 `provider_config`
//!   （JSON），由对应 provider 自行解析。
//!
//! ## 模块结构
//!
//! | 子模块 | 职责 |
//! |--------|------|
//! | [`model`] | 纯数据模型与校验逻辑（`AiServicesConfig`、`AiServiceProvider` 等） |
//! | [`error`] | 统一错误类型 [`AiServiceError`] |
//! | [`storage`] | 跨平台路径解析与文件读写（原子写入 + 内存缓存） |
//! | [`commands`] | Tauri commands——配置 CRUD + OCR 执行 |
//! | [`paddleocr`] | PaddleOCR API 客户端（Job / Sync 双模式） |
//!
//! ## 存储位置
//!
//! | 平台 | 路径 |
//! |------|------|
//! | macOS | `~/Library/Application Support/com.wppcp.fluen/ai_services_config.json` |
//! | Windows | `%APPDATA%\Fluen\ai_services_config.json` |
//! | Linux | `~/.config/Fluen/ai_services_config.json`（或 `$XDG_CONFIG_HOME/Fluen/`） |

pub mod commands;
pub mod error;
pub mod model;
pub mod paddleocr;
pub mod provider;
pub mod storage;
