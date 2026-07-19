#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use rust_i18n::{i18n, t};
i18n!("../../../locales", fallback = ["en_US", "zh_SIMPLIFIED"]);

use rgsm_core::cloud_sync::SyncEventEmitter;
use rgsm_core::config::{cloud_namespace_generation, get_config};
use tauri::Manager;

use log::{error, info, warn};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

use rgsm_core::config::config_check;

// GUI-specific modules
#[cfg(debug_assertions)]
mod bindings_format;
mod cloud_library;
mod hooks;
mod ipc_handler;
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
        use tauri_specta::Event;
        if let Err(err) = (ipc_handler::CloudSyncStatusEvent {
            active_jobs: status.active_jobs,
            current_description: status.current_description.clone(),
            jobs: status.jobs.clone(),
        })
        .emit(&self.app)
        {
            warn!(
                target: "rgsm::cloud::emitter",
                "Failed to emit cloud sync status: {err:?}"
            );
        }
    }

    fn emit_error(&self, error: &rgsm_core::cloud_sync::CloudSyncError) {
        use tauri_specta::Event;
        if let Err(err) = (ipc_handler::CloudSyncErrorEvent {
            game_name: error.game_name.clone(),
            error: error.error.clone(),
        })
        .emit(&self.app)
        {
            warn!(
                target: "rgsm::cloud::emitter",
                "Failed to emit cloud sync error: {err:?}"
            );
        }
    }
}

