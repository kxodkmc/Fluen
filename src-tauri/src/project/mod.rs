//! # project
//!
//! 文章项目管理模块——负责在本地文件系统中创建、加载和维护
//! 文章项目的完整目录结构与元信息文件。
//!
//! ## 模块结构
//!
//! | 子模块 | 职责 |
//! |--------|------|
//! | [`model`] | 纯数据模型、校验逻辑与文件夹名清洗 |
//! | [`error`] | 内部错误类型 [`ProjectError`] + 可序列化错误响应 [`ProjectErrorResponse`] |
//! | [`frontmatter`] | YAML front matter 解析与序列化工具（CRLF 安全） |
//! | [`validator`] | 项目结构校验（硬校验阻断 / 软校验收集警告） |
//! | [`creator`] | 项目创建逻辑 |
//! | [`loader`] | 项目加载逻辑 |
//! | [`commands`] | Tauri commands，薄封装层 |
//!
//! ## 文件结构
//!
//! ```text
//! project_name/
//! ├── config.yaml                  # 文章元信息
//! ├── references/
//! │   ├── references-index.json    # 参考文献索引
//! │   ├── md/                      # PDF转MD存储
//! │   ├── translation-cache/       # 翻译缓存
//! │   └── raw/                     # 源文件
//! ├── data/
//! │   ├── experiments/             # 实验数据
//! │   └── questionnaires/          # 问卷数据
//! └── manuscript/
//!     ├── main.md                  # 主文档（编辑器唯一真实数据源）
//!     ├── sections/
//!     │   ├── sections.json        # 章节顺序索引
//!     │   └── sec-{UUID4}.md       # 章节备份（front matter + 正文）
//!     └── assets/                   # 章节资源
//! ```
//!
//! [`ProjectError`]: error::ProjectError
//! [`ProjectErrorResponse`]: error::ProjectErrorResponse

pub mod commands;
pub mod creator;
pub mod error;
pub mod frontmatter;
pub mod loader;
pub mod model;
pub mod section;
pub mod validator;
