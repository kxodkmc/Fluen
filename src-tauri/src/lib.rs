// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[allow(dead_code)]
mod ai_services;
#[allow(dead_code)]
mod app_config;
pub mod editor;
mod builtin_providers;
mod knowledge_builder;
mod logging;
#[allow(dead_code)]
mod llm_config;
mod motis_chat;
#[allow(dead_code)]
mod mascot;
mod platform;
mod project;
mod recent_projects;
#[allow(dead_code)]
mod references;
mod task_queue;

use ai_services::storage::ConfigStorage as AiServicesConfigStorage;
use app_config::storage::AppConfigStorage;
use llm_config::storage::ConfigStorage;
use recent_projects::storage::RecentProjectsStorage;
use task_queue::TaskQueueState;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 先解析 app_config 以获取日志配置（失败用默认，不阻塞启动）。
    let app_storage =
        AppConfigStorage::new().expect("无法确定 App 配置目录");
    let log_config = app_storage
        .load_from_disk()
        .map(|c| c.logging)
        .unwrap_or_default();

    // 初始化日志系统（失败不阻塞启动，后续 tracing 调用退化为静默）。
    let log_guard = logging::init_logging(&log_config).ok();
    tracing::info!(target: "fluen_logging", "Fluen 启动中");

    let llm_storage =
        ConfigStorage::new().expect("无法确定 LLM 配置目录");
    let ai_services_storage =
        AiServicesConfigStorage::new().expect("无法确定 AI 服务配置目录");
    let mascot_config_storage =
        mascot::storage::MascotConfigStorage::new().expect("无法确定宠物配置目录");
    let mascot_data_storage =
        mascot::storage::MascotDataStorage::new().expect("无法确定宠物数据目录");
    let motis_chat_state = motis_chat::MotisChatState::new();
    let editor_state = editor::commands::EditorState::new();
    let ocr_state = ai_services::commands::OcrState::new();
    let import_state = references::commands::ImportState::new();
    let task_queue_state = TaskQueueState::new();
    let recent_projects_storage =
        RecentProjectsStorage::new().expect("无法确定最近打开项目数据目录");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(llm_storage)
        .manage(app_storage)
        .manage(ai_services_storage)
        .manage(mascot_config_storage)
        .manage(mascot_data_storage)
        .manage(motis_chat_state)
        .manage(editor_state)
        .manage(ocr_state)
        .manage(import_state)
        .manage(task_queue_state)
        .manage(recent_projects_storage)
        .manage(logging::LogGuardHolder(log_guard))
        .invoke_handler(tauri::generate_handler![
            greet,
            llm_config::commands::get_llm_config,
            llm_config::commands::save_llm_config,
            llm_config::commands::get_llm_config_path,
            app_config::commands::get_app_config,
            app_config::commands::save_app_config,
            app_config::commands::get_app_config_path,
            ai_services::commands::get_ai_services_config,
            ai_services::commands::save_ai_services_config,
            ai_services::commands::get_ai_services_config_path,
            ai_services::commands::ocr_recognize,
            ai_services::commands::ocr_cancel,
            mascot::commands::get_mascot_config,
            mascot::commands::save_mascot_config,
            mascot::commands::get_mascot_config_path,
            mascot::commands::get_mascot_data,
            mascot::commands::save_mascot_data,
            mascot::commands::get_mascot_data_path,
            motis_chat::commands::motis_chat_send,
            motis_chat::commands::motis_chat_cancel,
            project::commands::get_default_projects_dir,
            project::commands::create_project,
            project::commands::open_project,
            project::commands::create_section,
            project::commands::rename_heading,
            project::commands::insert_heading,
            project::commands::save_temp_md,
            editor::commands::editor_load,
            editor::commands::editor_get_text,
            editor::commands::editor_replace_text,
            editor::commands::editor_undo,
            editor::commands::editor_redo,
            editor::commands::editor_render_html,
            editor::commands::editor_save,
            editor::commands::editor_save_content,
            editor::commands::editor_save_asset,
            editor::commands::editor_history_info,
            editor::commands::editor_record_file_delete,
            editor::commands::editor_record_auto_optimize,
            editor::commands::editor_clear_history,
            references::commands::import_reference,
            references::commands::import_references,
            references::commands::get_import_status,
            references::commands::cancel_import,
            references::commands::cancel_all_imports,
            references::commands::list_references,
            references::commands::get_reference,
            references::commands::delete_reference,
            references::commands::retry_import,
            references::commands::update_reference_title,
            references::commands::check_references_consistency,
            references::reader::reference_read_content,
            references::reader::reference_resolve_assets,
            references::marks::reference_list_marks,
            references::marks::reference_create_mark,
            references::marks::reference_update_mark,
            references::marks::reference_delete_mark,
            task_queue::commands::task_queue_enqueue,
            task_queue::commands::task_queue_list,
            task_queue::commands::task_queue_cancel,
            task_queue::commands::task_queue_retry,
            task_queue::commands::task_queue_resume,
            task_queue::commands::task_queue_delete,
            task_queue::commands::task_queue_clear_finished,
            knowledge_builder::commands::knowledge_build_start,
            knowledge_builder::commands::knowledge_init,
            knowledge_builder::commands::knowledge_list_entries,
            knowledge_builder::commands::knowledge_get_entry,
            knowledge_builder::commands::knowledge_query,
            knowledge_builder::commands::knowledge_meta,
            recent_projects::commands::recent_projects_list,
            recent_projects::commands::recent_projects_record,
            recent_projects::commands::recent_projects_remove,
            recent_projects::commands::recent_projects_trim,
            logging::commands::log_frontend,
            logging::commands::open_logs_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
