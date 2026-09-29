use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use tauri::Manager;
use tauri_plugin_log::{Target, TargetKind};

mod audio;
mod catalog;
mod commands;
mod config;
mod db;
mod downloader;
mod engines;
mod error;
mod hardware;
mod paths;

use audio::record::AudioRecorder;
use commands::*;
use config::Settings;
use downloader::DownloadManager;
use engines::EngineRegistry;
use hardware::HardwareInfo;
use paths::AppPaths;

/// Cancellation flags for running synthesis/transcription jobs.
#[derive(Default)]
pub struct JobRegistry {
    jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl JobRegistry {
    pub fn register(&self, job_id: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        lock(&self.jobs).insert(job_id.to_string(), flag.clone());
        flag
    }

    pub fn cancel(&self, job_id: &str) -> bool {
        match lock(&self.jobs).get(job_id) {
            Some(flag) => {
                flag.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    pub fn finish(&self, job_id: &str) {
        lock(&self.jobs).remove(job_id);
    }
}

pub struct AppState {
    pub paths: AppPaths,
    pub settings: Mutex<Settings>,
    pub hardware: HardwareInfo,
    pub downloads: DownloadManager,
    pub engines: Mutex<EngineRegistry>,
    pub recorder: Mutex<AudioRecorder>,
    pub jobs: JobRegistry,
}

impl AppState {
    pub fn settings(&self) -> MutexGuard<'_, Settings> {
        lock(&self.settings)
    }

    pub fn engines(&self) -> MutexGuard<'_, EngineRegistry> {
        lock(&self.engines)
    }

    pub fn recorder(&self) -> MutexGuard<'_, AudioRecorder> {
        lock(&self.recorder)
    }

    /// Inference threads: the user setting, or physical cores (capped at 8) when set to 0 (auto).
    pub fn cpu_threads(&self) -> usize {
        match self.settings().cpu_threads {
            0 => num_cpus::get_physical().clamp(1, 8),
            n => n,
        }
    }
}

/// Lock a mutex, recovering from poisoning (a panicked job must not brick the app).
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let paths = AppPaths::init().expect("Failed to initialize app paths");
    let settings = Settings::load(&paths).unwrap_or_default();
    let hardware = HardwareInfo::detect();
    let log_dir = paths.logs.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::Folder {
                        path: log_dir,
                        file_name: Some("talkr".into()),
                    }),
                    Target::new(TargetKind::Webview),
                ])
                .level(log::LevelFilter::Info)
                .build(),
        )
        .manage(AppState {
            paths,
            settings: Mutex::new(settings),
            hardware,
            downloads: DownloadManager::default(),
            engines: Mutex::new(EngineRegistry::default()),
            recorder: Mutex::new(AudioRecorder::new()),
            jobs: JobRegistry::default(),
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
            let state = app.state::<AppState>();
            // Initialize synchronously so the first history command never races the schema.
            if let Err(e) = db::init(&state.paths) {
                log::error!("Database initialization failed: {}", e);
            }
            let retention_days = state.settings().history_retention_days;
            match db::prune_old_history(&state.paths, retention_days) {
                Ok(deleted) => remove_owned_audio(&state.paths, &deleted.audio_paths),
                Err(e) => log::warn!("History retention failed: {}", e),
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
