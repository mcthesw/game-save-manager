#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use rust_i18n::{i18n, t};
i18n!("../../../locales", fallback = ["en_US", "zh_SIMPLIFIED"]);

use rgsm_core::cloud_sync::SyncEventEmitter;
use rgsm_core::config::{Settings, get_config};
use tauri::Manager;

use log::{error, info, warn};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

use rgsm_core::config::config_check;

// GUI-specific modules
mod app_operations;
mod app_updates;
mod cloud_library;
mod cloud_library_cache;
mod cloud_operation;
mod commands;
mod hooks;
mod http;
mod main_window;
mod process_util;
mod progress_notices;
mod quick_actions;
mod remote_progress;
mod snapshot_sync;
mod sound;
mod transfer_job;

/// Tauri adapter for SyncEventEmitter — bridges core events to Tauri's event system.
struct TauriSyncEmitter {
    app: tauri::AppHandle,
}

impl SyncEventEmitter for TauriSyncEmitter {
    fn emit_status(&self, status: &rgsm_core::cloud_sync::CloudSyncStatus) {
        let event = commands::CloudSyncStatusEvent {
            active_jobs: status.active_jobs,
            current_description: status.current_description.clone(),
            jobs: status.jobs.clone(),
        };
        http::emit(&self.app, "cloud-sync-status", &event);
    }
}

pub fn configure_development_data_dir() -> anyhow::Result<()> {
    #[cfg(debug_assertions)]
    {
        validate_e2e_cutover_failpoint()?;
        let development_data_dir = match std::env::var_os("RGSM_E2E_APP_DATA_DIR") {
            Some(value) => {
                let path = std::path::PathBuf::from(value);
                if !path.is_absolute() {
                    anyhow::bail!(
                        "RGSM_E2E_APP_DATA_DIR must be an absolute path, got {}",
                        path.display()
                    );
                }
                path
            }
            None => {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../.rgsm-dev/app-data")
            }
        };
        rgsm_core::app_dirs::set_app_data_dir_override(development_data_dir)?;
    }
    Ok(())
}

pub fn prepare_http_host_configuration() -> anyhow::Result<()> {
    configure_development_data_dir()?;
    http::prepare_configuration()
}

pub fn openapi_json() -> anyhow::Result<String> {
    use utoipa::OpenApi;

    Ok(serde_json::to_string_pretty(
        &commands::http_commands::ApiDoc::openapi(),
    )?)
}

/// Unoptimised builds keep large futures on the stack, and a cloud round nests
/// deeply enough to overflow the default 2 MiB worker stack there.
fn use_roomy_async_runtime() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(8 * 1024 * 1024)
        .build()?;
    tauri::async_runtime::set(runtime.handle().clone());
    // Tauri only holds a handle, so the runtime must outlive the process.
    std::mem::forget(runtime);
    Ok(())
}