pub fn run() -> anyhow::Result<()> {
    info!("{}", t!("home.hello_world"));
    let config_status = config_check()?;

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

    let command_builder = tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
            ipc_handler::open_url,
            ipc_handler::get_build_info,
            ipc_handler::open_file_or_folder,
            ipc_handler::get_app_log_dir,
            ipc_handler::choose_save_file,
            ipc_handler::choose_save_dir,
            ipc_handler::get_local_config,
            ipc_handler::add_game,
            ipc_handler::update_game,
            ipc_handler::restore_snapshot,
            ipc_handler::delete_snapshot,
            ipc_handler::batch_delete_snapshots,
            ipc_handler::get_cloud_namespace_generation,
            ipc_handler::delete_game,
            ipc_handler::get_game_snapshots_info,
            ipc_handler::verify_archive_integrity,
            ipc_handler::set_config,
            ipc_handler::reset_settings,
            ipc_handler::create_snapshot,
            ipc_handler::open_backup_folder,
            ipc_handler::get_game_extra_backups,
            ipc_handler::delete_extra_backup,
            ipc_handler::restore_extra_backup,
            ipc_handler::open_extra_backup_folder,
            ipc_handler::check_cloud_backend,
            ipc_handler::inspect_cloud_library,
            ipc_handler::create_cloud_library,
            ipc_handler::review_cloud_library_join,
            ipc_handler::join_cloud_library,
            ipc_handler::review_cloud_library_cutover,
            ipc_handler::cutover_cloud_library,
            ipc_handler::get_cloud_archive_library,
            ipc_handler::review_v2_game_progress,
            ipc_handler::keep_v2_local_progress,
            ipc_handler::accept_v2_remote_progress,
            ipc_handler::preview_materialize_all,
            ipc_handler::upload_cloud_archive,
            ipc_handler::download_cloud_archive,
            ipc_handler::delete_v2_snapshot,
            ipc_handler::set_shared_snapshot_retention,
            ipc_handler::set_snapshot_retention_protected,
            ipc_handler::get_current_device_game_statuses,
            ipc_handler::set_device_game_visibility,
            ipc_handler::set_device_game_managed,
            ipc_handler::evict_local_archive,
            ipc_handler::materialize_all_cloud_archives,
            ipc_handler::set_game_sync_mode,
            ipc_handler::cloud_upload_all,
            ipc_handler::cloud_download_all,
            ipc_handler::cancel_cloud_sync,
            ipc_handler::set_snapshot_description,
            ipc_handler::backup_all,
            ipc_handler::apply_all,
            ipc_handler::set_quick_backup_game,
            ipc_handler::set_game_auto_backup,
            ipc_handler::set_game_automation,
            ipc_handler::set_game_auto_save_settings,
            ipc_handler::set_snapshot_created_by,
            ipc_handler::get_auto_backup_status,
            ipc_handler::list_running_processes,
            ipc_handler::resolve_path,
            ipc_handler::get_current_device_info,
            ipc_handler::toggle_quick_action_sound_preview,
            ipc_handler::stop_sound_playback,
            ipc_handler::choose_quick_action_sound_file,
            ipc_handler::set_snapshot_head,
            ipc_handler::detach_snapshot,
            ipc_handler::create_snapshot_at,
            ipc_handler::fetch_ludusavi_games,
            ipc_handler::get_game_save_paths,
            ipc_handler::get_path_placeholder_catalog,
            ipc_handler::preview_save_unit_resolution,
            ipc_handler::set_game_device_binding,
            ipc_handler::save_restore_mapping,
            ipc_handler::get_ludusavi_manifest_status,
            ipc_handler::update_ludusavi_manifest,
            ipc_handler::reset_ludusavi_manifest_to_bundled,
            ipc_handler::check_paths,
            ipc_handler::detect_game_roots,
            ipc_handler::detect_store_user_ids,
            ipc_handler::get_system_fonts,
            ipc_handler::get_sync_state,
            ipc_handler::scan_vns,
            ipc_handler::list_config_backups,
            ipc_handler::restore_config_backup,
            ipc_handler::sync_game,
            ipc_handler::resolve_game_sync_conflict,
            ipc_handler::sync_config,
        ])
        .events(tauri_specta::collect_events![
            ipc_handler::IpcNotification,
            quick_actions::QuickActionCompleted,
            ipc_handler::CloudSyncStatusEvent,
            ipc_handler::CloudSyncErrorEvent
        ])
        .constant("DEFAULT_CONFIG", rgsm_core::config::Config::default());

    #[cfg(debug_assertions)]
    command_builder.export(
        specta_typescript::Typescript::default()
            .bigint(specta_typescript::BigIntExportBehavior::Number) // 设置 bigint 为 number
            .formatter(bindings_format::strip_trailing_whitespace)
            .header("/* tslint:disable */"), // 添加头部，关闭TS的检查，避免编译失败
        "../src/bindings.ts",
    )?;

    // Init app
    let app = tauri::Builder::default()
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
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            app.get_webview_window("main")
                .expect("no main window")
                .set_focus()
                .expect("failed to set focus");
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(command_builder.invoke_handler())
        .setup(move |app| {
            let emitter = std::sync::Arc::new(TauriSyncEmitter {
                app: app.handle().clone(),
            });
            let cloud_sync_manager = rgsm_core::cloud_sync::CloudSyncTaskManager::new(emitter);
            let cloud_sync_worker = cloud_sync_manager.clone();
            tauri::async_runtime::spawn(async move {
                cloud_sync_worker.run().await;
            });
            let config = get_config().expect("Failed to load config while building hooks");
            let snapshot_sync_runtime = snapshot_sync::SnapshotSyncRuntimeState::default();
            app.manage(snapshot_sync_runtime.clone());
            let pipeline =
                hooks::build_builtin_pipeline(app.handle(), cloud_sync_manager.clone(), &config);
            app.manage(hooks::HookPipelineState::new(pipeline));

            let startup_cloud_sync_manager = cloud_sync_manager.clone();
            app.manage(cloud_sync_manager);
            if config_status.config_migrated {
                let migrated_config = config.clone();
                let generation =
                    cloud_namespace_generation().expect("Failed to load Cloud Library generation");
                tauri::async_runtime::spawn(async move {
                    startup_cloud_sync_manager
                        .enqueue_config_upload_if_enabled(
                            &migrated_config,
                            generation,
                            "config_migration",
                        )
                        .await;
                });
            }
            snapshot_sync::setup(app.handle().clone(), snapshot_sync_runtime);

            sound::setup(app).expect("Cannot setup sound manager");
            // 处理快捷备份，包括托盘、定时、快捷键
            quick_actions::setup(app).expect("Cannot setup quick actions");
            // 注册命令
            command_builder.mount_events(app);
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
