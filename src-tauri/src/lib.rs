mod app_state;
mod commands;
mod context;
mod conversations;
mod engine_packages;
mod engines;
mod errors;
mod events;
mod hardware;
mod models;
mod processes;
mod prompts;
mod scheduler;
mod settings;
mod storage;

use app_state::AppState;
use tauri::Manager;

pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .try_init();

    let application = tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_directory = app.path().app_data_dir()?;
            let state = AppState::new(&data_directory)
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;
            app.manage(state);
            tracing::info!(path = %data_directory.display(), "NeuraLoc-Core foundation initialized");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_commands::get_app_snapshot,
            commands::chat_commands::start_chat_generation,
            commands::chat_commands::cancel_chat_generation,
            commands::conversation_commands::list_conversations,
            commands::conversation_commands::get_conversation,
            commands::conversation_commands::rename_conversation,
            commands::conversation_commands::set_conversation_pinned,
            commands::conversation_commands::delete_conversation,
            commands::conversation_commands::export_conversation,
            commands::conversation_commands::branch_conversation,
            commands::engine_package_commands::list_engine_packages,
            commands::engine_package_commands::install_engine_package,
            commands::engine_package_commands::import_engine_package,
            commands::engine_package_commands::verify_engine_package,
            commands::engine_package_commands::uninstall_engine_package,
            commands::engine_commands::get_engine_status,
            commands::engine_commands::get_engine_health,
            commands::engine_commands::start_engine,
            commands::engine_commands::stop_engine,
            commands::engine_commands::get_engine_logs,
            commands::hardware_commands::get_hardware_snapshot,
            commands::hardware_commands::refresh_hardware,
            commands::model_commands::list_models,
            commands::model_commands::import_model,
            commands::model_commands::scan_model_folder,
            commands::model_commands::cancel_model_scan,
            commands::model_commands::reverify_model,
            commands::model_commands::remove_model_record,
            commands::prompt_commands::list_prompts,
            commands::prompt_commands::import_prompt,
            commands::prompt_commands::create_prompt,
            commands::prompt_commands::save_prompt,
            commands::prompt_commands::get_prompt_version,
            commands::prompt_commands::duplicate_prompt,
            commands::prompt_commands::delete_prompt,
            commands::prompt_commands::set_prompt_pinned,
            commands::prompt_commands::export_prompt,
            commands::prompt_commands::compile_prompt,
            commands::settings_commands::get_settings,
            commands::settings_commands::update_settings,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build NeuraLoc-Core");

    application.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            if let Some(state) = app_handle.try_state::<AppState>() {
                tauri::async_runtime::block_on(state.processes.stop_all());
            }
        }
    });
}
