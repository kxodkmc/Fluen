// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[allow(dead_code)]
mod ai_assistant;
mod ai_services;
mod agent_prompts;
mod agent_runtime;
mod agent_tools;
#[allow(dead_code)]
mod app_config;
mod data_analysis;
pub mod editor;
mod builtin_providers;
mod chat_bridge;
mod knowledge_builder;
mod knowledge_mcp_bridge;
mod llm_chat;
mod logging;
#[allow(dead_code)]
mod llm_config;
mod mcp_host;
mod motis_chat;
#[allow(dead_code)]
mod mascot;
mod platform;
mod project;
mod recent_projects;
#[allow(dead_code)]
mod references;
mod task_queue;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::Manager;

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
    let federation_pool = motis_chat::FederationPool::new();
    let ai_assistant_state = ai_assistant::AiAssistantState::new();
    let editor_state = editor::commands::EditorState::new();
    let ocr_state = ai_services::commands::OcrState::new();
    let task_queue_state = TaskQueueState::new();
    let recent_projects_storage =
        RecentProjectsStorage::new().expect("无法确定最近打开项目数据目录");
    let socstat_mcp_host = mcp_host::socstat::SocstatMcpHost::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(llm_storage)
        .manage(app_storage)
        .manage(ai_services_storage)
        .manage(mascot_config_storage)
        .manage(mascot_data_storage)
        .manage(motis_chat_state)
        .manage(federation_pool)
        .manage(ai_assistant_state)
        .manage(editor_state)
        .manage(ocr_state)
        .manage(task_queue_state)
        .manage(recent_projects_storage)
        .manage(socstat_mcp_host.clone())
        .manage(logging::LogGuardHolder(log_guard))
        .setup(|app| {
            // 启动时中断接续：从最近打开项目列表恢复未完成任务
            // （文献导入 / 知识库构建），Running 任务重置为 Pending 后继续执行。
            let recent = app.state::<RecentProjectsStorage>();
            let project_paths: Vec<PathBuf> = recent
                .get()
                .map(|data| {
                    data.entries
                        .iter()
                        .map(|e| PathBuf::from(&e.project_path))
                        .collect()
                })
                .unwrap_or_default();

            let task_queue_state = app.state::<TaskQueueState>();
            let llm_storage = app.state::<ConfigStorage>();
            let ai_storage = app.state::<AiServicesConfigStorage>();
            let recovered = task_queue::recovery::recover_on_startup(
                app.handle(),
                &Arc::new(llm_storage.inner().clone()),
                &Arc::new(ai_storage.inner().clone()),
                &task_queue_state,
                project_paths,
            );
            if recovered > 0 {
                tracing::info!(recovered, "启动时已触发任务恢复");
            }

            // socstat MCP 服务器随应用启动连接并待机（失败降级为无统计工具，
            // 不阻塞启动；详见 mcp_host 模块文档）
            let socstat = app.state::<mcp_host::socstat::SocstatMcpHost>().inner().clone();
            tauri::async_runtime::spawn(async move { socstat.warm().await });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            data_analysis::commands::data_load_dataset,
            data_analysis::commands::data_preview_rows,
            data_analysis::commands::data_list_datasets,
            data_analysis::commands::data_import_dataset,
            data_analysis::commands::data_descriptive,
            data_analysis::commands::data_frequencies,
            data_analysis::commands::data_crosstab,
            data_analysis::commands::data_independent_t_test,
            data_analysis::commands::data_paired_t_test,
            data_analysis::commands::data_one_way_anova,
            data_analysis::commands::data_mann_whitney_u_test,
            data_analysis::commands::data_wilcoxon_signed_rank_test,
            data_analysis::commands::data_kruskal_wallis_test,
            data_analysis::commands::data_chi_square_test,
            data_analysis::commands::data_fisher_exact_test,
            data_analysis::commands::data_shapiro_wilk,
            data_analysis::commands::data_ks_normality_test,
            data_analysis::commands::data_correlation,
            data_analysis::commands::data_correlation_pair,
            data_analysis::commands::data_partial_correlation,
            data_analysis::commands::data_regression,
            data_analysis::commands::data_logistic_regression,
            data_analysis::commands::data_vif,
            data_analysis::commands::data_pca,
            data_analysis::commands::data_reliability,
            data_analysis::commands::data_post_hoc,
            data_analysis::commands::data_factorial_anova,
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
            motis_chat::commands::motis_chat_resolve_approval,
            ai_assistant::commands::ai_assistant_send,
            ai_assistant::commands::ai_assistant_cancel,
            project::commands::get_default_projects_dir,
            project::commands::create_project,
            project::commands::open_project,
            project::commands::create_section,
            project::commands::rename_heading,
            project::commands::insert_heading,
            project::commands::save_document,
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
            references::commands::references_enqueue_imports,
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
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // 应用退出：优雅停机 MCP 服务器子进程（关 stdin → 5s 超时 kill）
            if let tauri::RunEvent::Exit = event {
                let socstat = app.state::<mcp_host::socstat::SocstatMcpHost>();
                tauri::async_runtime::block_on(socstat.shutdown());
            }
        });
}
