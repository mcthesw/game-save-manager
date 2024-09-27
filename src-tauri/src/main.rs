#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

#[macro_use]
extern crate rust_i18n;
use rust_i18n::t;
i18n!("../locales", fallback = ["en_US", "zh_SIMPLIFIED"]);

use config::{get_config, Config};

use std::sync::{Arc, Mutex};
use tauri::api::notification::Notification;
use tracing::info;
use tracing_subscriber::{filter::LevelFilter, Layer};

use crate::config::config_check;

mod backup;
mod cloud_sync;
mod config;
mod default_value;
mod errors;
mod ipc_handler;
mod quick_actions;
mod traits;

fn main() {
    // Init config
    if let Err(e) = config_check() {
        panic!("Check on config file filed: {:?}", e);
    }
    let config = get_config().unwrap_or_else(|e| panic!("Cannot load config file: {:?}", e));

    // Init log
    init_log(&config);
    info!("{}", t!("home.hello_world"));

    // Init app
    let app = tauri::Builder::default()
        .manage(Arc::new(
            // 自动备份间隔，启动时默认为无（不自动备份）
            quick_actions::AutoBackupDuration::new(0),
        ))
        .invoke_handler(tauri::generate_handler![
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
        ]);

    // 只允许运行一个实例
    let app = app.plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {}));

    // 处理退出到托盘
    if config.settings.exit_to_tray {
        app.system_tray(quick_actions::get_tray())
            .on_system_tray_event(quick_actions::tray_event_handler)
            .setup(quick_actions::setup_timer)
            .build(tauri::generate_context!())
            .expect("Cannot build tauri app")
            .run(|_app_handle, event| {
                if let tauri::RunEvent::ExitRequested { api, .. } = event {
                    api.prevent_exit();
                }
            });
        return;
    }

    // 不需要退出到托盘
    app.run(tauri::generate_context!())
        .expect("error while running tauri application");

    // 需要初始化Notification，否则第一次提示不会显示
    Notification::new("Init Info")
        .title("Init")
        .body("Initiating notification module")
        .show()
        .expect("Cannot show notification");
}

fn init_log(config: &Config) {
    use tracing_appender::rolling::{RollingFileAppender, Rotation};
    use tracing_subscriber::{fmt, fmt::time, layer::SubscriberExt, util::SubscriberInitExt};

    let console_layer = fmt::layer().with_timer(time::LocalTime::rfc_3339());

    if config.settings.log_to_file {
        let file_appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("RGSM")
            .filename_suffix("log")
            .max_log_files(3)
            .build("./log")
            .expect("initializing rolling file appender failed");

        let file_layer = fmt::layer()
            .with_timer(time::LocalTime::rfc_3339())
            .with_writer(file_appender)
            .with_ansi(false)
            .with_filter(LevelFilter::INFO);

        tracing_subscriber::registry()
            .with(console_layer)
            .with(file_layer)
            .init();
    } else {
        tracing_subscriber::registry().with(console_layer).init();
    };
}
