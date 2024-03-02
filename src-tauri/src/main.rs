#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::sync::{Arc, Mutex};

use config::get_config;

mod archive;
mod backup;
mod cloud;
mod config;
mod errors;
mod ipc_handler;
mod tray;

fn main() {
    let app = tauri::Builder::default()
        .manage(Arc::new(Mutex::new(tray::QuickBackupState::default())))
        .invoke_handler(tauri::generate_handler![
            ipc_handler::local_config_check,
            ipc_handler::open_url,
            ipc_handler::choose_save_file,
            ipc_handler::choose_save_dir,
            ipc_handler::get_local_config,
            ipc_handler::add_game,
            ipc_handler::local_config_check,
            ipc_handler::apply_backup,
            ipc_handler::delete_backup,
            ipc_handler::delete_game,
            ipc_handler::get_backup_list_info,
            ipc_handler::set_config,
            ipc_handler::reset_settings,
            ipc_handler::backup_save,
            ipc_handler::open_backup_folder,
            ipc_handler::check_cloud_backend,
            ipc_handler::cloud_upload_all,
            ipc_handler::cloud_download_all,
            ipc_handler::set_backup_describe,
            ipc_handler::backup_all,
            ipc_handler::apply_all,
            ipc_handler::set_quick_backup_game,
        ]);

    // 只允许运行一个实例
    let app = app.plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {}));

    // 处理退出到托盘
    if let Ok(config) = get_config() {
        if config.settings.exit_to_tray {
            app.system_tray(tray::get_tray())
                .on_system_tray_event(tray::tray_event_handler)
                .setup(tray::setup_timer)
                .build(tauri::generate_context!())
                .expect("Cannot build tauri app")
                .run(|_app_handle, event| {
                    if let tauri::RunEvent::ExitRequested { api, .. } = event {
                        api.prevent_exit();
                    }
                });
            return;
        }
    }
    // 不需要退出到托盘
    app.run(tauri::generate_context!())
        .expect("error while running tauri application")
}
