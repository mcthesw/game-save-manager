//! Updater module for handling version migrations of various components
//!
//! This module provides functionality for:
//! - Version probing
//! - Data migration between versions
//! - Backup creation
//! - Component updates

mod backup;
mod cloud_config;
pub(crate) use backup::preserve_original;
pub mod migration;
pub mod probe;

#[allow(dead_code)]
pub mod versions;

pub(crate) use cloud_config::decode_legacy_cloud_config;
pub use migration::update_config;
