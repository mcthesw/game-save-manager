//! Lifecycle hook pipeline for extensibility.
//!
//! The [`LifecycleHook`] trait (renamed from the original `SnapshotHook`) covers
//! all domain events: snapshot, game, config, and sync lifecycle.
//! Hooks are dispatched by [`HookPipeline`] in priority order.
//!
//! Built-in hooks live in this module; GUI-specific hooks (notifications,
//! scheduler sync) stay in the GUI crate and are injected at startup.

pub mod contexts;
pub mod pipeline;
pub mod traits;

pub mod checksum_hook;
pub mod cloud_sync_hook;
pub mod pre_restore_backup_hook;

pub use checksum_hook::{ArchiveHashHook, ArchiveVerifyHook};
pub use cloud_sync_hook::CloudSyncEnqueueHook;
pub use contexts::*;
pub use pipeline::*;
pub use pre_restore_backup_hook::PreRestoreBackupHook;
pub use traits::{HookResult, LifecycleHook, SnapshotHook, SyncJobQueue};
