mod inventory;
mod model;
#[cfg(test)]
mod tests;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use crate::{
    backup::{
        ArchiveFormat, ArchiveMigrationError, ArchiveMigrationInput, CompressionPreset, Game,
        catalog::SnapshotCatalog, compute_file_hash, convert_snapshot_archive,
    },
    device::DeviceId,
    preclude::BackupError,
};

pub use model::{
    LocalUpgradeView, UpgradeIssue, UpgradeIssueKind, UpgradePendingItem, UpgradeRetry,
    UpgradeUnitChoice,
};
use model::{UpgradeItem, UpgradeItemState, UpgradeJournal};

// One local application operation at a time. Each archive is independently
// checkpointed; pausing or closing the UI simply stops requesting more steps.
static UPGRADE_OPERATION: Mutex<()> = Mutex::new(());
const JOURNAL_VERSION: u32 = 1;

pub struct LocalUpgrade {
    root: PathBuf,
    device_id: DeviceId,
    games: Vec<Game>,
    preset: CompressionPreset,
}

impl LocalUpgrade {
    pub fn new(
        root: PathBuf,
        device_id: DeviceId,
        games: Vec<Game>,
        preset: CompressionPreset,
    ) -> Self {
        Self {
            root,
            device_id,
            games,
            preset,
        }
    }

    pub fn inspect(&self) -> Result<LocalUpgradeView, BackupError> {
        let (journal, problems) = self.inventory()?;
        Ok(self.view(&journal, problems))
    }

    pub fn start(&self) -> Result<LocalUpgradeView, BackupError> {
        let _guard = UPGRADE_OPERATION
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let (mut journal, problems) = self.inventory()?;
        journal.started = true;
        self.store(&journal)?;
        Ok(self.view(&journal, problems))
    }

    /// Finish at most one archive. No persistent worker, cloud operation or
    /// automatic deletion runs when the player leaves this flow.
    pub fn step(&self) -> Result<LocalUpgradeView, BackupError> {
        let _guard = UPGRADE_OPERATION
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let (mut journal, problems) = self.inventory()?;
        if !journal.started {
            return Err(unexpected("local upgrade has not been started"));
        }
        let Some(index) = journal.items.iter().position(|item| {
            matches!(
                item.state,
                UpgradeItemState::Queued | UpgradeItemState::Prepared
            )
        }) else {
            return Ok(self.view(&journal, problems));
        };
        // Persist the output identity before touching an archive discovered
        // since the previous step, so an interrupted conversion can resume it.
        self.store(&journal)?;
        let mut item = journal.items[index].clone();
        let result = self.prepare(&mut item);
        if result.is_ok() {
            // A crash before catalog replacement resumes with a verified file;
            // a crash after replacement recognizes the same immutable filename.
            journal.items[index] = item.clone();
            self.store(&journal)?;
            if let Err(error) = self.commit(&item) {
                log::warn!("Local archive upgrade catalog commit failed: {error}");
                item.state = UpgradeItemState::Pending;
                item.issue = Some(issue(UpgradeIssueKind::HistoryChanged));
            } else {
                item.state = UpgradeItemState::Completed;
                item.issue = None;
                self.retain_original(&item);
            }
        } else if let Err(error) = result {
            log::warn!("Local archive upgrade needs attention: {error}");
            item.state = UpgradeItemState::Pending;
            item.issue = Some(match error {
                ArchiveMigrationError::Association(entry) => UpgradeIssue {
                    kind: UpgradeIssueKind::AssociationRequired,
                    archive_entry: Some(entry),
                },
                ArchiveMigrationError::MultipleInstances(_) => {
                    issue(UpgradeIssueKind::MultipleInstances)
                }
                ArchiveMigrationError::Io(error)
                    if error.kind() == std::io::ErrorKind::NotFound =>
                {
                    issue(UpgradeIssueKind::MissingArchive)
                }
                _ => issue(UpgradeIssueKind::UnreadableArchive),
            });
        }
        journal.items[index] = item;
        self.store(&journal)?;
        Ok(self.view(&journal, problems))
    }

