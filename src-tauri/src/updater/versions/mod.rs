//! Version-specific migration modules

/// Minimum supported version for auto-migration
pub const MIN_SUPPORTED_VERSION: &str = "1.0.0";
/// Current version from Cargo.toml
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Version 1.6.0 - introduced branch/tree view for snapshots
pub const VERSION_1_6_0: &str = "1.6.0";
/// Version 1.7.5 - introduced stable save-unit IDs
pub const VERSION_1_7_5: &str = "1.7.5";

// 1.4.X
mod v1_4_0;
pub use v1_4_0::{Config as Config1_4_0, VERSION as VERSION_1_4_0};
