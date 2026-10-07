use std::{collections::BTreeMap, fs};

use crate::{
    backup::{
        ArchiveBackend, ArchiveFormat, SevenZBackend, Snapshot, catalog::SnapshotCatalog,
        inspect_archive_size, new_archive_name, snapshot_archive_path,
    },
    preclude::BackupError,
};

use super::{LocalUpgrade, model::*};

impl LocalUpgrade {
    pub(super) fn inventory(
        &self,
    ) -> Result<(UpgradeJournal, Vec<UpgradePendingItem>), BackupError> {
        let mut journal = self.load()?;
        let mut problems = Vec::new();
        for game in &self.games {
            let catalog = SnapshotCatalog::new(&self.root, game, &self.device_id);
            let snapshots = match catalog.read() {
                Ok(snapshots) => snapshots,
                Err(BackupError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                    crate::backup::GameSnapshots::new(&game.name)
                }
                Err(error) => {
                    log::warn!("Cannot inspect local upgrade catalog: {error}");
                    problems.push(UpgradePendingItem {
                        id: format!("catalog:{}", game.backup_dir_name()),
                        game_id: game.backup_dir_name().into_owned(),
                        game_name: game.name.clone(),
                        snapshot_id: String::new(),
                        source_path: catalog.path().to_string_lossy().into_owned(),
                        issue: UpgradeIssue {
                            kind: UpgradeIssueKind::CatalogUnreadable,
                            archive_entry: None,
                        },
                        save_units: Vec::new(),
                    });
                    continue;
                }
            };
            for snapshot in snapshots.backups {
                if (snapshot.archive_format == ArchiveFormat::SevenZ
                    && snapshot.archive_name.is_some())
                    || journal.items.iter().any(|item| {
                        !item.extra_backup
                            && item.game_id == game.backup_dir_name().as_ref()
                            && item.snapshot.date == snapshot.date
                    })
                {
                    continue;
                }
                let directory = self.root.join(game.backup_dir_name().as_ref());
                let source = snapshot_archive_path(&directory, &snapshot);
                self.add_item(&mut journal, game, snapshot, source, false)?;
            }
            let extra = self
                .root
                .join(game.backup_dir_name().as_ref())
                .join("extra_backup");
            if !extra.is_dir() {
                continue;
            }
            for entry in fs::read_dir(&extra)? {
                let path = entry?.path();
                let format = match path.extension().and_then(|value| value.to_str()) {
                    Some("zip") => ArchiveFormat::Zip,
                    Some("7z") => ArchiveFormat::SevenZ,
                    _ => continue,
                };
                let Some(id) = path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .filter(|value| value.starts_with("Overwrite_"))
                else {
                    continue;
                };
                if format == ArchiveFormat::SevenZ
                    && SevenZBackend.archive_version(&path).ok()
                        == Some(crate::backup::ArchiveVersion::V6)
                {
                    continue;
                }
                if journal.items.iter().any(|item| {
                    item.extra_backup
                        && item.game_id == game.backup_dir_name().as_ref()
                        && item.snapshot.date == id
                }) {
                    continue;
                }
                let snapshot: Snapshot = serde_json::from_value(
                    serde_json::json!({"date":id,"describe":"","path":path,"archive_format":format}),
                )?;
                self.add_item(&mut journal, game, snapshot, path, true)?;
            }
        }
        Ok((journal, problems))
    }

    fn add_item(
        &self,
        journal: &mut UpgradeJournal,
        game: &crate::backup::Game,
        snapshot: Snapshot,
        source: std::path::PathBuf,
        extra_backup: bool,
    ) -> Result<(), BackupError> {
        let directory = if extra_backup {
            self.root
                .join(game.backup_dir_name().as_ref())
                .join("extra_backup")
        } else {
            self.root.join(game.backup_dir_name().as_ref())
        };
        let original_size = fs::metadata(&source).map_or(snapshot.size, |metadata| metadata.len());
        let unpacked_size = inspect_archive_size(&source, snapshot.archive_format).ok();
        let name = new_archive_name(&snapshot, &directory)?;
        let output_name = Some(if extra_backup {
            format!("Overwrite_{name}")
        } else {
            name
        });
        let id = uuid::Uuid::new_v4().to_string();
        let retained = self
            .journal_dir()
            .join("originals")
            .join(crate::device::encode_device_id(
                game.backup_dir_name().as_ref(),
            ))
            .join(format!(
                "{id}_{}",
                source.file_name().unwrap_or_default().to_string_lossy()
            ));
        journal.items.push(UpgradeItem {
            id,
            game_id: game.backup_dir_name().into_owned(),
            game_name: game.name.clone(),
            snapshot,
            source_path: source.to_string_lossy().into_owned(),
            retained_path: retained.to_string_lossy().into_owned(),
            extra_backup,
            replacement_path: None,
            output_name,
            original_size,
            unpacked_size,
            original_hash: None,
            output_hash: None,
            output_size: 0,
            state: UpgradeItemState::Queued,
            issue: None,
            associations: BTreeMap::new(),
        });
        Ok(())
    }

    pub(super) fn view(
        &self,
        journal: &UpgradeJournal,
        mut pending: Vec<UpgradePendingItem>,
    ) -> LocalUpgradeView {
        let mut view = LocalUpgradeView {
            started: journal.started,
            total: journal.items.len(),
            remaining: 0,
            completed: 0,
            original_count: 0,
            original_bytes: 0,
            estimated_extra_bytes: 0,
            unknown_sizes: 0,
            pending: Vec::new(),
        };
        let mut scratch = 0;
        for item in &journal.items {
            match item.state {
                UpgradeItemState::Queued | UpgradeItemState::Prepared => {
                    view.remaining += 1;
                    let size = item.unpacked_size.unwrap_or(item.original_size);
                    view.estimated_extra_bytes = view
                        .estimated_extra_bytes
                        .saturating_add(size)
                        .saturating_add(1024 * 1024);
                    scratch = scratch.max(size);
                    if item.unpacked_size.is_none() {
                        view.unknown_sizes += 1;
                    }
                }
                UpgradeItemState::Completed | UpgradeItemState::Cleaned => {
                    view.completed += 1;
                }
                UpgradeItemState::Pending => {}
            }
            if item.state == UpgradeItemState::Completed && self.original_path(item).is_file() {
                view.original_count += 1;
                view.original_bytes = view.original_bytes.saturating_add(item.original_size);
            }
            if let Some(issue) = &item.issue {
                let save_units = self
                    .games
                    .iter()
                    .find(|game| game.backup_dir_name().as_ref() == item.game_id)
                    .map(|game| {
                        game.save_paths
                            .iter()
                            .map(|unit| UpgradeUnitChoice {
                                id: unit.id,
                                path: unit
                                    .manifest_pattern()
                                    .map(|(pattern, _)| pattern.raw().to_string())
                                    .or_else(|| unit.get_path_for_device(&self.device_id).cloned())
                                    .or_else(|| {
                                        item.snapshot
                                            .device_id
                                            .as_ref()
                                            .and_then(|id| unit.get_path_for_device(id).cloned())
                                    })
                                    .unwrap_or_default(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                pending.push(UpgradePendingItem {
                    id: item.id.clone(),
                    game_id: item.game_id.clone(),
                    game_name: item.game_name.clone(),
                    snapshot_id: item.snapshot.date.clone(),
                    source_path: self.original_path(item).to_string_lossy().into_owned(),
                    issue: issue.clone(),
                    save_units,
                });
            }
        }
        view.estimated_extra_bytes = view.estimated_extra_bytes.saturating_add(scratch);
        view.pending = pending;
        view
    }
}
