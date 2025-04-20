//! Version-specific migration modules

/// Minimum supported version for auto-migration
pub const MIN_SUPPORTED_VERSION: &str = "1.0.0";
/// Current version from Cargo.toml
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

// 1.4.X
mod v1_4_0;
pub use v1_4_0::{Config as Config1_4_0, VERSION as VERSION_1_4_0};
