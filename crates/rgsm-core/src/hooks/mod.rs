//! Lifecycle hook pipeline for extensibility.
//!
//! The [`LifecycleHook`] trait covers the snapshot, game and config events that
//! have a listener.
//! Hooks are dispatched by [`HookPipeline`] in priority order.
//!
//! Built-in hooks live in this module; GUI-specific hooks (notifications,
//! scheduler sync) stay in the GUI crate and are injected at startup.

pub mod contexts;
pub mod pipeline;
pub mod traits;

pub mod pre_restore_backup_hook;
mod v2_snapshot_sync_hook;

pub use contexts::*;
pub use pipeline::*;
pub use pre_restore_backup_hook::PreRestoreBackupHook;
pub use traits::{HookResult, LifecycleHook};
pub use v2_snapshot_sync_hook::{SnapshotSyncTarget, V2SnapshotSyncHook};
