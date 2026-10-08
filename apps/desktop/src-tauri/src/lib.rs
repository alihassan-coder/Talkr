use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tauri::Manager;
use tauri_plugin_log::{Target, TargetKind};

mod audio;
mod catalog;
mod commands;
mod config;
mod db;
mod dictation;
mod downloader;
mod engine_host;
mod error;
mod hardware;
mod paths;
mod tray;

use audio::record::AudioRecorder;
use commands::*;
use config::Settings;
use downloader::DownloadManager;
use engine_host::{EngineHost, GpuPolicy, NativeLauncher};
use hardware::HardwareProbe;
use paths::AppPaths;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

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

    pub fn is_cancelled(&self, job_id: &str) -> bool {
        lock(&self.jobs).get(job_id).is_some_and(|f| f.load(Ordering::Relaxed))
    }

    pub fn finish(&self, job_id: &str) {
        lock(&self.jobs).remove(job_id);
    }
}

pub struct AppState {
    pub paths: AppPaths,
    pub settings: Mutex<Settings>,
    /// Detected in the background at launch; see `HardwareProbe`.
    pub hardware: HardwareProbe,
    pub downloads: DownloadManager,
    pub engine: EngineHost,
    pub recorder: Mutex<AudioRecorder>,
    pub jobs: JobRegistry,
}

impl AppState {
    pub fn settings(&self) -> MutexGuard<'_, Settings> {
        lock(&self.settings)
    }

    pub fn recorder(&self) -> MutexGuard<'_, AudioRecorder> {
        lock(&self.recorder)
    }

    /// How speech to text should use the GPU, from the compute setting.
    pub fn gpu_policy(&self) -> GpuPolicy {
        match self.settings().device {
            config::DevicePreference::Auto => GpuPolicy::Auto,
            config::DevicePreference::Gpu => GpuPolicy::Prefer,
            config::DevicePreference::Cpu => GpuPolicy::Never,
        }
    }

    /// Inference threads: the user setting, or physical cores (capped at 8) when set to 0 (auto).
    /// Never more than the logical CPUs this process may use (cgroup and affinity limits
    /// included): extra threads only make whisper.cpp and onnxruntime slower.
    pub fn cpu_threads(&self) -> usize {
        let available = num_cpus::get().max(1);
        match self.settings().cpu_threads {
            0 => num_cpus::get_physical().clamp(1, 8).min(available),
            n => n.min(available),
        }
    }
}

/// How long a loaded model may sit unused before the engine process is stopped.
const ENGINE_IDLE: Duration = Duration::from_secs(5 * 60);
/// The same for an engine that holds no model (it only answered a device probe, e.g. for the
/// Settings page): it costs memory (a GPU context, with Vulkan) and is quick to start again.
const ENGINE_IDLE_NO_MODEL: Duration = Duration::from_secs(60);

/// Passed by the login item: start in the tray, without the window.
const STARTED_HIDDEN_ARG: &str = "--hidden";
/// Start a dictation, or finish the running one.
const DICTATE_ARG: &str = "--dictate";

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Whether closing the window should leave Talkr running in the tray.
fn keeps_running_in_tray(app: &tauri::AppHandle) -> bool {
    let state = app.state::<AppState>();
    let settings = state.settings();
    dictation::supported() && settings.dictation.enabled && settings.dictation.close_to_tray
}

