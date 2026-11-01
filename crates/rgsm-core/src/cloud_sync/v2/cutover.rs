use super::{
    ArchiveIntegrity, ArchiveIntegrityError, CLOUD_MANIFEST_PATH, CloudManifest,
    CloudNamespaceClassification, CloudNamespaceClassifier, CloudNamespaceDescriptor,
    CloudNamespaceError, DELETION_REGISTRY_PATH, DeletionRegistry, GameManifest, ManifestError,
    SHARED_LIBRARY_PATH, SnapshotNode, SnapshotState, V2_NAMESPACE_DESCRIPTOR_PATH,
    cloud_archive_path, device_profile_path,
};
use crate::backup::{GameSnapshots, Snapshot, snapshot_archive_path};
use crate::cloud_sync::V1_SAVE_DATA_PREFIX;
use crate::cloud_sync::transfer::CloudTransfer;
use crate::cloud_sync::utils::{
    SyncOperationError, game_cloud_archive_path, load_remote_game_snapshots,
};
use crate::config::{
    ConfigurationOwners, DeviceProfile, OwnershipError, SharedLibrary,
    SharedSnapshotRetentionPolicy, V2_CONFIG_SCHEMA_VERSION,
};
use crate::device::{DeviceId, encode_device_id};
use crate::preclude::BackendError;
use opendal::{ErrorKind, Operator};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;
const CUTOVER_PROGRESS_SCHEMA_VERSION: u32 = 1;
const SOURCE_RETRY_DELAY: Duration = Duration::from_secs(2);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct CloudLibraryCutoverReview {
    pub game_count: usize,
    pub snapshot_count: usize,
    pub declared_bytes: u64,
    /// Games whose automatic-snapshot limit this device had before the upgrade.
    /// Carrying a limit into the Cloud Library makes cleanup delete old
    /// automatic Snapshots on every Device and in the cloud, so it is offered
    /// rather than applied silently.
    pub automatic_cleanup: Vec<CutoverAutomaticCleanup>,
    /// Games with history in the old cloud folder that the old cloud
    /// configuration does not list. Their Snapshots are not carried over.
    pub unlisted_cloud_games: Vec<CutoverUnlistedGame>,
    /// Games on this Device that the old cloud configuration does not list.
    /// They stay local, with cloud sync off.
    pub local_only_games: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct CutoverUnlistedGame {
    pub game_id: String,
    pub snapshot_count: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct CutoverAutomaticCleanup {
    pub game_id: String,
    pub game_name: String,
    pub automatic_snapshots_per_branch: u32,
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
    /// Limits still waiting for the player's decision. Empty once `execute`
    /// has stored the choice in `shared_library`.
    #[serde(default)]
    automatic_cleanup_offer: BTreeMap<String, u32>,
    #[serde(default)]
    unlisted_cloud_games: Vec<CutoverUnlistedGame>,
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
    source_retry_delay: Duration,
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
            source_retry_delay: SOURCE_RETRY_DELAY,
        }
    }
    pub async fn review(
        &self,
        local_library: &SharedLibrary,
    ) -> Result<CloudLibraryCutoverReview, CloudLibraryCutoverError> {
        let plan = match self.load_progress().await? {
            Some(plan) => plan,
            None => self.freeze_plan().await?,
        };
        Ok(Self::summarize(&plan, local_library))
    }
    /// `apply_automatic_cleanup` is only consulted when the plan is first
    /// frozen; a resumed Cutover keeps the choice it started with.
    pub async fn execute(
        &self,
        apply_automatic_cleanup: bool,
    ) -> Result<CloudLibraryCutoverResult, CloudLibraryCutoverError> {
        let mut plan = match self.load_progress().await? {
            Some(plan) => plan,
            None => {
                let mut plan = self.freeze_plan().await?;
                let offer = std::mem::take(&mut plan.automatic_cleanup_offer);
                if apply_automatic_cleanup {
                    for game in &mut plan.shared_library.games {
                        game.snapshot_retention = offer.get(&game.storage_key).map(|&limit| {
                            SharedSnapshotRetentionPolicy {
                                automatic_snapshots_per_branch: limit,
                            }
                        });
                    }
                }
                self.store_progress(&plan).await?;
                plan
            }
        };
        if self.verify_if_published(&plan).await? {
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
        self.publish_unless_claimed(&plan).await?;
        Ok(Self::result(plan))
    }
    /// Another device may have published while this one was uploading archives;
    /// publishing over it would mix two libraries, so look again right before
    /// writing the shared files.
    async fn publish_unless_claimed(
        &self,
        plan: &CutoverPlan,
    ) -> Result<(), CloudLibraryCutoverError> {
        if self.verify_if_published(plan).await? {
            return Ok(());
        }
        let manifest = self.build_manifest(plan)?;
        self.publish(plan, &manifest).await?;
        self.verify_published(&plan.library_id, &plan.shared_library, &manifest)
            .await
    }
    /// Ok(true) when this plan is already published; an unrelated published
    /// namespace is an error.
    async fn verify_if_published(
        &self,
        plan: &CutoverPlan,
    ) -> Result<bool, CloudLibraryCutoverError> {
        if !self.descriptor_exists().await? {
            return Ok(false);
        }
        if !Self::is_complete(plan) {
            return Err(CloudLibraryCutoverError::UnexpectedPublishedNamespace);
        }
        let manifest = self.build_manifest(plan)?;
        self.verify_published(&plan.library_id, &plan.shared_library, &manifest)
            .await?;
        Ok(true)
    }
    pub async fn finish(&self) -> Result<(), CloudLibraryCutoverError> {
        remove_path_if_exists(&self.progress_path).await?;
        remove_dir_if_exists(&self.staging_root()).await?;
        Ok(())
    }
    async fn freeze_plan(&self) -> Result<CutoverPlan, CloudLibraryCutoverError> {
        let classifier = CloudNamespaceClassifier::new(self.operator.clone());
        let config = match classifier.classify().await {
            Ok(CloudNamespaceClassification::V1Only { config }) => *config,
            // Without a descriptor nothing in `v2/` is published. After local
            // progress is lost the legacy configuration is still the source of
            // truth, and re-uploading an archive overwrites identical bytes.
            Err(CloudNamespaceError::PartialV2(_)) => classifier
                .read_legacy_config()
                .await?
                .ok_or(CloudLibraryCutoverError::CutoverUnavailable)?,
            Ok(_) => return Err(CloudLibraryCutoverError::CutoverUnavailable),
            Err(error) => return Err(error.into()),
        };
        let mut owners = ConfigurationOwners::from_legacy(&config, &self.current_device_id);
        owners.device_profiles.insert(
            self.current_device_id.clone(),
            self.current_device_profile
                .for_managed_games_in(&owners.shared_library),
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
        let automatic_cleanup_offer = self.automatic_cleanup_offer(&owners.shared_library);
        let unlisted_cloud_games = self.unlisted_cloud_games(&owners.shared_library).await?;
        Ok(CutoverPlan {
            schema_version: CUTOVER_PROGRESS_SCHEMA_VERSION,
            library_id: CloudNamespaceDescriptor::default().library_id,
            shared_library: owners.shared_library,
            device_profiles: owners.device_profiles,
            games,
            automatic_cleanup_offer,
            unlisted_cloud_games,
        })
    }
    /// The old cloud configuration is the last saver's game list, so history
    /// folders of games it lacks would otherwise vanish without a word.
    async fn unlisted_cloud_games(
        &self,
        library: &SharedLibrary,
    ) -> Result<Vec<CutoverUnlistedGame>, CloudLibraryCutoverError> {
        let entries = match self.operator.list(V1_SAVE_DATA_PREFIX).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut unlisted = Vec::new();
        for entry in entries {
            let game_id = entry.name().trim_end_matches('/');
            if !entry.metadata().is_dir()
                || game_id.is_empty()
                || library.games.iter().any(|game| game.storage_key == game_id)
            {
                continue;
            }
            match load_remote_game_snapshots(&self.operator, game_id, None).await {
                Ok(Some(snapshots)) if !snapshots.backups.is_empty() => {
                    unlisted.push(CutoverUnlistedGame {
                        game_id: game_id.to_string(),
                        snapshot_count: snapshots.backups.len(),
                    });
                }
                Ok(_) => {}
                Err(SyncOperationError::Backend(BackendError::Cloud(error))) => {
                    return Err((*error).into());
                }
                // A damaged folder nobody listed must not stop the upgrade.
                Err(error) => log::warn!(
                    target: "rgsm::cloud::cutover",
                    "Ignoring unreadable legacy folder {game_id}: {error}"
                ),
            }
        }
        unlisted.sort_by(|a, b| a.game_id.cmp(&b.game_id));
        Ok(unlisted)
    }
    /// The limit the initiating Device applied to its own automatic backups:
    /// the Game's own limit wins over the global one, and 0 means unlimited.
    fn automatic_cleanup_offer(&self, library: &SharedLibrary) -> BTreeMap<String, u32> {
        let global = self.current_device_profile.behavior.max_auto_backup_count;
        library
            .games
            .iter()
            .filter_map(|game| {
                let limit = self
                    .current_device_profile
                    .games
                    .get(&game.storage_key)
                    .and_then(|profile| profile.auto_backup_limit)
                    .unwrap_or(global);
                (limit > 0).then(|| (game.storage_key.clone(), limit))
            })
            .collect()
    }
    fn summarize(plan: &CutoverPlan, local_library: &SharedLibrary) -> CloudLibraryCutoverReview {
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
            automatic_cleanup: plan
                .shared_library
                .games
                .iter()
                .filter_map(|game| {
                    plan.automatic_cleanup_offer
                        .get(&game.storage_key)
                        .map(|&limit| CutoverAutomaticCleanup {
                            game_id: game.storage_key.clone(),
                            game_name: game.name.clone(),
                            automatic_snapshots_per_branch: limit,
                        })
                })
                .collect(),
            unlisted_cloud_games: plan.unlisted_cloud_games.clone(),
            local_only_games: local_library
                .games
                .iter()
                .filter(|local| {
                    !plan
                        .shared_library
                        .games
                        .iter()
                        .any(|game| game.storage_key == local.storage_key)
                })
                .map(|game| game.name.clone())
                .collect(),
        }
    }
    fn result(plan: CutoverPlan) -> CloudLibraryCutoverResult {
        let snapshot_count = plan
            .games
            .iter()
            .map(|game| game.snapshots.backups.len())
            .sum();
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
            snapshot_count,
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
                match self
                    .fetch_legacy_archive(game_id, snapshot, &source_staging)
                    .await?
                {
                    Some(integrity) => (source_staging.clone(), integrity, false),
                    None => return Ok(CutoverArchiveResult::Unavailable),
                }
            };
        let v2_path = cloud_archive_path(
            game_id,
            &snapshot.date,
            snapshot.archive_format,
            snapshot.archive_name.as_deref(),
        )?;
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
    /// A 1.8 device uploads the metadata before the archive and WebDAV can cut a
    /// response short, so a missing or mismatching source gets a few spaced
    /// attempts before it is recorded as unavailable for good.
    async fn fetch_legacy_archive(
        &self,
        game_id: &str,
        snapshot: &Snapshot,
        staging: &Path,
    ) -> Result<Option<ArchiveIntegrity>, CloudLibraryCutoverError> {
        let legacy_path = game_cloud_archive_path(game_id, snapshot)?;
        for attempt in 0..self.max_attempts {
            if attempt > 0 {
                tokio::time::sleep(self.source_retry_delay).await;
            }
            match CloudTransfer::new(&self.operator)
                .download_file_streaming(&legacy_path, staging)
                .await
            {
                Ok(()) => {
                    if let Some(integrity) = accepted_legacy_archive(staging, snapshot) {
                        return Ok(Some(integrity));
                    }
                    remove_path_if_exists(staging).await?;
                }
                Err(error) if is_backend_not_found(&error) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(None)
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
                .for_managed_games_in(&plan.shared_library),
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
            Err(std::env::VarError::NotPresent) => parse_interrupt_after_archives(None),
            Err(_) => Err(CloudLibraryCutoverError::InvalidCutoverInterruptFailpoint),
            Ok(raw) => parse_interrupt_after_archives(Some(&raw)),
        }
    }
    #[cfg(not(debug_assertions))]
    {
        Ok(None)
    }
}

#[cfg(any(debug_assertions, test))]
fn parse_interrupt_after_archives(
    raw: Option<&str>,
) -> Result<Option<usize>, CloudLibraryCutoverError> {
    raw.map(|value| {
        value
            .parse::<usize>()
            .ok()
            .filter(|&value| value > 0)
            .ok_or(CloudLibraryCutoverError::InvalidCutoverInterruptFailpoint)
    })
    .transpose()
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
    fn no_local_games() -> SharedLibrary {
        SharedLibrary {
            schema_version: V2_CONFIG_SCHEMA_VERSION,
            games: Vec::new(),
        }
    }
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
            auto_backup_limit: None,
            ludusavi_meta: None,
            device_bindings: HashMap::new(),
        }
    }
    fn snapshot(id: &str, parent: Option<&str>, payload: &[u8]) -> Snapshot {
        Snapshot {
            date: id.into(),
            describe: format!("Snapshot {id}"),
            path: format!("ignored/{id}.zip"),
            archive_name: None,
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
        cutover_for_local_config(op, root, &Config::default())
    }
    fn cutover_for_local_config(
        op: Operator,
        root: &Path,
        local_config: &Config,
    ) -> CloudLibraryCutover {
        let device_id = "current-device".to_string();
        let mut current_profile = ConfigurationOwners::from_legacy(local_config, &device_id)
            .device_profiles
            .remove(&device_id)
            .unwrap();
        current_profile.local_archive_root = Some("current-device-private-root".into());
        let mut runner = CloudLibraryCutover::new(
            op,
            root.join("local"),
            root.join("progress.json"),
            device_id,
            current_profile,
            2,
        );
        runner.source_retry_delay = Duration::ZERO;
        runner
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
        let result = runner.execute(false).await.unwrap();
        assert_eq!(result.snapshot_count, 2);
        assert!(result.device_profiles["current-device"].games.is_empty());
        let published: DeviceProfile = serde_json::from_slice(
            &op.read(&device_profile_path("current-device"))
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(published.games.is_empty());
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
    async fn source_archive_that_arrives_late_is_not_recorded_as_unavailable() {
        let op = operator();
        let root = temp_dir::TempDir::new().unwrap();
        let bytes = b"uploaded after its metadata";
        let late = snapshot("late", None, bytes);
        let mut catalog = GameSnapshots::new("test-game");
        catalog.backups.push(late.clone());
        seed(&op, catalog, &[]).await;
        let mut runner = cutover(op.clone(), root.path());
        runner.source_retry_delay = Duration::from_millis(100);
        let upload_path = game_cloud_archive_path("test-game", &late).unwrap();
        let writer = op.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            writer.write(&upload_path, bytes.to_vec()).await.unwrap();
        });

        let result = runner.execute(false).await.unwrap();

        assert_eq!(result.unavailable_archives, 0);
    }
    #[tokio::test]
    async fn source_archive_that_never_arrives_is_recorded_after_the_attempts() {
        let op = operator();
        let root = temp_dir::TempDir::new().unwrap();
        let absent = snapshot("absent", None, b"never uploaded");
        let mut catalog = GameSnapshots::new("test-game");
        catalog.backups.push(absent);
        seed(&op, catalog, &[]).await;

        let result = cutover(op, root.path()).execute(false).await.unwrap();

        assert_eq!(result.unavailable_archives, 1);
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
        let result = runner.execute(false).await.unwrap();
        assert_eq!(result.unavailable_archives, 0);
        assert_eq!(std::fs::read_dir(runner.staging_root()).unwrap().count(), 0);
        let v2_path = cloud_archive_path("test-game", "cloud", ArchiveFormat::Zip, None).unwrap();
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
        let v2_path = cloud_archive_path("test-game", "done", ArchiveFormat::Zip, None).unwrap();
        op.write(&v2_path, bytes.to_vec()).await.unwrap();
        op.delete(V1_CONFIG_PATH).await.unwrap();
        op.delete(&game_cloud_archive_path("test-game", &snapshot).unwrap())
            .await
            .unwrap();
        let result = runner.execute(false).await.unwrap();
        assert_eq!(result.snapshot_count, 1);
        assert_eq!(op.read(&v2_path).await.unwrap().to_vec(), bytes);
        assert!(op.read(V2_NAMESPACE_DESCRIPTOR_PATH).await.is_ok());
    }
    #[tokio::test]
    async fn does_not_publish_over_a_library_another_device_published_meanwhile() {
        let op = operator();
        let (first_root, second_root) = (
            temp_dir::TempDir::new().unwrap(),
            temp_dir::TempDir::new().unwrap(),
        );
        let bytes = b"shared archive";
        let snapshot = snapshot("one", None, bytes);
        let mut catalog = GameSnapshots::new("test-game");
        catalog.backups.push(snapshot.clone());
        seed(&op, catalog, &[(&snapshot, bytes)]).await;
        let first = cutover(op.clone(), first_root.path());
        let second = cutover(op.clone(), second_root.path());
        let mut late_plan = second.freeze_plan().await.unwrap();
        let result = second
            .migrate_archive("test-game", &snapshot)
            .await
            .unwrap();
        late_plan.games[0].results.insert("one".into(), result);
        first.execute(false).await.unwrap();
        let published = op.read(SHARED_LIBRARY_PATH).await.unwrap().to_vec();
        let descriptor = op
            .read(V2_NAMESPACE_DESCRIPTOR_PATH)
            .await
            .unwrap()
            .to_vec();

        let error = second.publish_unless_claimed(&late_plan).await.unwrap_err();

        assert!(matches!(
            error,
            CloudLibraryCutoverError::FinalVerificationMismatch
        ));
        assert_eq!(
            op.read(SHARED_LIBRARY_PATH).await.unwrap().to_vec(),
            published
        );
        assert_eq!(
            op.read(V2_NAMESPACE_DESCRIPTOR_PATH)
                .await
                .unwrap()
                .to_vec(),
            descriptor
        );
    }
    async fn published_retention(op: &Operator) -> Option<u32> {
        let library: SharedLibrary =
            serde_json::from_slice(&op.read(SHARED_LIBRARY_PATH).await.unwrap().to_vec()).unwrap();
        library.games[0]
            .snapshot_retention
            .map(|policy| policy.automatic_snapshots_per_branch)
    }
    #[tokio::test]
    async fn automatic_cleanup_is_offered_and_written_only_after_the_player_agrees() {
        let local = |global: u32, per_game: Option<u32>| {
            let mut config = Config {
                games: vec![game()],
                ..Config::default()
            };
            config.settings.max_auto_backup_count = global;
            config.games[0].auto_backup_limit = per_game;
            config
        };
        for (config, expected) in [
            (local(5, None), Some(5)),
            (local(5, Some(2)), Some(2)),
            (local(5, Some(0)), None),
            (local(0, None), None),
        ] {
            for apply in [false, true] {
                let op = operator();
                let root = temp_dir::TempDir::new().unwrap();
                seed(&op, GameSnapshots::new("test-game"), &[]).await;
                let runner = cutover_for_local_config(op.clone(), root.path(), &config);
                let review = runner.review(&no_local_games()).await.unwrap();
                assert_eq!(
                    review
                        .automatic_cleanup
                        .iter()
                        .map(|game| (game.game_id.as_str(), game.automatic_snapshots_per_branch))
                        .collect::<Vec<_>>(),
                    expected.map_or(vec![], |limit| vec![("test-game", limit)])
                );
                runner.execute(apply).await.unwrap();
                assert_eq!(
                    published_retention(&op).await,
                    expected.filter(|_| apply),
                    "apply={apply}"
                );
            }
        }
    }
    #[tokio::test]
    async fn resumed_cutover_keeps_its_automatic_cleanup_choice() {
        let op = operator();
        let root = temp_dir::TempDir::new().unwrap();
        let bytes = b"one";
        let first = snapshot("one", None, bytes);
        let second = snapshot("two", Some("one"), b"two");
        let mut catalog = GameSnapshots::new("test-game");
        catalog.backups = vec![first.clone(), second.clone()];
        seed(&op, catalog, &[(&first, bytes), (&second, b"two")]).await;
        let mut config = Config {
            games: vec![game()],
            ..Config::default()
        };
        config.settings.max_auto_backup_count = 4;
        let runner = cutover_for_local_config(op.clone(), root.path(), &config);
        let mut plan = runner.freeze_plan().await.unwrap();
        plan.automatic_cleanup_offer.clear();
        runner.store_progress(&plan).await.unwrap();
        assert!(
            runner
                .review(&no_local_games())
                .await
                .unwrap()
                .automatic_cleanup
                .is_empty()
        );
        runner.execute(true).await.unwrap();
        assert_eq!(published_retention(&op).await, None);
    }
    #[tokio::test]
    async fn review_names_history_and_games_the_old_cloud_configuration_does_not_list() {
        let op = operator();
        let root = temp_dir::TempDir::new().unwrap();
        let listed = snapshot("listed", None, b"listed");
        let mut catalog = GameSnapshots::new("test-game");
        catalog.backups.push(listed.clone());
        seed(&op, catalog, &[(&listed, b"listed")]).await;
        let mut stray = GameSnapshots::new("Stray Game");
        stray.backups = vec![snapshot("a", None, b"a"), snapshot("b", Some("a"), b"b")];
        for (key, bytes) in [
            ("Stray Game", serde_json::to_vec(&stray).unwrap()),
            (
                "Empty Game",
                serde_json::to_vec(&GameSnapshots::new("Empty Game")).unwrap(),
            ),
            ("Broken Game", b"{broken".to_vec()),
        ] {
            op.write(&game_cloud_metadata_path(key).unwrap(), bytes)
                .await
                .unwrap();
        }
        let local = ConfigurationOwners::from_legacy(
            &Config {
                games: vec![
                    game(),
                    Game {
                        name: "Local Only".into(),
                        storage_key: "local-only".into(),
                        ..game()
                    },
                ],
                ..Config::default()
            },
            &"current-device".to_string(),
        )
        .shared_library;
        let runner = cutover(op.clone(), root.path());

        let review = runner.review(&local).await.unwrap();

        assert_eq!(
            review.unlisted_cloud_games,
            vec![CutoverUnlistedGame {
                game_id: "Stray Game".into(),
                snapshot_count: 2
            }]
        );
        assert_eq!(review.local_only_games, vec!["Local Only".to_string()]);
        assert_eq!(review.game_count, 1);
    }
    #[tokio::test]
    async fn restarts_over_unpublished_v2_objects_when_progress_is_lost() {
        let op = operator();
        let root = temp_dir::TempDir::new().unwrap();
        let bytes = b"uploaded before the interruption";
        let snapshot = snapshot("done", None, bytes);
        let mut catalog = GameSnapshots::new("test-game");
        catalog.backups.push(snapshot.clone());
        seed(&op, catalog, &[(&snapshot, bytes)]).await;
        let v2_path = cloud_archive_path("test-game", "done", ArchiveFormat::Zip, None).unwrap();
        op.write(&v2_path, bytes.to_vec()).await.unwrap();
        let runner = cutover(op.clone(), root.path());
        assert!(!root.path().join("progress.json").exists());
        let result = runner.execute(false).await.unwrap();
        assert_eq!(result.snapshot_count, 1);
        assert_eq!(result.unavailable_archives, 0);
        assert!(op.read(V2_NAMESPACE_DESCRIPTOR_PATH).await.is_ok());
    }
    #[tokio::test]
    async fn partial_v2_without_legacy_configuration_is_not_restarted() {
        let op = operator();
        let root = temp_dir::TempDir::new().unwrap();
        op.write("v2/archives/orphan.zip", b"x".to_vec())
            .await
            .unwrap();
        assert!(matches!(
            cutover(op, root.path()).review(&no_local_games()).await,
            Err(CloudLibraryCutoverError::CutoverUnavailable)
        ));
    }

    #[test]
    fn interrupt_failpoint_is_optional_and_accepts_positive_counts() {
        assert_eq!(parse_interrupt_after_archives(None).unwrap(), None);
        assert_eq!(parse_interrupt_after_archives(Some("1")).unwrap(), Some(1));
    }

    #[test]
    fn invalid_interrupt_failpoint_is_a_hard_error() {
        for value in ["0", "-1", "nope", ""] {
            assert!(matches!(
                parse_interrupt_after_archives(Some(value)),
                Err(CloudLibraryCutoverError::InvalidCutoverInterruptFailpoint)
            ));
        }
    }
}
