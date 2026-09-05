use super::{
    ArchiveIntegrity, ArchiveIntegrityError, CLOUD_MANIFEST_PATH, CloudManifest,
    CloudNamespaceClassification, CloudNamespaceClassifier, CloudNamespaceDescriptor,
    CloudNamespaceError, DELETION_REGISTRY_PATH, DeletionRegistry, GameManifest, ManifestError,
    SHARED_LIBRARY_PATH, SnapshotNode, SnapshotState, V2_NAMESPACE_DESCRIPTOR_PATH,
    cloud_archive_path, device_profile_path,
};
use crate::backup::{GameSnapshots, Snapshot, snapshot_archive_path};
use crate::cloud_sync::transfer::CloudTransfer;
use crate::cloud_sync::utils::{
    SyncOperationError, game_cloud_archive_path, load_remote_game_snapshots,
};
use crate::config::{
    ConfigurationOwners, DeviceProfile, OwnershipError, SharedLibrary, V2_CONFIG_SCHEMA_VERSION,
};
use crate::device::{DeviceId, encode_device_id};
use crate::preclude::BackendError;
use opendal::{ErrorKind, Operator};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use thiserror::Error;
const CUTOVER_PROGRESS_SCHEMA_VERSION: u32 = 1;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct CloudLibraryCutoverReview {
    pub game_count: usize,
    pub snapshot_count: usize,
    pub declared_bytes: u64,
}
#[derive(Debug)]
pub struct CloudLibraryCutoverResult {
    pub library_id: String,
    pub shared_library: SharedLibrary,
    pub device_profiles: HashMap<DeviceId, DeviceProfile>,
    pub snapshot_count: usize,
    pub unavailable_archives: usize,
}
#[derive(Debug, Serialize, Deserialize)]
struct CutoverPlan {
    schema_version: u32,
    library_id: String,
    shared_library: SharedLibrary,
    device_profiles: HashMap<DeviceId, DeviceProfile>,
    games: Vec<FrozenGame>,
}
#[derive(Debug, Serialize, Deserialize)]
struct FrozenGame {
    game_id: String,
    snapshots: GameSnapshots,
    results: BTreeMap<String, CutoverArchiveResult>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CutoverArchiveResult {
    Verified {
        integrity: ArchiveIntegrity,
        local_present: bool,
    },
    Unavailable,
}
pub struct CloudLibraryCutover {
    operator: Operator,
    local_archive_root: PathBuf,
    progress_path: PathBuf,
    current_device_id: DeviceId,
    current_device_profile: DeviceProfile,
    max_attempts: usize,
}
impl CloudLibraryCutover {
    pub fn new(
        operator: Operator,
        local_archive_root: PathBuf,
        progress_path: PathBuf,
        current_device_id: DeviceId,
        current_device_profile: DeviceProfile,
        max_attempts: usize,
    ) -> Self {
        Self {
            operator,
            local_archive_root,
            progress_path,
            current_device_id,
            current_device_profile,
            max_attempts: max_attempts.max(1),
        }
    }
    pub async fn review(&self) -> Result<CloudLibraryCutoverReview, CloudLibraryCutoverError> {
        let plan = match self.load_progress().await? {
            Some(plan) => plan,
            None => self.freeze_plan().await?,
        };
        Ok(Self::summarize(&plan))
    }
    pub async fn execute(&self) -> Result<CloudLibraryCutoverResult, CloudLibraryCutoverError> {
        let mut plan = match self.load_progress().await? {
            Some(plan) => plan,
            None => {
                let plan = self.freeze_plan().await?;
                self.store_progress(&plan).await?;
                plan
            }
        };
        if self.descriptor_exists().await? {
            if !Self::is_complete(&plan) {
                return Err(CloudLibraryCutoverError::UnexpectedPublishedNamespace);
            }
            let manifest = self.build_manifest(&plan)?;
            self.verify_published(&plan.library_id, &plan.shared_library, &manifest)
                .await?;
            return Ok(Self::result(plan));
        }
        for game_index in 0..plan.games.len() {
            let game_id = plan.games[game_index].game_id.clone();
            let snapshots = plan.games[game_index].snapshots.backups.clone();
            for snapshot in snapshots {
                if plan.games[game_index].results.contains_key(&snapshot.date) {
                    continue;
                }
                let result = self.migrate_archive(&game_id, &snapshot).await?;
                plan.games[game_index]
                    .results
                    .insert(snapshot.date.clone(), result);
                self.store_progress(&plan).await?;
                if let Some(limit) = e2e_cutover_interrupt_after_archives()? {
                    let completed = plan
                        .games
                        .iter()
                        .map(|game| game.results.len())
                        .sum::<usize>();
                    if completed >= limit {
                        return Err(CloudLibraryCutoverError::InjectedInterruption(completed));
                    }
                }
            }
        }
        let manifest = self.build_manifest(&plan)?;
        self.publish(&plan, &manifest).await?;
        self.verify_published(&plan.library_id, &plan.shared_library, &manifest)
            .await?;
        Ok(Self::result(plan))
    }
    pub async fn finish(&self) -> Result<(), CloudLibraryCutoverError> {
        remove_path_if_exists(&self.progress_path).await?;
        remove_dir_if_exists(&self.staging_root()).await?;
        Ok(())
    }
    async fn freeze_plan(&self) -> Result<CutoverPlan, CloudLibraryCutoverError> {
        let config = match CloudNamespaceClassifier::new(self.operator.clone())
            .classify()
            .await?
        {
            CloudNamespaceClassification::V1Only { config } => *config,
            _ => return Err(CloudLibraryCutoverError::CutoverUnavailable),
        };
        let mut owners = ConfigurationOwners::from_legacy(&config, &self.current_device_id);
        owners.device_profiles.insert(
            self.current_device_id.clone(),
            self.current_device_profile
                .for_shared_library(&owners.shared_library),
        );
        owners.validate()?;
        let mut games = Vec::with_capacity(config.games.len());
        for game in &config.games {
            let mut snapshots = load_remote_game_snapshots(&self.operator, &game.storage_key, None)
                .await?
                .unwrap_or_else(|| GameSnapshots::new(&game.storage_key));
            snapshots.normalize_heads_for_device(&self.current_device_id);
            let mut identities = std::collections::HashSet::new();
            for snapshot in &snapshots.backups {
                if !identities.insert(snapshot.date.clone()) {
                    return Err(CloudLibraryCutoverError::DuplicateSnapshot(
                        snapshot.date.clone(),
                    ));
                }
            }
            games.push(FrozenGame {
                game_id: game.storage_key.clone(),
                snapshots,
                results: BTreeMap::new(),
            });
        }
        Ok(CutoverPlan {
            schema_version: CUTOVER_PROGRESS_SCHEMA_VERSION,
            library_id: CloudNamespaceDescriptor::default().library_id,
            shared_library: owners.shared_library,
            device_profiles: owners.device_profiles,
            games,
        })
    }
    fn summarize(plan: &CutoverPlan) -> CloudLibraryCutoverReview {
        let snapshot_count = plan
            .games
            .iter()
            .map(|game| game.snapshots.backups.len())
            .sum();
        let declared_bytes = plan
            .games
            .iter()
            .flat_map(|game| &game.snapshots.backups)
            .fold(0_u64, |total, snapshot| total.saturating_add(snapshot.size));
        CloudLibraryCutoverReview {
            game_count: plan.shared_library.games.len(),
            snapshot_count,
            declared_bytes,
        }
    }
    fn result(plan: CutoverPlan) -> CloudLibraryCutoverResult {
        let summary = Self::summarize(&plan);
        let unavailable_archives = plan
            .games
            .iter()
            .flat_map(|game| game.results.values())
            .filter(|result| matches!(result, CutoverArchiveResult::Unavailable))
            .count();
        CloudLibraryCutoverResult {
            library_id: plan.library_id,
            shared_library: plan.shared_library,
            device_profiles: plan.device_profiles,
            snapshot_count: summary.snapshot_count,
            unavailable_archives,
        }
    }
    fn is_complete(plan: &CutoverPlan) -> bool {
        plan.games.iter().all(|game| {
            game.results.len() == game.snapshots.backups.len()
                && game
                    .snapshots
                    .backups
                    .iter()
                    .all(|snapshot| game.results.contains_key(&snapshot.date))
        })
    }
    async fn migrate_archive(
        &self,
        game_id: &str,
        snapshot: &Snapshot,
    ) -> Result<CutoverArchiveResult, CloudLibraryCutoverError> {
        let staging_root = self.staging_root();
        tokio::fs::create_dir_all(&staging_root).await?;
        let stem = format!(
            "{}-{}",
            encode_device_id(game_id),
            encode_device_id(&snapshot.date)
        );
        let source_staging = staging_root.join(format!("{stem}.source"));
        let verify_staging = staging_root.join(format!("{stem}.verify"));
        remove_path_if_exists(&source_staging).await?;
        remove_path_if_exists(&verify_staging).await?;
        let local_path = snapshot_archive_path(&self.local_archive_root.join(game_id), snapshot);
        let (source, integrity, local_present) =
            if let Some(integrity) = accepted_legacy_archive(&local_path, snapshot) {
                (local_path, integrity, true)
            } else {
                let legacy_path = game_cloud_archive_path(game_id, snapshot)?;
                match CloudTransfer::new(&self.operator)
                    .download_file_streaming(&legacy_path, &source_staging)
                    .await
                {
                    Ok(()) => match accepted_legacy_archive(&source_staging, snapshot) {
                        Some(integrity) => (source_staging.clone(), integrity, false),
                        None => {
                            remove_path_if_exists(&source_staging).await?;
                            return Ok(CutoverArchiveResult::Unavailable);
                        }
                    },
                    Err(error) if is_backend_not_found(&error) => {
                        return Ok(CutoverArchiveResult::Unavailable);
                    }
                    Err(error) => return Err(error.into()),
                }
            };
        let v2_path = cloud_archive_path(game_id, &snapshot.date, snapshot.archive_format)?;
        let transfer = CloudTransfer::new(&self.operator);
        let mut verified = false;
        for _ in 0..self.max_attempts {
            transfer.upload_file_streaming(&source, &v2_path).await?;
            transfer
                .download_file_streaming(&v2_path, &verify_staging)
                .await?;
            if integrity.verify_file(&verify_staging).is_ok() {
                verified = true;
                break;
            }
        }
        remove_path_if_exists(&source_staging).await?;
        remove_path_if_exists(&verify_staging).await?;
        if !verified {
            return Err(CloudLibraryCutoverError::ArchiveVerificationFailed {
                game_id: game_id.to_string(),
                snapshot_id: snapshot.date.clone(),
                attempts: self.max_attempts,
            });
        }
        Ok(CutoverArchiveResult::Verified {
            integrity,
            local_present,
        })
    }
    fn build_manifest(
        &self,
        plan: &CutoverPlan,
    ) -> Result<CloudManifest, CloudLibraryCutoverError> {
        let mut manifest = CloudManifest::default();
        for frozen in &plan.games {
            let mut game = GameManifest::new(&frozen.game_id);
            for snapshot in &frozen.snapshots.backups {
                let result = frozen.results.get(&snapshot.date).ok_or_else(|| {
                    CloudLibraryCutoverError::MissingProgressResult(snapshot.date.clone())
                })?;
                let integrity = match result {
                    CutoverArchiveResult::Verified { integrity, .. } => Some(integrity.clone()),
                    CutoverArchiveResult::Unavailable => None,
                };
                let mut node = SnapshotNode::from_snapshot(snapshot, integrity);
                if let SnapshotState::Live(live) = &mut node.state {
                    live.cloud_archive_verified =
                        matches!(result, CutoverArchiveResult::Verified { .. });
                }
                game.upsert_live(node)?;
                if matches!(
                    result,
                    CutoverArchiveResult::Verified {
                        local_present: true,
                        ..
                    }
                ) {
                    game.report_local_archive(
                        self.current_device_id.clone(),
                        snapshot.date.clone(),
                        true,
                    );
                }
            }
            for (device_id, head) in frozen.snapshots.head_entries() {
                game.set_head(device_id.clone(), head.clone());
            }
            manifest.games.insert(frozen.game_id.clone(), game);
        }
        manifest.validate()?;
        Ok(manifest)
    }
    async fn publish(
        &self,
        plan: &CutoverPlan,
        manifest: &CloudManifest,
    ) -> Result<(), CloudLibraryCutoverError> {
        self.write_verified(
            SHARED_LIBRARY_PATH,
            &serde_json::to_vec_pretty(&plan.shared_library)?,
        )
        .await?;
        self.write_verified(CLOUD_MANIFEST_PATH, &serde_json::to_vec_pretty(manifest)?)
            .await?;
        self.write_verified(
            DELETION_REGISTRY_PATH,
            &serde_json::to_vec_pretty(&DeletionRegistry::default())?,
        )
        .await?;
        let mut profiles = plan.device_profiles.iter().collect::<Vec<_>>();
        profiles.sort_by_key(|(device_id, _)| *device_id);
        for (device_id, profile) in profiles {
            self.write_verified(
                &device_profile_path(device_id),
                &serde_json::to_vec_pretty(profile)?,
            )
            .await?;
        }
        self.write_verified(
            V2_NAMESPACE_DESCRIPTOR_PATH,
            &serde_json::to_vec_pretty(&CloudNamespaceDescriptor::with_library_id(
                &plan.library_id,
            ))?,
        )
        .await
    }
    async fn write_verified(
        &self,
        path: &str,
        bytes: &[u8],
    ) -> Result<(), CloudLibraryCutoverError> {
        for _ in 0..self.max_attempts {
            self.operator.write(path, bytes.to_vec()).await?;
            match self.operator.read(path).await {
                Ok(stored) if stored.to_vec().as_slice() == bytes => return Ok(()),
                Ok(_) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(CloudLibraryCutoverError::WriteVerificationFailed {
            path: path.to_string(),
            attempts: self.max_attempts,
        })
    }
    async fn verify_published(
        &self,
        library_id: &str,
        library: &SharedLibrary,
        manifest: &CloudManifest,
    ) -> Result<(), CloudLibraryCutoverError> {
        match CloudNamespaceClassifier::new(self.operator.clone())
            .classify()
            .await?
        {
            CloudNamespaceClassification::SupportedV2 {
                descriptor,
                shared_library,
                manifest: stored_manifest,
            } if descriptor.library_id == library_id
                && shared_library == *library
                && stored_manifest == *manifest =>
            {
                Ok(())
            }
            _ => Err(CloudLibraryCutoverError::FinalVerificationMismatch),
        }
    }
    async fn descriptor_exists(&self) -> Result<bool, opendal::Error> {
        match self.operator.read(V2_NAMESPACE_DESCRIPTOR_PATH).await {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }
    async fn load_progress(&self) -> Result<Option<CutoverPlan>, CloudLibraryCutoverError> {
        let bytes = match tokio::fs::read(&self.progress_path).await {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let mut plan: CutoverPlan = serde_json::from_slice(&bytes)?;
        if plan.schema_version != CUTOVER_PROGRESS_SCHEMA_VERSION {
            return Err(CloudLibraryCutoverError::UnsupportedProgressSchema(
                plan.schema_version,
            ));
        }
        plan.shared_library.validate()?;
        for (device_id, profile) in &plan.device_profiles {
            if profile.schema_version != V2_CONFIG_SCHEMA_VERSION {
                return Err(OwnershipError::UnsupportedSchema {
                    owner: format!("Device Profile {device_id}"),
                    found: profile.schema_version,
                }
                .into());
            }
            if device_id != &profile.device.id {
                return Err(OwnershipError::ProfileIdentityMismatch {
                    key: device_id.clone(),
                    embedded: profile.device.id.clone(),
                }
                .into());
            }
        }
        plan.device_profiles.insert(
            self.current_device_id.clone(),
            self.current_device_profile
                .for_shared_library(&plan.shared_library),
        );
        Ok(Some(plan))
    }
    async fn store_progress(&self, plan: &CutoverPlan) -> Result<(), CloudLibraryCutoverError> {
        if let Some(parent) = self.progress_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        CloudTransfer::new(&self.operator)
            .write_local_bytes_atomically(&self.progress_path, &serde_json::to_vec_pretty(plan)?)
            .await?;
        Ok(())
    }
    fn staging_root(&self) -> PathBuf {
        self.progress_path.with_extension("staging")
    }
}
fn accepted_legacy_archive(path: &Path, snapshot: &Snapshot) -> Option<ArchiveIntegrity> {
    let integrity = ArchiveIntegrity::from_file(path).ok()?;
    if snapshot.size > 0 && snapshot.size != integrity.size {
        return None;
    }
    if snapshot
        .archive_hash
        .as_ref()
        .is_some_and(|expected| !expected.eq_ignore_ascii_case(&integrity.xxh3_64))
    {
        return None;
    }
    Some(integrity)
}
fn is_backend_not_found(error: &BackendError) -> bool {
    matches!(error, BackendError::Cloud(source) if source.kind() == ErrorKind::NotFound)
}

/// Parse the debug-only Cutover interrupt failpoint.
///
/// Unset keeps normal Cutover. A positive integer interrupts after that many
/// persisted archive results. Any other value is a hard error so tests cannot
/// silently skip the injected interruption.
pub fn validate_e2e_cutover_interrupt_env() -> Result<Option<usize>, CloudLibraryCutoverError> {
    e2e_cutover_interrupt_after_archives()
}

fn e2e_cutover_interrupt_after_archives() -> Result<Option<usize>, CloudLibraryCutoverError> {
    #[cfg(debug_assertions)]
    {
        match std::env::var("RGSM_E2E_CUTOVER_INTERRUPT_AFTER_ARCHIVES") {
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(_) => Err(CloudLibraryCutoverError::InvalidCutoverInterruptFailpoint),
            Ok(raw) => {
                let parsed = raw.parse::<i64>().ok().filter(|&value| value > 0);
                parsed
                    .map(|value| Some(value as usize))
                    .ok_or(CloudLibraryCutoverError::InvalidCutoverInterruptFailpoint)
            }
        }
    }
    #[cfg(not(debug_assertions))]
    {
        Ok(None)
    }
}
async fn remove_path_if_exists(path: &Path) -> Result<(), std::io::Error> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
async fn remove_dir_if_exists(path: &Path) -> Result<(), std::io::Error> {
    match tokio::fs::remove_dir_all(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
#[derive(Debug, Error)]
pub enum CloudLibraryCutoverError {
    #[error("Cloud Cutover is available only for a legacy V1 Cloud Library")]
    CutoverUnavailable,
    #[error("Cloud Cutover progress uses unsupported schema {0}")]
    UnsupportedProgressSchema(u32),
    #[error("Legacy Catalog contains duplicate Snapshot identity: {0}")]
    DuplicateSnapshot(String),
    #[error("Cloud Cutover progress is missing Snapshot result: {0}")]
    MissingProgressResult(String),
    #[error("A namespace descriptor exists before the frozen Cutover is complete")]
    UnexpectedPublishedNamespace,
    #[error("V2 Archive verification failed for {game_id}/{snapshot_id} after {attempts} attempts")]
    ArchiveVerificationFailed {
        game_id: String,
        snapshot_id: String,
        attempts: usize,
    },
    #[error("Cloud object {path} did not match after {attempts} write attempts")]
    WriteVerificationFailed { path: String, attempts: usize },
    #[error("Published V2 Cloud Library does not match the frozen Cutover")]
    FinalVerificationMismatch,
    #[error("injected Cloud Cutover interruption after {0} archive(s)")]
    InjectedInterruption(usize),
    #[error("RGSM_E2E_CUTOVER_INTERRUPT_AFTER_ARCHIVES must be a positive integer")]
    InvalidCutoverInterruptFailpoint,
    #[error(transparent)]
    Namespace(#[from] CloudNamespaceError),
    #[error(transparent)]
    Sync(#[from] SyncOperationError),
    #[error(transparent)]
    Backend(#[from] BackendError),
    #[error(transparent)]
    Integrity(#[from] ArchiveIntegrityError),
    #[error(transparent)]
    Ownership(#[from] OwnershipError),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("Cloud Cutover serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Cloud Cutover I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Cloud Cutover transport error: {0}")]
    Transport(#[from] opendal::Error),
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::{ArchiveFormat, CreatedBy, Game};
    use crate::cloud_sync::V1_CONFIG_PATH;
    use crate::cloud_sync::utils::game_cloud_metadata_path;
    use crate::config::Config;
    use opendal::services;
    use std::collections::HashMap;
    fn operator() -> Operator {
        Operator::new(services::Memory::default()).unwrap().finish()
    }
    fn game() -> Game {
        Game {
            name: "Test Game".into(),
            storage_key: "test-game".into(),
            save_paths: Vec::new(),
            game_paths: HashMap::new(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            ludusavi_meta: None,
            device_bindings: HashMap::new(),
        }
    }
    fn snapshot(id: &str, parent: Option<&str>, payload: &[u8]) -> Snapshot {
        Snapshot {
            date: id.into(),
            describe: format!("Snapshot {id}"),
            path: format!("ignored/{id}.zip"),
            archive_format: ArchiveFormat::Zip,
            size: payload.len() as u64,
            parent: parent.map(str::to_string),
            archive_hash: Some(format!("{:016x}", xxhash_rust::xxh3::xxh3_64(payload))),
            created_at: None,
            device_id: Some("creator-device".into()),
            created_by: CreatedBy::Manual,
        }
    }
    async fn seed(
        op: &Operator,
        snapshots: GameSnapshots,
        archives: &[(&Snapshot, &[u8])],
    ) -> Vec<u8> {
        let config = Config {
            games: vec![game()],
            ..Config::default()
        };
        let config_bytes = serde_json::to_vec_pretty(&config).unwrap();
        op.write(V1_CONFIG_PATH, config_bytes.clone())
            .await
            .unwrap();
        op.write(
            &game_cloud_metadata_path("test-game").unwrap(),
            serde_json::to_vec_pretty(&snapshots).unwrap(),
        )
        .await
        .unwrap();
        for (snapshot, bytes) in archives {
            op.write(
                &game_cloud_archive_path("test-game", snapshot).unwrap(),
                bytes.to_vec(),
            )
            .await
            .unwrap();
        }
        config_bytes
    }
    fn cutover(op: Operator, root: &Path) -> CloudLibraryCutover {
        let device_id = "current-device".to_string();
        let mut current_profile = ConfigurationOwners::from_legacy(&Config::default(), &device_id)
            .device_profiles
            .remove(&device_id)
            .unwrap();
        current_profile.local_archive_root = Some("current-device-private-root".into());
        CloudLibraryCutover::new(
            op,
            root.join("local"),
            root.join("progress.json"),
            device_id,
            current_profile,
            2,
        )
    }
    #[tokio::test]
    async fn migrates_local_and_marks_missing_without_touching_v1() {
        let op = operator();
        let root = temp_dir::TempDir::new().unwrap();
        let local_bytes = b"local archive";
        let mut local = snapshot("local", None, local_bytes);
        let local_path = root.path().join("relocated/local.zip");
        local.path = local_path.to_string_lossy().into_owned();
        let missing = snapshot("missing", Some("local"), b"missing bytes");
        let mut catalog = GameSnapshots::new("test-game");
        catalog.backups = vec![local.clone(), missing];
        catalog.device_heads.insert("deck".into(), "missing".into());
        let v1_config = seed(&op, catalog, &[]).await;
        std::fs::create_dir_all(local_path.parent().unwrap()).unwrap();
        std::fs::write(&local_path, local_bytes).unwrap();
        let runner = cutover(op.clone(), root.path());
        let result = runner.execute().await.unwrap();
        assert_eq!(result.snapshot_count, 2);
        assert_eq!(result.unavailable_archives, 1);
        assert_eq!(
            result.device_profiles["current-device"]
                .local_archive_root
                .as_deref(),
            Some("current-device-private-root")
        );
        assert_eq!(op.read(V1_CONFIG_PATH).await.unwrap().to_vec(), v1_config);
        let manifest: CloudManifest =
            serde_json::from_slice(&op.read(CLOUD_MANIFEST_PATH).await.unwrap().to_vec()).unwrap();
        let game = &manifest.games["test-game"];
        assert!(matches!(
            &game.snapshots["local"].state,
            SnapshotState::Live(live)
                if live.cloud_archive_verified && live.integrity.is_some()
        ));
        assert!(matches!(
            &game.snapshots["missing"].state,
            SnapshotState::Live(live)
                if !live.cloud_archive_verified && live.integrity.is_none()
        ));
        let json = serde_json::to_string(&manifest).unwrap();
        assert!(json.contains("creator-device"));
        assert_eq!(game.device_heads["deck"], "missing");
    }
    #[tokio::test]
    async fn cloud_only_archive_uses_bounded_staging_and_cleans_it() {
        let op = operator();
        let root = temp_dir::TempDir::new().unwrap();
        let bytes = b"cloud archive";
        let cloud = snapshot("cloud", None, bytes);
        let mut catalog = GameSnapshots::new("test-game");
        catalog.backups.push(cloud.clone());
        seed(&op, catalog, &[(&cloud, bytes)]).await;
        let runner = cutover(op.clone(), root.path());
        let result = runner.execute().await.unwrap();
        assert_eq!(result.unavailable_archives, 0);
        assert_eq!(std::fs::read_dir(runner.staging_root()).unwrap().count(), 0);
        let v2_path = cloud_archive_path("test-game", "cloud", ArchiveFormat::Zip).unwrap();
        assert_eq!(op.read(&v2_path).await.unwrap().to_vec(), bytes);
    }
    #[tokio::test]
    async fn resume_uses_frozen_terminal_results_after_v1_changes() {
        let op = operator();
        let root = temp_dir::TempDir::new().unwrap();
        let bytes = b"already migrated";
        let snapshot = snapshot("done", None, bytes);
        let mut catalog = GameSnapshots::new("test-game");
        catalog.backups.push(snapshot.clone());
        seed(&op, catalog, &[(&snapshot, bytes)]).await;
        let runner = cutover(op.clone(), root.path());
        let mut plan = runner.freeze_plan().await.unwrap();
        let integrity = ArchiveIntegrity {
            size: bytes.len() as u64,
            xxh3_64: snapshot.archive_hash.clone().unwrap(),
        };
        plan.games[0].results.insert(
            snapshot.date.clone(),
            CutoverArchiveResult::Verified {
                integrity,
                local_present: false,
            },
        );
        runner.store_progress(&plan).await.unwrap();
        let v2_path = cloud_archive_path("test-game", "done", ArchiveFormat::Zip).unwrap();
        op.write(&v2_path, bytes.to_vec()).await.unwrap();
        op.delete(V1_CONFIG_PATH).await.unwrap();
        op.delete(&game_cloud_archive_path("test-game", &snapshot).unwrap())
            .await
            .unwrap();
        let result = runner.execute().await.unwrap();
        assert_eq!(result.snapshot_count, 1);
        assert_eq!(op.read(&v2_path).await.unwrap().to_vec(), bytes);
        assert!(op.read(V2_NAMESPACE_DESCRIPTOR_PATH).await.is_ok());
    }

    fn with_interrupt_env<T>(value: Option<&str>, body: impl FnOnce() -> T) -> T {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().expect("interrupt env lock");
        unsafe {
            match value {
                Some(value) => {
                    std::env::set_var("RGSM_E2E_CUTOVER_INTERRUPT_AFTER_ARCHIVES", value)
                }
                None => std::env::remove_var("RGSM_E2E_CUTOVER_INTERRUPT_AFTER_ARCHIVES"),
            }
        }
        let result = body();
        unsafe {
            std::env::remove_var("RGSM_E2E_CUTOVER_INTERRUPT_AFTER_ARCHIVES");
        }
        result
    }

    #[test]
    fn unset_interrupt_failpoint_is_ignored() {
        with_interrupt_env(None, || {
            assert_eq!(e2e_cutover_interrupt_after_archives().unwrap(), None);
        });
    }

    #[test]
    fn invalid_interrupt_failpoint_is_a_hard_error() {
        for value in ["0", "-1", "nope", ""] {
            with_interrupt_env(Some(value), || {
                assert!(matches!(
                    e2e_cutover_interrupt_after_archives(),
                    Err(CloudLibraryCutoverError::InvalidCutoverInterruptFailpoint)
                ));
            });
        }
    }
}
