//! # references
//!
//! 参考文献导入与管理模块。
//!
//! ## 职责
//!
//! - 将 PDF / 图片等文献通过 OCR 转换为标准化 Markdown
//! - 维护 `references-index.json` 索引文件（带锁原子读写）
//! - 管理文献文件（raw 备份、md 正文、resource 资源）
//! - 提供导入 / 删除 / 重试 / 查询等 Tauri commands
//!
//! ## 模块结构
//!
//! | 子模块 | 职责 |
//! |--------|------|
//! | [`model`] | 数据模型（`ReferenceEntry`、`ReferenceStatus` 等） |
//! | [`error`] | 统一错误类型 [`ReferenceError`] |
//! | [`storage`] | 索引文件 I/O（`Mutex` 互斥 + 原子写入 + 内存缓存） |
//! | [`consistency`] | 启动时孤儿文件扫描与一致性校验 |
//! | [`importer`] | 导入编排（preflight → 去重 → 备份 → OCR → 保存） |
//! | [`commands`] | Tauri commands（11 个命令 + `ImportState`） |
//! | [`reader`] | 文献阅读器命令（读 MD + 解析资源） |
//! | [`marks`] | 文献标记（选区高亮 CRUD） |
//!
//! ## 跨模块依赖
//!
//! ```text
//! references::importer
//!   ├── ai_services::storage::ConfigStorage       # 读取 OCR 配置
//!   ├── ai_services::provider::create_ocr_provider # 创建 OcrProvider
//!   └── ai_services::model::ServiceCategory::Ocr
//! ```

pub mod commands;
pub mod consistency;
pub mod error;
pub mod events;
pub mod importer;
pub mod marks;
pub mod model;
pub mod reader;
pub mod storage;
