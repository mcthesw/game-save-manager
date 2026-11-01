//! Per-game exclusion between capturing and restoring the live save.
//!
//! Local-only: it never waits on or takes cloud locks, so cloud availability
//! cannot affect local backup and restore.

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
};

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use crate::backup::Game;

static LOCKS: LazyLock<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>> = LazyLock::new(Mutex::default);

fn lock_for(game: &Game) -> Arc<AsyncMutex<()>> {
    LOCKS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .entry(game.backup_dir_name().into_owned())
        .or_default()
        .clone()
}

/// Waits until no other capture or restore of this game's save is running.
pub(super) async fn lock_game(game: &Game) -> OwnedMutexGuard<()> {
    lock_for(game).lock_owned().await
}

/// `None` when the game is busy; automatic backups skip instead of queueing.
pub(super) fn try_lock_game(game: &Game) -> Option<OwnedMutexGuard<()>> {
    lock_for(game).try_lock_owned().ok()
}