    pub fn retry(&self, request: UpgradeRetry) -> Result<LocalUpgradeView, BackupError> {
        let _guard = UPGRADE_OPERATION
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let (mut journal, problems) = self.inventory()?;
        if request.item_id.starts_with("catalog:") {
            return Ok(self.view(&journal, problems));
        }
        let item = journal
            .items
            .iter_mut()
            .find(|item| item.id == request.item_id)
            .ok_or_else(|| unexpected("upgrade item is missing"))?;
        if item.state != UpgradeItemState::Pending {
            return Err(unexpected("upgrade item does not need retry"));
        }
        if let Some(path) = request.replacement_path {
            item.replacement_path = (!path.trim().is_empty()).then_some(path);
        }
        if let (Some(entry), Some(unit)) = (request.archive_entry, request.save_unit_id) {
            item.associations.insert(entry, unit);
        }
        item.unpacked_size = crate::backup::inspect_archive_size(
            Path::new(
                item.replacement_path
                    .as_deref()
                    .unwrap_or(&item.source_path),
            ),
            item.snapshot.archive_format,
        )
        .ok();
        item.state = if item.output_hash.is_some() {
            UpgradeItemState::Prepared
        } else {
            UpgradeItemState::Queued
        };
        item.issue = None;
        self.store(&journal)?;
        Ok(self.view(&journal, problems))
    }

    /// Explicit player action, only after the new archive and its catalog row
    /// still verify. Replacement copies selected for retry are never deleted.
    pub fn cleanup_originals(&self) -> Result<LocalUpgradeView, BackupError> {
        let _guard = UPGRADE_OPERATION
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let (mut journal, problems) = self.inventory()?;
        for index in 0..journal.items.len() {
            let item = &mut journal.items[index];
            if item.state != UpgradeItemState::Completed {
                continue;
            }
            if self.cleanup_one(item).is_ok() {
                item.state = UpgradeItemState::Cleaned;
                item.issue = None;
            } else {
                item.issue = Some(issue(UpgradeIssueKind::OriginalChanged));
            }
            self.store(&journal)?;
        }
        Ok(self.view(&journal, problems))
    }

    fn prepare(&self, item: &mut UpgradeItem) -> Result<(), ArchiveMigrationError> {
        let destination = self.destination(item)?;
        if item.state == UpgradeItemState::Prepared {
            if item.output_hash.as_deref() != Some(&compute_file_hash(&destination)?) {
                return Err(ArchiveMigrationError::Invalid(
                    "prepared archive changed".into(),
                ));
            }
            return Ok(());
        }
        let game = self
            .games
            .iter()
            .find(|game| game.backup_dir_name().as_ref() == item.game_id)
            .ok_or_else(|| ArchiveMigrationError::Association(item.game_id.clone()))?;
        let catalog = SnapshotCatalog::new(&self.root, game, &self.device_id);
        let backup = self.journal_dir().join("catalogs").join(format!(
            "{}.json",
            crate::device::encode_device_id(&item.game_id)
        ));
        if !item.extra_backup && !backup.exists() {
            fs::create_dir_all(backup.parent().expect("catalog backup directory"))?;
            crate::atomic_file::write_bytes_atomically(&backup, &fs::read(catalog.path())?)?;
        }
        let source = Path::new(
            item.replacement_path
                .as_deref()
                .unwrap_or(&item.source_path),
        );
        fs::metadata(source)?;
        item.original_hash = if Path::new(&item.source_path).is_file() {
            Some(compute_file_hash(Path::new(&item.source_path))?)
        } else {
            None
        };
        fs::create_dir_all(destination.parent().expect("game directory"))?;
        item.output_size = convert_snapshot_archive(ArchiveMigrationInput {
            game,
            snapshot: &item.snapshot,
            device_id: &self.device_id,
            source,
            destination: &destination,
            preset: self.preset,
            associations: &item.associations,
        })?;
        item.output_hash = Some(compute_file_hash(&destination)?);
        item.state = UpgradeItemState::Prepared;
        Ok(())
    }

    fn commit(&self, item: &UpgradeItem) -> Result<(), BackupError> {
        if item.extra_backup {
            return Ok(());
        }
        let game = self
            .games
            .iter()
            .find(|game| game.backup_dir_name().as_ref() == item.game_id)
            .ok_or_else(|| unexpected("game definition changed"))?;
        let destination = self
            .destination(item)
            .map_err(|error| unexpected(&error.to_string()))?;
        SnapshotCatalog::new(&self.root, game, &self.device_id).update::<BackupError>(
            |catalog| {
                let snapshot = catalog
                    .backups
                    .iter_mut()
                    .find(|snapshot| snapshot.date == item.snapshot.date)
                    .ok_or_else(|| unexpected("snapshot history changed"))?;
                let already_committed = snapshot.archive_name == item.output_name
                    && snapshot.archive_hash == item.output_hash;
                if !already_committed
                    && (snapshot.archive_name != item.snapshot.archive_name
                        || snapshot.archive_format != item.snapshot.archive_format
                        || snapshot.archive_hash != item.snapshot.archive_hash)
                {
                    return Err(unexpected("snapshot archive changed"));
                }
                snapshot.archive_format = ArchiveFormat::SevenZ;
                snapshot.archive_name = item.output_name.clone();
                snapshot.path = destination.to_string_lossy().into_owned();
                snapshot.archive_hash = item.output_hash.clone();
                snapshot.size = item.output_size;
                Ok(())
            },
        )?;
        Ok(())
    }

