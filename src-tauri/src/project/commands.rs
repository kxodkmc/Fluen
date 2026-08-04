//! Tauri commands——供前端调用的项目操作接口。
//!
//! 本模块仅做参数接收与错误转换，业务逻辑委托给
//! [`creator`]（创建）、[`loader`]（加载）和 [`section`]（章节操作）模块。
//!
//! 通过 `@tauri-apps/api` 的 `invoke` 函数调用：
//!
//! ```typescript
//! // 获取默认文档目录
//! const dir = await invoke<string>('get_default_projects_dir');
//! // 创建项目
//! const path = await invoke<string>('create_project', { request: req });
//! // 打开项目
//! const result = await invoke<OpenProjectResult>('open_project', { projectPath: path });
//! // 创建章节
//! const result = await invoke<OpenProjectResult>('create_section', { request: req });
//! // 重命名标题
//! const result = await invoke<OpenProjectResult>('rename_heading', { request: req });
//! // 插入子标题
//! const result = await invoke<OpenProjectResult>('insert_heading', { request: req });
//! // 保存 .temp.md
//! const result = await invoke<OpenProjectResult>('save_temp_md', { request: req });
//! ```

use super::creator;
use super::error::ProjectErrorResponse;
use super::loader;
use super::model::{
    CreateProjectRequest, CreateSectionRequest, InsertHeadingRequest, OpenProjectResult,
    RenameHeadingRequest, SaveTempMdRequest,
};
use super::section;
use crate::platform;

/// 返回默认的文章存储根目录（`Documents/Fluen`）。
///
/// 前端在新建项目对话框初始化时调用此命令获取默认路径。
#[tauri::command]
pub fn get_default_projects_dir() -> Result<String, ProjectErrorResponse> {
    let dir = platform::fluen_documents_dir()
        .map_err(|e| ProjectErrorResponse::Io { reason: e.to_string() })?;
    Ok(dir.to_string_lossy().to_string())
}

/// 创建文章项目——委托给 [`creator`](super::creator) 模块。
#[tauri::command]
pub fn create_project(request: CreateProjectRequest) -> Result<String, ProjectErrorResponse> {
    creator::create_project(request).map_err(Into::into)
}

/// 打开文章项目——委托给 [`loader`](super::loader) 模块。
///
/// 读取项目配置、校验目录结构、加载所有章节并拼装 `.temp.md`。
/// 返回 [`OpenProjectResult`]，包含完整的项目数据与软校验警告。
#[tauri::command]
pub fn open_project(project_path: String) -> Result<OpenProjectResult, ProjectErrorResponse> {
    loader::open_project(&project_path).map_err(Into::into)
}

/// 创建新章节（一级标题）——委托给 [`section`](super::section) 模块。
///
/// 生成 `sec-{UUID4}.md` 文件，更新 `sections.json`，重新加载项目。
/// 返回更新后的 [`OpenProjectResult`]。
#[tauri::command]
pub fn create_section(
    request: CreateSectionRequest,
) -> Result<OpenProjectResult, ProjectErrorResponse> {
    section::create_section(request).map_err(Into::into)
}

/// 重命名标题（任意层级）——委托给 [`section`](super::section) 模块。
///
/// 通过 `section_id` + `level` + `old_text` 在章节文件中文本匹配定位标题行，
/// 替换为 `new_text`。不依赖行号。
#[tauri::command]
pub fn rename_heading(
    request: RenameHeadingRequest,
) -> Result<OpenProjectResult, ProjectErrorResponse> {
    section::rename_heading(request).map_err(Into::into)
}

/// 插入子标题——委托给 [`section`](super::section) 模块。
///
/// 在指定章节正文末尾追加 `#{new_level} {new_text}`。
#[tauri::command]
pub fn insert_heading(
    request: InsertHeadingRequest,
) -> Result<OpenProjectResult, ProjectErrorResponse> {
    section::insert_heading(request).map_err(Into::into)
}

/// 保存 `.temp.md` 内容——委托给 [`section`](super::section) 模块。
///
/// 将编辑后的 `.temp.md` 全文拆分回各 `sec-{id}.md` 源文件。
/// **安全校验**：拆分后的块数量必须与 `sections.json` 条目数一致。
#[tauri::command]
pub fn save_temp_md(
    request: SaveTempMdRequest,
) -> Result<OpenProjectResult, ProjectErrorResponse> {
    section::save_temp_md(request).map_err(Into::into)
}
