//! Updater module for handling version migrations of various components
//!
//! This module provides functionality for:
//! - Version probing
//! - Data migration between versions
//! - Backup creation
//! - Component updates

pub mod migration;
pub mod probe;

#[allow(dead_code)]
pub mod versions;

pub use migration::update_config;
