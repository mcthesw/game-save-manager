#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

#[macro_use]
extern crate rust_i18n;
use rust_i18n::t;
i18n!("../locales", fallback = ["en_US", "zh_SIMPLIFIED"]);

use config::get_config;
use tauri::Manager;

use log::{error, info};
use std::sync::Arc;

use crate::config::config_check;

mod backup;
mod cloud_sync;
mod config;
mod default_value;
mod errors;
mod ipc_handler;
mod quick_actions;
mod traits;

pub fn run() -> anyhow::Result<()> {
    info!("{}", t!("home.hello_world"));
    config_check()?;

    // 将 panic 信息记录到日志中
    std::panic::set_hook(Box::new(|panic_info| {
        // 获取 panic 的位置信息
        let location = panic_info.location().unwrap(); // 可以使用 unwrap_or_else() 处理 location 为 None 的情况

        // 获取 panic 的原因
        let message = panic_info
            .payload()
            .downcast_ref::<&str>()
            .unwrap_or(&"unknown reason"); // 处理 payload 不是 &str 类型的情况

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
            ipc_handler::get_locale_message
        ])
        .events(tauri_specta::collect_events![ipc_handler::IpcNotification])
        .constant("DEFAULT_CONFIG", config::Config::default());

    command_builder.export(
        specta_typescript::Typescript::default()
            .bigint(specta_typescript::BigIntExportBehavior::Number) // 设置 bigint 为 number
            .header("/* tslint:disable */"), // 添加头部，关闭TS的检查，避免编译失败
        "../src/bindings.ts",
    )?;

    // Init app
    let app = tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("logs".to_string()),
                    },
                ))
                .max_file_size(50_000 /* bytes */)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            app.get_webview_window("main")
                .expect("no main window")
                .set_focus()
                .expect("failed to set focus");
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(Arc::new(
            // 自动备份间隔，启动时默认为无（不自动备份）
            quick_actions::AutoBackupDuration::new(0),
        ))
        .invoke_handler(command_builder.invoke_handler())
        .setup(move |app| {
            // 处理快捷备份，包括托盘、定时、快捷键
            quick_actions::setup(app).expect("Cannot setup quick actions");
            // 注册命令
            command_builder.mount_events(app);
            Ok(())
        });

    // 处理退出到托盘（关闭窗口不退出）
    let config = get_config()?;
    if config.settings.exit_to_tray {
        app.build(tauri::generate_context!())
            .expect("Cannot build tauri app")
            .run(|_app_handle, event| {
                if let tauri::RunEvent::ExitRequested { api, .. } = event {
                    api.prevent_exit();
                }
            });
    } else {
        // 不需要退出到托盘
        app.run(tauri::generate_context!())
            .expect("error while running tauri application");
    }
    Ok(())
}

// fn init_log(config: &Config) {
//   use tracing_appender::rolling::{RollingFileAppender, Rotation};
//   use tracing_subscriber::{fmt, fmt::time, layer::SubscriberExt, util::SubscriberInitExt};

//   let console_layer = fmt::layer().with_timer(time::LocalTime::rfc_3339());

//   if config.settings.log_to_file {
//       let file_appender = RollingFileAppender::builder()
//           .rotation(Rotation::DAILY)
//           .filename_prefix("RGSM")
//           .filename_suffix("log")
//           .max_log_files(3)
//           .build("./log")
//           .expect("initializing rolling file appender failed");

//       let file_layer = fmt::layer()
//           .with_timer(time::LocalTime::rfc_3339())
//           .with_writer(file_appender)
//           .with_ansi(false)
//           .with_filter(LevelFilter::INFO);

//       tracing_subscriber::registry()
//           .with(console_layer)
//           .with(file_layer)
//           .init();
//   } else {
//       tracing_subscriber::registry().with(console_layer).init();
//   };
// }
