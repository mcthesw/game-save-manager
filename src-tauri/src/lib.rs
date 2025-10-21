#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use rust_i18n::{i18n, t};
i18n!("../locales", fallback = ["en_US", "zh_SIMPLIFIED"]);

use config::get_config;
use tauri::Manager;

use log::{error, info};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

use crate::config::config_check;

mod backup;
mod cloud_sync;
mod config;
mod default_value;
mod device;
mod ipc_handler;
mod path_resolver;
mod preclude;
mod quick_actions;
mod sound;
mod updater;

pub fn run() -> anyhow::Result<()> {
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

    let command_builder = tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
            ipc_handler::open_url,
            ipc_handler::open_file_or_folder,
            ipc_handler::choose_save_file,
            ipc_handler::choose_save_dir,
            ipc_handler::get_local_config,
            ipc_handler::add_game,
            ipc_handler::restore_snapshot,
            ipc_handler::delete_snapshot,
            ipc_handler::delete_game,
            ipc_handler::get_game_snapshots_info,
            ipc_handler::set_config,
            ipc_handler::reset_settings,
            ipc_handler::create_snapshot,
            ipc_handler::open_backup_folder,
            ipc_handler::check_cloud_backend,
            ipc_handler::cloud_upload_all,
            ipc_handler::cloud_download_all,
            ipc_handler::set_snapshot_description,
            ipc_handler::backup_all,
            ipc_handler::apply_all,
            ipc_handler::set_quick_backup_game,
            ipc_handler::resolve_path,
            ipc_handler::get_current_device_info,
            ipc_handler::toggle_quick_action_sound_preview,
            ipc_handler::stop_sound_playback,
            ipc_handler::choose_quick_action_sound_file,
        ])
        .events(tauri_specta::collect_events![
            ipc_handler::IpcNotification,
            quick_actions::QuickActionCompleted
        ])
        .constant("DEFAULT_CONFIG", config::Config::default());

    command_builder.export(
        specta_typescript::Typescript::default()
            .bigint(specta_typescript::BigIntExportBehavior::Number) // 设置 bigint 为 number
            .header("/* tslint:disable */"), // 添加头部，关闭TS的检查，避免编译失败
        "../src/bindings.ts",
    )?;

    // Init app
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(
            tauri_plugin_log::Builder::new()
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("logs".to_string()),
                    },
                ))
                .max_file_size(50_000 /* bytes */)
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
