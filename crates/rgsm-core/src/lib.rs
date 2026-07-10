//! rgsm-core: Core library for Game Save Manager
//!
//! Provides backup management, configuration, cloud synchronization,
//! and a lifecycle hook pipeline for extensibility.
//!
//! This crate has **zero** Tauri dependency. All platform-specific behavior
//! (notifications, event emission, UI dialogs) is injected via traits.

use rust_i18n::i18n;
i18n!("../../locales", fallback = ["en_US", "zh_SIMPLIFIED"]);

pub mod app_dirs;
pub mod backup;
pub mod cloud_sync;
pub mod config;
pub mod default_value;
pub mod device;
pub mod embedded_resources;
pub mod hooks;
pub mod ludusavi_manifest;
pub mod path_launcher;
pub mod path_pattern;
pub mod path_resolution;
pub mod path_resolver;
pub mod preclude;
pub mod services;
pub mod steam;
pub mod system_fonts;
pub mod updater;
pub mod vn_scanner;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn git_hash() -> &'static str {
    env!("RGSM_GIT_HASH")
}

/// Combined build identifier: `version (hash)`, e.g. `1.8.1 (a3b4c5d)`.
pub fn build_id() -> String {
    format!("{} ({})", version(), git_hash())
}
