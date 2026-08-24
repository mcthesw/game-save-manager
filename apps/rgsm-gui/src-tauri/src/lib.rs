#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use rust_i18n::{i18n, t};
i18n!("../../../locales", fallback = ["en_US", "zh_SIMPLIFIED"]);

use rgsm_core::cloud_sync::SyncEventEmitter;
use rgsm_core::config::get_config;
use tauri::Manager;

use log::{error, info};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

use rgsm_core::config::config_check;

// GUI-specific modules
mod cloud_library;
mod cloud_operation;
mod commands;
mod hooks;
mod http;
mod process_util;
mod quick_actions;
mod snapshot_sync;
mod sound;

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

    fn emit_error(&self, error: &rgsm_core::cloud_sync::CloudSyncError) {
        let event = commands::CloudSyncErrorEvent {
            game_name: error.game_name.clone(),
            error: error.error.clone(),
        };
        http::emit(&self.app, "cloud-sync-error", &event);
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

pub fn run() -> anyhow::Result<()> {
    configure_development_data_dir()?;

    info!("{}", t!("home.hello_world"));
    config_check()?;

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
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::LogDir { file_name: None },
                )])
                .max_file_size(500_000 /* 5 KB */)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(10))
                .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
                .build(),
        );
    // Dual Host E2E starts two HTTP-only processes. The desktop single-instance
    // lock would silently exit the second process before it can bind.
    if std::env::var_os("RGSM_HTTP_HOST_ONLY").is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            app.get_webview_window("main")
                .expect("no main window")
                .set_focus()
                .expect("failed to set focus");
        }));
    }
    let app = builder
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(move |app| {
            let http_host = tauri::async_runtime::block_on(http::start(app.handle().clone()))?;
            info!(
                target: "rgsm::http",
                "HTTP Host listening on {}",
                http_host.base_url
            );
            let runtime_config = serde_json::json!({
                "apiBaseUrl": http_host.base_url,
                "token": http_host.api_token,
            });
            let initialization_script = format!(
                "window.__RGSM_RUNTIME__ = {};",
                serde_json::to_string(&runtime_config)?
            );
            app.get_webview_window("main")
                .ok_or_else(|| anyhow::anyhow!("Main window is not available"))?
                .eval(&initialization_script)?;
            if std::env::var_os("RGSM_HTTP_HOST_ONLY").is_some() {
                app.get_webview_window("main")
                    .ok_or_else(|| anyhow::anyhow!("Main window is not available"))?
                    .hide()?;
            }
            app.manage(http_host);
            let emitter = std::sync::Arc::new(TauriSyncEmitter {
                app: app.handle().clone(),
            });
            let cloud_sync_manager = rgsm_core::cloud_sync::CloudSyncTaskManager::new(emitter);
            let cloud_sync_worker = cloud_sync_manager.clone();
            tauri::async_runtime::spawn(async move {
                cloud_sync_worker.run().await;
            });
            let config = get_config().expect("Failed to load config while building hooks");
            let cloud_operation_state = cloud_operation::CloudOperationState::default();
            app.manage(cloud_operation_state.clone());
            let pipeline = hooks::build_builtin_pipeline(app.handle(), &config);
            app.manage(hooks::HookPipelineState::new(pipeline));

            app.manage(cloud_sync_manager);
            snapshot_sync::setup(app.handle().clone(), cloud_operation_state);

            sound::setup(app).expect("Cannot setup sound manager");
            // 处理快捷备份，包括托盘、定时、快捷键
            quick_actions::setup(app).expect("Cannot setup quick actions");
            Ok(())
        });

    // 处理退出到托盘（关闭窗口不退出）
    let config = get_config()?;
    info!(target: "rgsm::main", "App has started.");

    let exit_code = app
        .build(tauri::generate_context!())
        .expect("Cannot build tauri app")
        .run_return(move |handle, event| {
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                handle
                    .save_window_state(StateFlags::all())
                    .expect("Cannot save window state");
                // Only prevent exit when exit to tray is enabled and exit code is not provided(User requested exit)
                if config.settings.exit_to_tray && code.is_none() {
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

#[cfg(debug_assertions)]
fn validate_e2e_cutover_failpoint() -> anyhow::Result<()> {
    rgsm_core::cloud_sync::v2::validate_e2e_cutover_interrupt_env()
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!("{error}"))
}

#[cfg(not(debug_assertions))]
fn validate_e2e_cutover_failpoint() -> anyhow::Result<()> {
    Ok(())
}