    fn cleanup_one(&self, item: &UpgradeItem) -> Result<(), BackupError> {
        let destination = self
            .destination(item)
            .map_err(|error| unexpected(&error.to_string()))?;
        if item.output_hash.as_deref() != Some(&compute_file_hash(&destination)?) {
            return Err(unexpected("converted archive changed"));
        }
        if !item.extra_backup {
            let game = self
                .games
                .iter()
                .find(|game| game.backup_dir_name().as_ref() == item.game_id)
                .ok_or_else(|| unexpected("game was removed"))?;
            let catalog = SnapshotCatalog::new(&self.root, game, &self.device_id).read()?;
            if !catalog.backups.iter().any(|snapshot| {
                snapshot.date == item.snapshot.date
                    && snapshot.archive_name == item.output_name
                    && snapshot.archive_hash == item.output_hash
            }) {
                return Err(unexpected("converted snapshot is no longer in the catalog"));
            }
        }
        let original = self.original_path(item);
        if original.exists() {
            if item.original_hash.as_deref() != Some(&compute_file_hash(original)?) {
                return Err(unexpected("original archive changed"));
            }
            fs::remove_file(original)?;
        }
        Ok(())
    }

    fn destination(&self, item: &UpgradeItem) -> Result<PathBuf, ArchiveMigrationError> {
        let name = item
            .output_name
            .as_deref()
            .ok_or_else(|| ArchiveMigrationError::Invalid("output name is missing".into()))?;
        let directory = self.root.join(&item.game_id);
        Ok(if item.extra_backup {
            directory.join("extra_backup").join(name)
        } else {
            directory.join(name)
        })
    }
    fn original_path<'a>(&self, item: &'a UpgradeItem) -> &'a Path {
        let retained = Path::new(&item.retained_path);
        if retained.exists() {
            retained
        } else {
            Path::new(&item.source_path)
        }
    }
    fn retain_original(&self, item: &UpgradeItem) {
        let source = Path::new(&item.source_path);
        let retained = Path::new(&item.retained_path);
        if !source.exists() || retained.exists() {
            return;
        }
        if compute_file_hash(source).ok().as_deref() != item.original_hash.as_deref() {
            return;
        }
        if let Some(parent) = retained.parent() {
            let result = fs::create_dir_all(parent).and_then(|_| fs::rename(source, retained));
            if let Err(error) = result {
                log::warn!("Original archive remains at its previous path: {error}");
            }
        }
    }
    fn journal_dir(&self) -> PathBuf {
        self.root.join(".rgsm-upgrade")
    }
    fn load(&self) -> Result<UpgradeJournal, BackupError> {
        let path = self.journal_dir().join("state.json");
        if !path.exists() {
            return Ok(UpgradeJournal {
                schema_version: JOURNAL_VERSION,
                ..Default::default()
            });
        }
        let journal: UpgradeJournal = serde_json::from_slice(&fs::read(path)?)?;
        if journal.schema_version != JOURNAL_VERSION {
            return Err(unexpected("unsupported local upgrade journal"));
        }
        Ok(journal)
    }
    fn store(&self, journal: &UpgradeJournal) -> Result<(), BackupError> {
        fs::create_dir_all(self.journal_dir())?;
        crate::atomic_file::write_bytes_atomically(
            &self.journal_dir().join("state.json"),
            &serde_json::to_vec_pretty(journal)?,
        )?;
        Ok(())
    }
}

fn unexpected(message: &str) -> BackupError {
    BackupError::Unexpected(anyhow::anyhow!(message.to_string()))
}
fn issue(kind: UpgradeIssueKind) -> UpgradeIssue {
    UpgradeIssue {
        kind,
        archive_entry: None,
    }
}
