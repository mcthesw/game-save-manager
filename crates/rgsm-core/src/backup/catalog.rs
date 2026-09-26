use std::sync::Mutex;

use super::{Game, GameSnapshots};
use crate::preclude::BackupError;

// Only synchronous catalog read/modify/write sections share this boundary.
// Compression, hooks and network requests must finish before entering it.
static CATALOG_WRITE: Mutex<()> = Mutex::new(());

impl Game {
    pub(crate) fn update_game_snapshots_info<E: From<BackupError>>(
        &self,
        update: impl FnOnce(&mut GameSnapshots) -> Result<(), E>,
    ) -> Result<GameSnapshots, E> {
        let _guard = CATALOG_WRITE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut current = match self.get_game_snapshots_info() {
            Ok(current) => current,
            Err(BackupError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                GameSnapshots::new(self.name.clone())
            }
            Err(error) => return Err(error.into()),
        };
        update(&mut current)?;
        self.set_game_snapshots_info(&current)?;
        Ok(current)
    }
}