/// Lock a mutex, recovering from poisoning (a panicked job must not brick the app).
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Show a native error dialog, then exit. For errors that leave Talkr unable to run at all, which
/// would otherwise end the process before any window appears and leave the user with nothing.
fn exit_with_startup_error(mut context: tauri::Context<tauri::Wry>, message: String) -> ! {
    eprintln!("Talkr could not start: {}", message);
    // Only the dialog: the main window would load a UI with no app state behind it.
    context.config_mut().app.windows.clear();
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let handle = app.handle().clone();
            app.dialog()
                .message(message)
                .title("Talkr could not start")
                .kind(MessageDialogKind::Error)
                .show(move |_| handle.exit(1));
            Ok(())
        })
        .run(context);
    if let Err(e) = result {
        eprintln!("Could not show the startup error dialog: {}", e);
    }
    std::process::exit(1)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let context = tauri::generate_context!();

    let paths = match AppPaths::init() {
        Ok(paths) => paths,
        Err(e) => exit_with_startup_error(
            context,
            format!(
                "Talkr could not set up its data folder.\n\n{}\n\nMake sure your home folder is writable, \
                 or set the TALKR_HOME environment variable to a writable folder, then start Talkr again.",
                e
            ),
        ),
    };
    // The logger starts with the app, below; keep what happened until then and log it in setup.
    let (settings, startup_notices) = match Settings::load_with_notices(&paths) {
        Ok(loaded) => loaded,
        Err(e) => (
            Settings::default(),
            vec![format!("Could not read {} ({}); using the default settings", paths.config_file.display(), e)],
        ),
    };
    // Detection runs external tools and can take seconds: never before the window opens.
    let hardware = HardwareProbe::new();
    hardware.warm_up();
    let log_dir = paths.logs.clone();
    let engine = EngineHost::new(Box::new(NativeLauncher::locate()), Some(paths.cache.join("engine-gpu-failed")));

    tauri::Builder::default()
        // First: a second launch hands over to the running Talkr (which shows its window) and
        // exits before it sets anything up.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // `talkr --dictate` toggles dictation in the running Talkr: bind it to a key where
            // Talkr cannot listen for one itself (Wayland), or use it from scripts.
            if args.iter().any(|a| a == DICTATE_ARG) {
                if let Some(d) = app.try_state::<dictation::Dictation>() {
                    d.toggle();
                }
            } else {
                show_main_window(app);
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![STARTED_HIDDEN_ARG]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
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
            engine,
            recorder: Mutex::new(AudioRecorder::new()),
            jobs: JobRegistry::default(),
        })
        .invoke_handler(tauri::generate_handler![
            get_hardware_info,
            get_engine_status,
            get_app_paths,
            read_history_audio,
            get_settings,
            update_settings,
            open_data_folder,
            stop_engine,
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
            save_export_bytes,
            history_clear,
            get_storage_usage,
            run_retention,
            dictation_status,
            list_microphones,
            dictation_capture_shortcut,
            dictation_stop,
            dictation_cancel,
            dictation_copy_last,
            dictation_warm_up,
            dictation_request_permission,
        ])
        .on_window_event(|window, event| {
            // With dictation on, closing the window keeps Talkr in the tray so the shortcut
            // still works. Quit from the tray menu.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" && keeps_running_in_tray(window.app_handle()) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(move |app| {
            // Panics go to talkr.log, so a failed run leaves a trace for bug reports. (The engine
            // process's own log arrives through engine_host.)
            let default_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                log::error!("panic: {}", info);
                default_hook(info);
            }));

            for notice in &startup_notices {
                log::warn!("{}", notice);
            }

            let state = app.state::<AppState>();

            // Schema setup and retention run off the main thread so they never delay the window.
            // History calls made meanwhile are safe: every connection migrates first (db module),
            // and one that fails here is retried by the next history call.
            let paths = state.paths.clone();
            let retention_days = state.settings().history_retention_days;
            std::thread::Builder::new().name("talkr-db-init".into()).spawn(move || {
                if let Err(e) = db::init(&paths) {
                    log::error!("{} (history will retry on next use)", e);
                    return;
                }
                match db::prune_old_history(&paths, retention_days) {
                    Ok(deleted) => remove_owned_audio(&paths, &deleted.audio_paths),
                    Err(e) => log::warn!("History retention failed: {}", e),
                }
            })?;

            // A loaded model holds hundreds of MB to GBs; give it back when nobody is using it,
            // unless it is the one dictation keeps ready.
            let handle = app.handle().clone();
            std::thread::Builder::new().name("talkr-engine-idle".into()).spawn(move || loop {
                std::thread::sleep(Duration::from_secs(30));
                let state = handle.state::<AppState>();
                let resident = state.engine.resident_model();
                if resident.is_some() && resident == dictation::warm_model(&state) {
                    continue;
                }
                let idle = if resident.is_some() { ENGINE_IDLE } else { ENGINE_IDLE_NO_MODEL };
                state.engine.stop_if_idle(idle);
            })?;

            app.manage(dictation::Dictation::start(app.handle()));
            if let Err(e) = tray::create(app.handle()) {
                log::error!("could not create the tray icon: {}", e);
            }
            // Launched at login: stay in the tray. Otherwise show the window (it is created
            // hidden so a login start never flashes it).
            let started_hidden = std::env::args().any(|a| a == STARTED_HIDDEN_ARG);
            let dictate_now = std::env::args().any(|a| a == DICTATE_ARG);
            if !(started_hidden || dictate_now) || !keeps_running_in_tray(app.handle()) {
                show_main_window(app.handle());
            }
            if dictate_now {
                app.state::<dictation::Dictation>().toggle();
            }
            Ok(())
        })
        .build(context)
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                app.state::<AppState>().engine.shutdown();
            }
        });
}

#[cfg(test)]
mod job_registry_tests {
    use super::*;

    #[test]
    fn cancel_reaches_the_registered_flag() {
        let jobs = JobRegistry::default();
        let flag = jobs.register("a");
        assert!(!jobs.is_cancelled("a"));
        assert!(jobs.cancel("a"));
        assert!(flag.load(Ordering::Relaxed));
        assert!(jobs.is_cancelled("a"));
    }

    #[test]
    fn unknown_and_finished_jobs() {
        let jobs = JobRegistry::default();
        assert!(!jobs.cancel("nope"));
        assert!(!jobs.is_cancelled("nope"));
        jobs.register("a");
        jobs.finish("a");
        assert!(!jobs.cancel("a"), "a finished job cannot be cancelled");
        assert!(!jobs.is_cancelled("a"));
        jobs.finish("a");
    }

    #[test]
    fn jobs_are_independent() {
        let jobs = JobRegistry::default();
        let a = jobs.register("a");
        let b = jobs.register("b");
        jobs.cancel("a");
        assert!(a.load(Ordering::Relaxed));
        assert!(!b.load(Ordering::Relaxed));
    }

    #[test]
    fn usable_from_many_threads() {
        let jobs = Arc::new(JobRegistry::default());
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let jobs = jobs.clone();
                std::thread::spawn(move || {
                    let id = format!("job-{i}");
                    let flag = jobs.register(&id);
                    assert!(jobs.cancel(&id));
                    assert!(flag.load(Ordering::Relaxed));
                    jobs.finish(&id);
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert!(lock(&jobs.jobs).is_empty());
    }
}
