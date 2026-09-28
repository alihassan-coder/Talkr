use std::sync::{Arc, Mutex};

use tauri::Manager;
use tauri_plugin_log::{Target, TargetKind};

mod error;
mod paths;
mod config;
mod hardware;
mod catalog;
mod downloader;
mod db;
mod audio;
mod engines;
mod commands;

use error::AppError;
use paths::AppPaths;
use config::Settings;
use hardware::HardwareInfo;
use downloader::DownloadManager;
use engines::EngineRegistry;
use audio::record::AudioRecorder;
use commands::*;

pub struct AppState {
    pub paths: AppPaths,
    pub settings: Mutex<Settings>,
    pub hardware: HardwareInfo,
    pub download_manager: Arc<Mutex<Option<DownloadManager>>>,
    pub engine_registry: Arc<Mutex<Option<EngineRegistry>>>,
    pub recorder: Arc<Mutex<Option<AudioRecorder>>>,
}

pub fn run() {
    let paths = AppPaths::init().expect("Failed to initialize app paths");
    let settings = Settings::load(&paths).unwrap_or_default();
    let hardware = HardwareInfo::detect();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: Some("talkr.log".into()) }),
                ])
                .level(log::LevelFilter::Info)
                .build(),
        )
        .manage(AppState {
            paths,
            settings: Mutex::new(settings),
            hardware,
            download_manager: Arc::new(Mutex::new(None)),
            engine_registry: Arc::new(Mutex::new(None)),
            recorder: Arc::new(Mutex::new(None)),
        })
        .invoke_handler(tauri::generate_handler![
            get_hardware_info,
            get_app_paths,
            get_settings,
            update_settings,
            open_data_folder,
            list_catalog,
            list_installed_models,
            download_model,
            cancel_download,
            delete_model,
            import_local_model,
            list_voices,
            synthesize,
            start_recording,
            stop_recording,
            get_input_level,
            transcribe_file,
            cancel_job,
            history_list,
            history_get,
            history_delete,
            history_toggle_favorite,
            history_export,
            history_clear,
            get_storage_usage,
            run_retention,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = db::init(&handle) {
                    log::error!("Database initialization failed: {}", e);
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}