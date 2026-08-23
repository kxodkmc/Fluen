//! 项目创建逻辑——在指定路径下生成完整文件结构。
//!
//! ## 生成的目录结构
//!
//! ```text
//! {storage_path}/{project_name}/
//! ├── config.yaml
//! ├── references/
//! │   ├── references-index.json
//! │   ├── md/
//! │   │   └── resource/
//! │   ├── translation-cache/
//! │   └── raw/
//! ├── data/
//! │   ├── experiments/
//! │   └── questionnaires/
//! └── manuscript/
//!     ├── sections/
//!     │   └── sections.json
//!     └── assets/
//! ```

use std::path::{Path, PathBuf};

use super::error::ProjectError;
use super::loader;
use super::model::{CreateProjectRequest, ProjectConfig};

/// 创建文章项目——在指定路径下生成完整文件结构。
///
/// # 执行流程
///
/// 1. 校验请求参数（`validate()`）
/// 2. 拼接完整路径 `storage_path / project_name`
/// 3. 检查路径是否已存在
/// 4. 创建所有目录
/// 5. 写入 `config.yaml`、`references-index.json`、`sections.json`
/// 6. 返回项目完整路径
pub fn create_project(request: CreateProjectRequest) -> Result<String, ProjectError> {
    request.validate()?;

    let project_dir = PathBuf::from(&request.storage_path).join(&request.project_name);

    if project_dir.exists() {
        return Err(ProjectError::AlreadyExists(
            project_dir.to_string_lossy().to_string(),
        ));
    }

    create_dirs(&project_dir)?;
    write_config_yaml(&project_dir, &request)?;
    write_references_index(&project_dir)?;
    write_sections_json(&project_dir)?;
    write_main_md(&project_dir)?;

    Ok(project_dir.to_string_lossy().to_string())
}

/// 创建项目的全部目录结构。
fn create_dirs(project_dir: &Path) -> Result<(), ProjectError> {
    let dirs = [
        project_dir,
        &project_dir.join("references"),
        &project_dir.join("references").join("md"),
        &project_dir.join("references").join("md").join("resource"),
        &project_dir.join("references").join("translation-cache"),
        &project_dir.join("references").join("raw"),
        &project_dir.join("data"),
        &project_dir.join("data").join("experiments"),
        &project_dir.join("data").join("questionnaires"),
        &project_dir.join("manuscript"),
        &project_dir.join("manuscript").join("sections"),
        &project_dir.join("manuscript").join("assets"),
    ];

    for dir in &dirs {
        std::fs::create_dir_all(dir)?;
    }

    Ok(())
}

/// 写入 `config.yaml`。
fn write_config_yaml(
    project_dir: &Path,
    request: &CreateProjectRequest,
) -> Result<(), ProjectError> {
    let description = request.description.as_deref().and_then(|d| {
        let trimmed = d.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let config = ProjectConfig::new(&request.title, &request.author, description);
    let yaml = serde_yaml::to_string(&config)?;
    super::atomic::atomic_write(&project_dir.join("config.yaml"), yaml.as_bytes())?;
    Ok(())
}

/// 写入 `references-index.json`（初始空数组）。
fn write_references_index(project_dir: &Path) -> Result<(), ProjectError> {
    let json = serde_json::to_string_pretty(&Vec::<serde_json::Value>::new())?;
    super::atomic::atomic_write(
        &project_dir.join("references").join("references-index.json"),
        json.as_bytes(),
    )?;
    Ok(())
}

/// 写入 `sections.json`（初始空数组）。
fn write_sections_json(project_dir: &Path) -> Result<(), ProjectError> {
    let json = serde_json::to_string_pretty(&Vec::<serde_json::Value>::new())?;
    super::atomic::atomic_write(
        &project_dir
            .join("manuscript")
            .join("sections")
            .join("sections.json"),
        json.as_bytes(),
    )?;
    Ok(())
}

/// 写入 `manuscript/main.md`（初始为空主文档，编辑器唯一真实数据源）。
fn write_main_md(project_dir: &Path) -> Result<(), ProjectError> {
    super::atomic::atomic_write(
        &project_dir.join("manuscript").join(loader::MAIN_MD_NAME),
        b"",
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::model::sanitize_project_name;
    use std::fs;

    fn temp_project_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fluen_creator_test_{}_{:?}_{}",
            std::process::id(),
            std::thread::current().id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_request(storage_path: &str) -> CreateProjectRequest {
        CreateProjectRequest {
            title: "My: Cool Article!".into(),
            author: "张三".into(),
            project_name: sanitize_project_name("My: Cool Article!"),
            storage_path: storage_path.into(),
            description: Some("A cool article about testing".into()),
        }
    }

    #[test]
    fn create_project_full_structure() {
        let storage = temp_project_dir();
        let request = sample_request(storage.to_str().unwrap());

        let result = create_project(request).unwrap();
        let project_dir = PathBuf::from(&result);

        // 验证目录结构
        assert!(project_dir.exists());
        assert!(project_dir.join("config.yaml").exists());
        assert!(project_dir.join("references/references-index.json").exists());
        assert!(project_dir.join("references/md").is_dir());
        assert!(project_dir.join("references/translation-cache").is_dir());
        assert!(project_dir.join("references/raw").is_dir());
        assert!(project_dir.join("data/experiments").is_dir());
        assert!(project_dir.join("data/questionnaires").is_dir());
        assert!(project_dir.join("manuscript/sections/sections.json").exists());
        assert!(project_dir.join("manuscript/assets").is_dir());
        assert!(project_dir.join("manuscript/main.md").exists());

        // 验证 config.yaml 内容
        let yaml_content = fs::read_to_string(project_dir.join("config.yaml")).unwrap();
        assert!(yaml_content.contains("My: Cool Article!"));
        assert!(yaml_content.contains("张三"));
        assert!(yaml_content.contains("version:"));
        assert!(yaml_content.contains("A cool article about testing"));

        // 验证 JSON 文件初始为空数组
        let refs_json =
            fs::read_to_string(project_dir.join("references/references-index.json")).unwrap();
        assert_eq!(refs_json.trim(), "[]");

        let sections_json =
            fs::read_to_string(project_dir.join("manuscript/sections/sections.json")).unwrap();
        assert_eq!(sections_json.trim(), "[]");

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn create_project_duplicate_fails() {
        let storage = temp_project_dir();
        let request = sample_request(storage.to_str().unwrap());

        create_project(request.clone()).unwrap();
        let result = create_project(request);
        assert!(matches!(result, Err(ProjectError::AlreadyExists(_))));

        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn create_project_invalid_request_fails() {
        let storage = temp_project_dir();
        let request = CreateProjectRequest {
            title: "  ".into(),
            author: "Author".into(),
            project_name: "test".into(),
            storage_path: storage.to_string_lossy().to_string(),
            description: None,
        };

        let result = create_project(request);
        assert!(matches!(result, Err(ProjectError::Validation(_))));

        let _ = fs::remove_dir_all(&storage);
    }
}
