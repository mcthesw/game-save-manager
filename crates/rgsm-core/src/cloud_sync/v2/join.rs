use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;
use tokio::sync::Mutex;

use super::{
    CloudLibraryTransport, CloudNamespaceClassification, CloudNamespaceClassifier,
    CloudNamespaceError, GameJoinClassification, OpenDalNamespaceTransport, SHARED_LIBRARY_PATH,
    compare_join_libraries, device_profile_path,
};
use crate::config::{DeviceProfile, OwnershipError, SharedGame, SharedLibrary};

static JOIN_WRITER_LOCK: Mutex<()> = Mutex::const_new(());

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CloudLibraryJoinReview {
    pub cloud_game_count: usize,
    pub items: Vec<CloudLibraryJoinItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CloudLibraryJoinItem {
    pub local_game_id: String,
    pub local_name: String,
    pub local_fingerprint: String,
    pub cloud_names: Vec<String>,
    pub cloud_fingerprint: Option<String>,
    pub classification: GameJoinClassification,
    pub difference: GameDefinitionDifference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct GameDefinitionDifference {
    pub name_changed: bool,
    pub local_save_unit_count: usize,
    pub cloud_save_unit_count: usize,
    pub save_units_changed: bool,
    pub local_recognition: bool,
    pub cloud_recognition: bool,
    pub recognition_changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum JoinGameAction {
    KeepCloud,
    AddLocal,
    ReplaceCloud,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct JoinGameDecision {
    pub local_game_id: String,
    pub local_fingerprint: String,
    pub cloud_fingerprint: Option<String>,
    pub action: JoinGameAction,
}

pub struct CloudLibraryJoin<T: CloudLibraryTransport> {
    transport: Arc<T>,
    max_attempts: usize,
}

impl CloudLibraryJoin<OpenDalNamespaceTransport> {
    pub fn new(operator: opendal::Operator, max_attempts: usize) -> Self {
        Self::with_transport(OpenDalNamespaceTransport::new(operator), max_attempts)
    }
}

impl<T: CloudLibraryTransport> CloudLibraryJoin<T> {
    pub fn with_transport(transport: T, max_attempts: usize) -> Self {
        Self {
            transport: Arc::new(transport),
            max_attempts: max_attempts.max(1),
        }
    }

    pub async fn review(
        &self,
        local: &SharedLibrary,
    ) -> Result<CloudLibraryJoinReview, CloudLibraryJoinError> {
        let cloud = self.load_supported().await?;
        build_review(local, &cloud)
    }

    pub async fn join(
        &self,
        local: &SharedLibrary,
        local_profile: &DeviceProfile,
        decisions: &[JoinGameDecision],
        confirmed_replacements: bool,
    ) -> Result<(SharedLibrary, DeviceProfile), CloudLibraryJoinError> {
        let _guard = JOIN_WRITER_LOCK.lock().await;
        validate_decisions(local, decisions, confirmed_replacements)?;

        for _ in 0..self.max_attempts {
            let latest = self.load_supported().await?;
            let accepted = apply_decisions(local, latest, decisions)?;
            let expected = serde_json::to_vec_pretty(&accepted)?;
            if decisions
                .iter()
                .any(|decision| decision.action != JoinGameAction::KeepCloud)
            {
                self.transport.write(SHARED_LIBRARY_PATH, &expected).await?;
                if self.transport.read(SHARED_LIBRARY_PATH).await?.as_deref()
                    != Some(expected.as_slice())
                {
                    continue;
                }
            }

            let profile = local_profile.for_shared_library(&accepted);
            let profile_path = device_profile_path(&profile.device.id);
            let profile_bytes = serde_json::to_vec_pretty(&profile)?;
            self.write_verified(&profile_path, &profile_bytes).await?;
            return Ok((accepted, profile));
        }
        Err(CloudLibraryJoinError::WriteVerificationFailed {
            path: SHARED_LIBRARY_PATH.to_string(),
            attempts: self.max_attempts,
        })
    }

    async fn load_supported(&self) -> Result<SharedLibrary, CloudLibraryJoinError> {
        match CloudNamespaceClassifier::with_transport(self.transport.clone())
            .classify()
            .await?
        {
            CloudNamespaceClassification::SupportedV2 { shared_library, .. } => Ok(shared_library),
            CloudNamespaceClassification::V1Only { .. } => {
                Err(CloudLibraryJoinError::JoinUnavailable("legacy_v1"))
            }
            CloudNamespaceClassification::Empty => {
                Err(CloudLibraryJoinError::JoinUnavailable("empty"))
            }
        }
    }

    async fn write_verified(&self, path: &str, bytes: &[u8]) -> Result<(), CloudLibraryJoinError> {
        for _ in 0..self.max_attempts {
            self.transport.write(path, bytes).await?;
            if self.transport.read(path).await?.as_deref() == Some(bytes) {
                return Ok(());
            }
        }
        Err(CloudLibraryJoinError::WriteVerificationFailed {
            path: path.to_string(),
            attempts: self.max_attempts,
        })
    }
}

fn build_review(
    local: &SharedLibrary,
    cloud: &SharedLibrary,
) -> Result<CloudLibraryJoinReview, CloudLibraryJoinError> {
    let items = compare_join_libraries(local, cloud)?
        .into_iter()
        .map(|candidate| {
            let cloud = candidate.cloud_candidates.first();
            Ok(CloudLibraryJoinItem {
                local_game_id: candidate.local.storage_key.clone(),
                local_name: candidate.local.name.clone(),
                local_fingerprint: candidate.local.portable_fingerprint()?,
                cloud_names: candidate
                    .cloud_candidates
                    .iter()
                    .map(|game| game.name.clone())
                    .collect(),
                cloud_fingerprint: cloud.map(|game| game.portable_fingerprint()).transpose()?,
                classification: candidate.classification,
                difference: difference(&candidate.local, cloud),
            })
        })
        .collect::<Result<Vec<_>, CloudLibraryJoinError>>()?;
    Ok(CloudLibraryJoinReview {
        cloud_game_count: cloud.games.len(),
        items,
    })
}

fn difference(local: &SharedGame, cloud: Option<&SharedGame>) -> GameDefinitionDifference {
    GameDefinitionDifference {
        name_changed: cloud.is_some_and(|game| game.name != local.name),
        local_save_unit_count: local.save_units.len(),
        cloud_save_unit_count: cloud.map_or(0, |game| game.save_units.len()),
        save_units_changed: cloud.is_none_or(|game| {
            game.normalized_portable().save_units != local.normalized_portable().save_units
        }),
        local_recognition: local.ludusavi_meta.is_some(),
        cloud_recognition: cloud.is_some_and(|game| game.ludusavi_meta.is_some()),
        recognition_changed: cloud.is_none_or(|game| game.ludusavi_meta != local.ludusavi_meta),
    }
}

fn validate_decisions(
    local: &SharedLibrary,
    decisions: &[JoinGameDecision],
    confirmed_replacements: bool,
) -> Result<(), CloudLibraryJoinError> {
    local.validate()?;
    let local_ids = local
        .games
        .iter()
        .map(|game| game.storage_key.as_str())
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    for decision in decisions {
        if !local_ids.contains(decision.local_game_id.as_str())
            || !seen.insert(decision.local_game_id.as_str())
        {
            return Err(CloudLibraryJoinError::InvalidDecision(
                decision.local_game_id.clone(),
            ));
        }
        if decision.action == JoinGameAction::ReplaceCloud && !confirmed_replacements {
            return Err(CloudLibraryJoinError::ReplacementConfirmationRequired);
        }
    }
    Ok(())
}

fn apply_decisions(
    local: &SharedLibrary,
    mut latest: SharedLibrary,
    decisions: &[JoinGameDecision],
) -> Result<SharedLibrary, CloudLibraryJoinError> {
    let local_by_id = local
        .games
        .iter()
        .map(|game| (game.storage_key.as_str(), game))
        .collect::<HashMap<_, _>>();
    for decision in decisions {
        let local_game = local_by_id[decision.local_game_id.as_str()];
        if local_game.portable_fingerprint()? != decision.local_fingerprint {
            return Err(CloudLibraryJoinError::LocalGameChanged(
                local_game.name.clone(),
            ));
        }
        let cloud_index = latest
            .games
            .iter()
            .position(|game| game.storage_key == decision.local_game_id);
        match decision.action {
            JoinGameAction::KeepCloud => {}
            JoinGameAction::AddLocal => match cloud_index {
                None => latest.games.push(local_game.clone()),
                Some(_) => {
                    return Err(CloudLibraryJoinError::InvalidDecision(
                        decision.local_game_id.clone(),
                    ));
                }
            },
            JoinGameAction::ReplaceCloud => {
                let Some(index) = cloud_index else {
                    return Err(CloudLibraryJoinError::TargetChanged(
                        local_game.name.clone(),
                    ));
                };
                let expected = decision.cloud_fingerprint.as_deref();
                let actual = latest.games[index].portable_fingerprint()?;
                if expected.is_none() || Some(actual.as_str()) != expected {
                    return Err(CloudLibraryJoinError::TargetChanged(
                        local_game.name.clone(),
                    ));
                }
                latest.games[index] = local_game.clone();
            }
        }
    }
    latest.games.sort_by(|left, right| {
        left.storage_key
            .cmp(&right.storage_key)
            .then_with(|| left.name.cmp(&right.name))
    });
    latest.validate()?;
    Ok(latest)
}

#[derive(Debug, Error)]
pub enum CloudLibraryJoinError {
    #[error(transparent)]
    Namespace(#[from] CloudNamespaceError),
    #[error(transparent)]
    Comparison(#[from] super::GameJoinComparisonError),
    #[error(transparent)]
    Ownership(#[from] OwnershipError),
    #[error("Cloud Library serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Cloud Library transport error: {0}")]
    Transport(#[from] opendal::Error),
    #[error("Cloud Library join is unavailable for root state: {0}")]
    JoinUnavailable(&'static str),
    #[error("Invalid join decision for Game: {0}")]
    InvalidDecision(String),
    #[error("Local Game changed during join review: {0}")]
    LocalGameChanged(String),
    #[error("Cloud Game changed during join review: {0}")]
    TargetChanged(String),
    #[error("Replacing a shared Game requires explicit confirmation")]
    ReplacementConfirmationRequired,
    #[error("Cloud object {path} did not match after {attempts} write attempts")]
    WriteVerificationFailed { path: String, attempts: usize },
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex as StdMutex;

    use async_trait::async_trait;

    use super::*;
    use crate::cloud_sync::v2::{
        CLOUD_MANIFEST_PATH, CloudManifest, CloudNamespaceDescriptor, NamespaceTransport,
        V2_NAMESPACE_DESCRIPTOR_PATH,
    };
    use crate::config::{
        ConfigurationOwners, SharedSaveUnit, SharedSaveUnitSource, V2_CONFIG_SCHEMA_VERSION,
    };

    #[derive(Default)]
    struct FakeTransport {
        objects: StdMutex<BTreeMap<String, Vec<u8>>>,
        writes: StdMutex<Vec<String>>,
    }

    #[async_trait]
    impl NamespaceTransport for FakeTransport {
        async fn read(&self, path: &str) -> Result<Option<Vec<u8>>, opendal::Error> {
            Ok(self.objects.lock().unwrap().get(path).cloned())
        }

        async fn list_sample(
            &self,
            prefix: &str,
            limit: usize,
        ) -> Result<Vec<String>, opendal::Error> {
            Ok(self
                .objects
                .lock()
                .unwrap()
                .keys()
                .filter(|path| prefix == "." || path.starts_with(prefix))
                .take(limit)
                .cloned()
                .collect())
        }
    }

    #[async_trait]
    impl CloudLibraryTransport for FakeTransport {
        async fn write(&self, path: &str, bytes: &[u8]) -> Result<(), opendal::Error> {
            self.writes.lock().unwrap().push(path.to_string());
            self.objects
                .lock()
                .unwrap()
                .insert(path.to_string(), bytes.to_vec());
            Ok(())
        }
    }

    fn game(id: &str, name: &str, unit_id: u32) -> SharedGame {
        SharedGame {
            name: name.into(),
            storage_key: id.into(),
            save_units: vec![SharedSaveUnit {
                id: unit_id,
                source: SharedSaveUnitSource::Concrete {
                    unit_type: crate::backup::SaveUnitType::Folder,
                },
            }],
            next_save_unit_id: unit_id + 1,
            ludusavi_meta: None,
        }
    }

    fn library(games: Vec<SharedGame>) -> SharedLibrary {
        SharedLibrary {
            schema_version: V2_CONFIG_SCHEMA_VERSION,
            games,
        }
    }

    fn transport(cloud: &SharedLibrary) -> FakeTransport {
        let transport = FakeTransport::default();
        let mut objects = transport.objects.lock().unwrap();
        objects.insert(
            V2_NAMESPACE_DESCRIPTOR_PATH.into(),
            serde_json::to_vec(&CloudNamespaceDescriptor::default()).unwrap(),
        );
        objects.insert(
            CLOUD_MANIFEST_PATH.into(),
            serde_json::to_vec(&CloudManifest::default()).unwrap(),
        );
        objects.insert(
            SHARED_LIBRARY_PATH.into(),
            serde_json::to_vec(cloud).unwrap(),
        );
        drop(objects);
        transport
    }

    fn profile(local: &SharedLibrary) -> DeviceProfile {
        let config = crate::config::Config {
            games: local
                .games
                .iter()
                .map(|game| crate::backup::Game {
                    name: game.name.clone(),
                    storage_key: game.storage_key.clone(),
                    save_paths: Vec::new(),
                    game_paths: HashMap::new(),
                    next_save_unit_id: game.next_save_unit_id,
                    cloud_sync_enabled: false,
                    auto_backup: None,
                    ludusavi_meta: None,
                    device_bindings: HashMap::new(),
                })
                .collect(),
            ..Default::default()
        };
        let owners = ConfigurationOwners::from_legacy(&config, &"device".into());
        owners.device_profiles["device"].clone()
    }

    #[tokio::test]
    async fn review_is_read_only_and_classifies_all_local_games() {
        let mut conflict = game("conflict", "Conflict", 1);
        let cloud_conflict = conflict.clone();
        conflict.name = "Local Conflict".into();
        let local = library(vec![
            game("same", "Same", 1),
            game("local", "Local", 1),
            game("duplicate", "Duplicate", 1),
            conflict,
        ]);
        let cloud = library(vec![
            game("same", "Same", 1),
            game("remote-duplicate", "Duplicate", 1),
            cloud_conflict,
        ]);
        let join = CloudLibraryJoin::with_transport(transport(&cloud), 2);

        let review = join.review(&local).await.unwrap();

        assert_eq!(review.items.len(), 4);
        assert_eq!(
            review
                .items
                .iter()
                .map(|item| item.classification)
                .collect::<Vec<_>>(),
            vec![
                GameJoinClassification::GameDefinitionConflict,
                GameJoinClassification::PossibleDuplicate,
                GameJoinClassification::LocalOnly,
                GameJoinClassification::Same,
            ]
        );
        assert!(join.transport.writes.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn join_adds_and_replaces_whole_games_then_publishes_profile() {
        let local_only = game("local", "Local", 1);
        let replacement = game("conflict", "Local Conflict", 2);
        let cloud_conflict = game("conflict", "Cloud Conflict", 1);
        let local = library(vec![local_only.clone(), replacement.clone()]);
        let cloud = library(vec![cloud_conflict.clone(), game("remote", "Remote", 1)]);
        let review = build_review(&local, &cloud).unwrap();
        let decisions = review
            .items
            .iter()
            .map(|item| JoinGameDecision {
                local_game_id: item.local_game_id.clone(),
                local_fingerprint: item.local_fingerprint.clone(),
                cloud_fingerprint: item.cloud_fingerprint.clone(),
                action: if item.local_game_id == "local" {
                    JoinGameAction::AddLocal
                } else {
                    JoinGameAction::ReplaceCloud
                },
            })
            .collect::<Vec<_>>();
        let join = CloudLibraryJoin::with_transport(transport(&cloud), 2);

        let (accepted, _) = join
            .join(&local, &profile(&local), &decisions, true)
            .await
            .unwrap();

        assert!(accepted.games.contains(&local_only));
        assert!(accepted.games.contains(&replacement));
        assert!(
            accepted
                .games
                .iter()
                .any(|game| game.storage_key == "remote")
        );
        assert_eq!(
            join.transport.writes.lock().unwrap().as_slice(),
            [SHARED_LIBRARY_PATH, device_profile_path("device").as_str()]
        );
    }

    #[tokio::test]
    async fn stale_replacement_does_not_write() {
        let local = library(vec![game("game", "Local", 2)]);
        let reviewed_cloud = library(vec![game("game", "Cloud", 1)]);
        let review = build_review(&local, &reviewed_cloud).unwrap();
        let decision = JoinGameDecision {
            local_game_id: "game".into(),
            local_fingerprint: review.items[0].local_fingerprint.clone(),
            cloud_fingerprint: review.items[0].cloud_fingerprint.clone(),
            action: JoinGameAction::ReplaceCloud,
        };
        let changed_cloud = library(vec![game("game", "Changed Again", 3)]);
        let join = CloudLibraryJoin::with_transport(transport(&changed_cloud), 2);

        assert!(matches!(
            join.join(&local, &profile(&local), &[decision], true)
                .await,
            Err(CloudLibraryJoinError::TargetChanged(name)) if name == "Local"
        ));
        assert!(join.transport.writes.lock().unwrap().is_empty());
    }
}
