use super::{
    ArchiveFormat, CapturePlan, CreatedBy, Game, GameSnapshots, RestoreNotifier, Snapshot,
    archive::{ArchiveIdentity, write_snapshot_archive},
    archive_file_name,
    extra_backups::cleanup_oldest_extra_backups,
    game::{SnapshotCreated, unused_snapshot_id},
};
use crate::{device::DeviceId, preclude::BackupError};
use std::{fs, path::Path};

pub struct CaptureSnapshotOptions<'a> {
    pub backup_base: &'a Path,
    pub device_id: &'a DeviceId,
    pub preset: crate::backup::CompressionPreset,
    pub describe: &'a str,
    pub parent_date: Option<String>,
    pub created_by: CreatedBy,
    pub source_fingerprint: Option<String>,
    /// Stage notifications during capture (compression is the long pole).
    pub notifier: Option<&'a dyn RestoreNotifier>,
}

impl Game {
    /// Persist a snapshot from a fully preflighted immutable capture plan.
    /// Configuration and host discovery stay outside this domain operation.
    pub async fn create_snapshot_from_capture_plan(
        &self,
        plan: &CapturePlan,
        options: CaptureSnapshotOptions<'_>,
    ) -> Result<SnapshotCreated, BackupError> {
        let backup_path = options.backup_base.join(self.backup_dir_name().as_ref());
        fs::create_dir_all(&backup_path)?;
        let infos = match self.get_game_snapshots_info() {
            Ok(infos) => infos,
            Err(BackupError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                GameSnapshots::new(self.name.clone())
            }
            Err(error) => return Err(error),
        };
        let date = unused_snapshot_id(&backup_path, &infos)?;
        let created_at = Some(chrono::Utc::now().timestamp_millis());
        let archive_format = ArchiveFormat::SevenZ;
        let archive_path = backup_path.join(archive_file_name(&date, archive_format));
        if let Some(notifier) = options.notifier {
            notifier.notify(
                crate::backup::RestoreNotificationLevel::Info,
                rust_i18n::t!("backend.stage.title").as_ref(),
                rust_i18n::t!("backend.stage.compress", count = plan.groups.len()).as_ref(),
            );
        }
        let parent = options
            .parent_date
            .or_else(|| infos.current_device_head().cloned());
        let mut snapshot = Snapshot {
            date: date.clone(),
            describe: options.describe.to_string(),
            path: archive_path.to_string_lossy().into_owned(),
            archive_format,
            size: 0,
            parent,
            archive_hash: None,
            created_at,
            device_id: Some(options.device_id.clone()),
            created_by: options.created_by,
        };
        snapshot.size = write_snapshot_archive(
            plan,
            &archive_path,
            options.preset,
            ArchiveIdentity::for_snapshot(self, &snapshot, options.device_id),
            options.source_fingerprint,
        )?;
        let infos = self.update_game_snapshots_info::<BackupError>(|current| {
            current.backups.push(snapshot);
            current.set_current_device_head(Some(date.clone()));
            Ok(())
        })?;

        Ok(SnapshotCreated {
            snapshots: infos,
            remote_archive_path: format!(
                "save_data/{}/{}",
                self.backup_dir_name(),
                archive_file_name(&date, archive_format)
            ),
            local_archive_path: archive_path,
        })
    }

    pub fn create_overwrite_snapshot_from_capture_plan(
        &self,
        plan: &CapturePlan,
        backup_base: &Path,
        preset: crate::backup::CompressionPreset,
        max_extra_backup_count: u32,
        device_id: &DeviceId,
    ) -> Result<String, BackupError> {
        let extra_backup_path = backup_base
            .join(self.backup_dir_name().as_ref())
            .join("extra_backup");
        fs::create_dir_all(&extra_backup_path)?;
        let date = chrono::Local::now()
            .format("Overwrite_%Y-%m-%d_%H-%M-%S")
            .to_string();
        let archive_path = extra_backup_path.join(archive_file_name(&date, ArchiveFormat::SevenZ));
        let snapshot = Snapshot {
            date: date.clone(),
            describe: String::new(),
            path: String::new(),
            archive_format: ArchiveFormat::SevenZ,
            size: 0,
            parent: None,
            archive_hash: None,
            created_at: Some(chrono::Utc::now().timestamp_millis()),
            device_id: Some(device_id.clone()),
            created_by: CreatedBy::Manual,
        };
        write_snapshot_archive(
            plan,
            &archive_path,
            preset,
            ArchiveIdentity::for_snapshot(self, &snapshot, device_id),
            None,
        )?;
        cleanup_oldest_extra_backups(&extra_backup_path, max_extra_backup_count)?;
        Ok(date)
    }
}