pub fn run() -> anyhow::Result<()> {
    let http_host_only = std::env::var_os("RGSM_HTTP_HOST_ONLY").is_some();
    configure_development_data_dir()?;
    use_roomy_async_runtime()?;

    // 将 panic 信息记录到日志中
    std::panic::set_hook(Box::new(|panic_info| {
        // 获取 panic 的位置信息
        let location = panic_info.location().unwrap(); // 可以使用 unwrap_or_else() 处理 location 为 None 的情况

        // 获取 panic 的原因
        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown reason".to_string()
        };

        // 使用 log crate 记录错误信息，并包含位置和原因
        error!(
            "{}:{}:{} - {}",
            location.file(),
            location.line(),
            location.column(),
            message,
        );
    }));

    // Init app
    let mut builder = tauri::Builder::default()
        .manage(app_operations::AppOperations::default())
        .manage(app_updates::InstallState::default())
        .manage(remote_progress::RemoteProgressState::new(!http_host_only))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::LogDir { file_name: None },
                )])
                // Dependencies log request traces and Debug-print their configuration,
                // which includes the WebDAV account name. Release logs stay at INFO so
                // they remain small and free of credentials.
                .level(if cfg!(debug_assertions) {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Info
                })
                .max_file_size(500_000 /* 500 KB */)
                // Keeps the active file plus nine rotated `.log` files. The plugin only
                // counts and prunes `*.log`, so a `.log.bak` file is never removed.
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(10))
                .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
                .build(),
        );
    // Dual Host E2E starts two HTTP-only processes. The desktop single-instance
    // lock would silently exit the second process before it can bind.
    if !http_host_only {
        builder = builder
            .plugin(tauri_plugin_global_shortcut::Builder::new().build())
            .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                // Windows delivers this callback inside the synchronous WM_COPYDATA
                // request sent by the second process. Defer WebView construction until
                // the callback returns so the sender can complete and release its lock.
                main_window::defer_show_main_window(app);
            }));
    }
    let app = builder.setup(move |app| {
        // Config migration rewrites user data, so it runs only after the
        // single-instance lock is held and the log plugin is writing.
        info!("{}", t!("home.hello_world"));
        let config_outcome = match config_check() {
            Ok(outcome) => outcome,
            Err(error) => {
                error!(target: "rgsm::main", "Startup configuration check failed: {error}");
                if !http_host_only {
                    rfd::MessageDialog::new()
                        .set_level(rfd::MessageLevel::Error)
                        .set_title(t!("backend.config.startup_failed_title").as_ref())
                        .set_description(
                            t!(
                                "backend.config.startup_failed_body",
                                error = error.to_string()
                            )
                            .as_ref(),
                        )
                        .show();
                }
                app.cleanup_before_exit();
                std::process::exit(1);
            }
        };
        if should_notify_config_update(http_host_only, config_outcome.config_migrated) {
            rgsm_core::preclude::show_notification(
                t!("backend.config.config_upgraded_title"),
                t!("backend.config.config_upgraded_body"),
            );
        }
        let http_host = tauri::async_runtime::block_on(http::start(app.handle().clone()))?;
        info!(
            target: "rgsm::http",
            "HTTP Host listening on {}",
            http_host.base_url
        );
        if !http_host_only {
            main_window::show_main_window(app.handle())?;
        }
        let emitter = std::sync::Arc::new(TauriSyncEmitter {
            app: app.handle().clone(),
        });
        let cloud_sync_manager = rgsm_core::cloud_sync::CloudSyncTaskManager::new(emitter);
        let config = get_config().expect("Failed to load config while building hooks");
        let cloud_operation_state = cloud_operation::CloudOperationState::default();
        app.manage(cloud_operation_state.clone());
        let pipeline = hooks::build_builtin_pipeline(app.handle(), &config);
        app.manage(hooks::HookPipelineState::new(pipeline));

        app.manage(cloud_sync_manager);
        snapshot_sync::setup(app.handle().clone(), cloud_operation_state);

        sound::setup(app).expect("Cannot setup sound manager");
        // 处理快捷备份，包括托盘、定时、快捷键
        quick_actions::setup(app, !http_host_only).expect("Cannot setup quick actions");
        Ok(())
    });

    // 处理退出到托盘（关闭窗口不退出）
    info!(target: "rgsm::main", "App has started.");

    let exit_code = app
        .build(tauri::generate_context!())
        .expect("Cannot build tauri app")
        .run_return(move |handle, event| {
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if !http_host_only {
                    handle
                        .save_window_state(StateFlags::all())
                        .expect("Cannot save window state");
                }
                // The HTTP-only host has no window by design, so its event loop
                // stays alive until the test harness explicitly terminates it.
                if should_prevent_exit(http_host_only, exit_to_tray_enabled(), code) {
                    api.prevent_exit();
                }
            }
        });

    if exit_code == 0 {
        info!(target: "rgsm::main", "App has exited successfully.");
        Ok(())
    } else {
        error!(target: "rgsm::main", "App has exited with error code {}.", exit_code);
        Err(anyhow::anyhow!(
            "App has exited with error code {}.",
            exit_code
        ))
    }
}

/// Read on every exit request: a setting saved while the app runs must govern
/// the very next window close.
fn exit_to_tray_enabled() -> bool {
    match get_config() {
        Ok(config) => config.settings.exit_to_tray,
        Err(error) => {
            warn!(target: "rgsm::main", "Cannot read the exit-to-tray setting, using its default: {error}");
            Settings::default().exit_to_tray
        }
    }
}

fn should_prevent_exit(http_host_only: bool, exit_to_tray: bool, code: Option<i32>) -> bool {
    code.is_none() && (http_host_only || exit_to_tray)
}

fn should_notify_config_update(http_host_only: bool, config_migrated: bool) -> bool {
    !http_host_only && config_migrated
}

#[cfg(debug_assertions)]
fn validate_e2e_cutover_failpoint() -> anyhow::Result<()> {
    rgsm_core::cloud_sync::v2::validate_e2e_cutover_interrupt_env()
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!("{error}"))
}

#[cfg(test)]
mod lifecycle_tests {
    use super::{
        exit_to_tray_enabled, get_config, should_notify_config_update, should_prevent_exit,
    };

    #[test]
    fn configuration_update_notice_is_owned_by_desktop_startup() {
        assert!(should_notify_config_update(false, true));
        assert!(!should_notify_config_update(false, false));
        assert!(!should_notify_config_update(true, true));
        assert!(!should_notify_config_update(true, false));
    }

    #[test]
    fn windowless_http_host_stays_alive_until_explicitly_stopped() {
        assert!(should_prevent_exit(true, false, None));
    }

    #[test]
    fn desktop_exit_policy_respects_tray_setting_and_explicit_exit() {
        assert!(should_prevent_exit(false, true, None));
        assert!(!should_prevent_exit(false, false, None));
        assert!(!should_prevent_exit(true, true, Some(0)));
    }

    #[test]
    fn exit_to_tray_follows_the_setting_saved_while_running() {
        let data_dir = temp_dir::TempDir::new().unwrap();
        rgsm_core::app_dirs::set_app_data_dir_override(data_dir.path().to_path_buf()).unwrap();
        rgsm_core::config::config_check().unwrap();
        assert!(exit_to_tray_enabled());

        let mut config = get_config().unwrap();
        config.settings.exit_to_tray = false;
        rgsm_core::config::set_config_local(&config).unwrap();
        assert!(!exit_to_tray_enabled());

        config.settings.exit_to_tray = true;
        rgsm_core::config::set_config_local(&config).unwrap();
        assert!(exit_to_tray_enabled());
    }
}
