use std::sync::Mutex;

use super::{Game, GameSnapshots};
use crate::preclude::BackupError;

// Only synchronous catalog read/modify/write sections share this boundary.
// Compression, hooks and network requests must finish before entering it.
static CATALOG_WRITE: Mutex<()> = Mutex::new(());

pub(crate) struct SnapshotCatalog {
    path: std::path::PathBuf,
    game_name: String,
    device_id: crate::device::DeviceId,
}

impl SnapshotCatalog {
    pub(crate) fn new(
        root: &std::path::Path,
        game: &Game,
        device_id: &crate::device::DeviceId,
    ) -> Self {
        Self {
            path: root
                .join(game.backup_dir_name().as_ref())
                .join("Backups.json"),
            game_name: game.name.clone(),
            device_id: device_id.clone(),
        }
    }
    pub(crate) fn path(&self) -> &std::path::Path {
        &self.path
    }
    pub(crate) fn read(&self) -> Result<GameSnapshots, BackupError> {
        let mut snapshots: GameSnapshots = serde_json::from_slice(&std::fs::read(&self.path)?)?;
        snapshots.normalize_heads_for_device(&self.device_id);
        Ok(snapshots)
    }
    fn write(&self, snapshots: &GameSnapshots) -> Result<(), BackupError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut snapshots = snapshots.clone();
        snapshots.normalize_heads_for_device(&self.device_id);
        crate::atomic_file::write_bytes_atomically(
            &self.path,
            &serde_json::to_vec_pretty(&snapshots)?,
        )?;
        Ok(())
    }
    pub(crate) fn update<E: From<BackupError>>(
        &self,
        update: impl FnOnce(&mut GameSnapshots) -> Result<(), E>,
    ) -> Result<GameSnapshots, E> {
        let _guard = CATALOG_WRITE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut current = match self.read() {
            Ok(current) => current,
            Err(BackupError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                GameSnapshots::new(self.game_name.clone())
            }
            Err(error) => return Err(error.into()),
        };
        update(&mut current)?;
        self.write(&current)?;
        Ok(current)
    }
}

impl Game {
    fn catalog(&self) -> Result<SnapshotCatalog, BackupError> {
        Ok(SnapshotCatalog::new(
            &crate::config::get_backup_path()?,
            self,
            crate::device::get_current_device_id(),
        ))
    }
    pub fn get_game_snapshots_info(&self) -> Result<GameSnapshots, BackupError> {
        self.catalog()?.read()
    }
    pub fn set_game_snapshots_info(&self, snapshots: &GameSnapshots) -> Result<(), BackupError> {
        let catalog = self.catalog()?;
        let _guard = CATALOG_WRITE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        catalog.write(snapshots)
    }
    pub(crate) fn update_game_snapshots_info<E: From<BackupError>>(
        &self,
        update: impl FnOnce(&mut GameSnapshots) -> Result<(), E>,
    ) -> Result<GameSnapshots, E> {
        self.catalog()?.update(update)
    }
}
